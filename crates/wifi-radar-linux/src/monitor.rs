//! Passive monitor-mode support (parity with Python `iface.MonitorSession`).
//!
//! [`MonitorSession`] owns a `{phy}mon` virtual interface created next to the
//! managed one, so nl80211 scans on the original interface keep working while
//! airodump-ng / tshark listen on the monitor VIF. Everything here is
//! passive: the interface only receives frames — no injection, deauth,
//! association or cracking. Creating the VIF needs root.

use std::path::Path;
use std::process::Command;
use std::time::Duration;

use wifi_radar_core::ScanError;

use crate::proc::run_with_timeout;

/// Timeout for the short `iw` / `ip` management commands (Python: 5 s).
const IFACE_CMD_TIMEOUT: Duration = Duration::from_secs(5);

/// Suffix appended to the managed interface name to form the monitor VIF.
pub const MON_SUFFIX: &str = "mon";

/// True when the process runs with effective uid 0.
///
/// Monitor VIF creation needs `CAP_NET_ADMIN`; root is the practical proxy
/// the Python app also uses (`os.geteuid() == 0`).
pub fn is_root() -> bool {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|text| parse_effective_uid(&text))
        .is_some_and(|uid| uid == 0)
}

/// Effective uid from the `Uid:` line of `/proc/<pid>/status`.
fn parse_effective_uid(status: &str) -> Option<u32> {
    // "Uid:\t<real>\t<effective>\t<saved>\t<fs>"
    let line = status.lines().find(|l| l.starts_with("Uid:"))?;
    line.split_whitespace().nth(2)?.parse().ok()
}

/// Interface names are passed as argv (never through a shell), but still
/// reject anything the kernel would not accept or that looks like an option.
fn valid_iface_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 15
        && !name.starts_with('-')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
}

fn iface_exists(name: &str) -> bool {
    Path::new("/sys/class/net").join(name).exists()
}

/// Create and destroy a monitor-mode virtual interface.
///
/// Unlike the Python class, [`teardown`](Self::teardown) only deletes a VIF
/// this session created itself, so a pre-existing `wlan0mon` set up by the
/// user (e.g. via airmon-ng) is reused but never removed. Dropping the session
/// tears down as well, so a panic does not leak the VIF.
#[derive(Debug)]
pub struct MonitorSession {
    phy_iface: String,
    mon_iface: String,
    created: bool,
}

impl MonitorSession {
    /// Session for the managed interface `phy_iface` (e.g. `wlan0`).
    pub fn new(phy_iface: impl Into<String>) -> Self {
        let phy_iface = phy_iface.into();
        let mon_iface = format!("{phy_iface}{MON_SUFFIX}");
        Self {
            phy_iface,
            mon_iface,
            created: false,
        }
    }

    /// Managed interface this session was created for.
    pub fn phy_iface(&self) -> &str {
        &self.phy_iface
    }

    /// Monitor VIF name (`{phy}mon`).
    pub fn mon_iface(&self) -> &str {
        &self.mon_iface
    }

    /// True when this session created the VIF (and will delete it).
    pub fn created(&self) -> bool {
        self.created
    }

    /// Argv for `iw dev <phy> interface add <mon> type monitor`.
    fn iw_add_args(&self) -> Vec<String> {
        [
            "dev",
            &self.phy_iface,
            "interface",
            "add",
            &self.mon_iface,
            "type",
            "monitor",
        ]
        .iter()
        .map(|s| (*s).to_string())
        .collect()
    }

    /// Argv for `ip link set <mon> up`.
    fn ip_up_args(&self) -> Vec<String> {
        ["link", "set", &self.mon_iface, "up"]
            .iter()
            .map(|s| (*s).to_string())
            .collect()
    }

    /// Argv for `iw dev <mon> del`.
    fn iw_del_args(&self) -> Vec<String> {
        ["dev", &self.mon_iface, "del"]
            .iter()
            .map(|s| (*s).to_string())
            .collect()
    }

    /// Create the monitor VIF if needed and bring it up.
    pub fn ensure(&mut self) -> Result<(), ScanError> {
        if !valid_iface_name(&self.phy_iface) || !valid_iface_name(&self.mon_iface) {
            return Err(ScanError::Message(format!(
                "invalid interface name for monitor VIF: {:?}",
                self.mon_iface
            )));
        }
        if iface_exists(&self.mon_iface) {
            // Reused as-is; a failed `up` here is not fatal (it may already be).
            let _ = run_tool("ip", &self.ip_up_args());
            return Ok(());
        }
        let add = run_tool("iw", &self.iw_add_args())?;
        if !add.status.success() {
            return Err(ScanError::Message(format!(
                "iw: could not add {} on {} (root required?): {}",
                self.mon_iface,
                self.phy_iface,
                String::from_utf8_lossy(&add.stderr).trim()
            )));
        }
        self.created = true;
        let up = run_tool("ip", &self.ip_up_args())?;
        if !up.status.success() {
            return Err(ScanError::Message(format!(
                "ip: could not bring {} up: {}",
                self.mon_iface,
                String::from_utf8_lossy(&up.stderr).trim()
            )));
        }
        Ok(())
    }

    /// Remove the monitor VIF if this session created it. Idempotent.
    pub fn teardown(&mut self) {
        if self.created && iface_exists(&self.mon_iface) {
            let _ = run_tool("iw", &self.iw_del_args());
        }
        self.created = false;
    }
}

impl Drop for MonitorSession {
    fn drop(&mut self) {
        self.teardown();
    }
}

fn run_tool(program: &str, args: &[String]) -> Result<std::process::Output, ScanError> {
    let mut cmd = Command::new(program);
    cmd.args(args);
    run_with_timeout(&mut cmd, IFACE_CMD_TIMEOUT)
        .map_err(|e| ScanError::Message(format!("{program}: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mon_iface_is_phy_plus_suffix() {
        let s = MonitorSession::new("wlan0");
        assert_eq!(s.phy_iface(), "wlan0");
        assert_eq!(s.mon_iface(), "wlan0mon");
        assert!(!s.created());
    }

    #[test]
    fn iw_add_argv_matches_python() {
        let s = MonitorSession::new("wlan0");
        assert_eq!(
            s.iw_add_args(),
            [
                "dev",
                "wlan0",
                "interface",
                "add",
                "wlan0mon",
                "type",
                "monitor"
            ]
        );
    }

    #[test]
    fn ip_up_and_iw_del_argv() {
        let s = MonitorSession::new("wlp3s0");
        assert_eq!(s.ip_up_args(), ["link", "set", "wlp3s0mon", "up"]);
        assert_eq!(s.iw_del_args(), ["dev", "wlp3s0mon", "del"]);
    }

    #[test]
    fn rejects_unsafe_names_without_spawning() {
        for bad in [
            "",
            "-h",
            "wl an0",
            "a/b",
            "wlan0;rm",
            "averyveryverylongname",
        ] {
            let mut s = MonitorSession::new(bad);
            let err = s.ensure().expect_err("must reject");
            assert!(
                err.to_string().contains("invalid interface name"),
                "{bad}: {err}"
            );
            assert!(!s.created());
        }
    }

    #[test]
    fn teardown_without_create_is_noop() {
        // Even if a same-named interface existed, we did not create it.
        let mut s = MonitorSession::new("lo");
        s.teardown();
        s.teardown();
        assert!(!s.created());
    }

    #[test]
    fn parses_effective_uid_from_proc_status() {
        let status = "Name:\tx\nUid:\t1000\t0\t1000\t1000\nGid:\t1\t1\t1\t1\n";
        assert_eq!(parse_effective_uid(status), Some(0));
        assert_eq!(
            parse_effective_uid("Uid:\t1000\t1000\t1000\t1000\n"),
            Some(1000)
        );
        assert_eq!(parse_effective_uid("Name:\tx\n"), None);
        assert_eq!(parse_effective_uid("Uid:\t1000\n"), None);
    }

    #[test]
    fn is_root_matches_proc_status() {
        let expect = std::fs::read_to_string("/proc/self/status")
            .ok()
            .and_then(|t| parse_effective_uid(&t))
            == Some(0);
        assert_eq!(is_root(), expect);
    }
}
