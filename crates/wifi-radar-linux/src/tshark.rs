//! Parse tshark probe-request field output (parity with Python
//! `scan/tshark.py`).
//!
//! The capture command prints one tab-separated row per probe request:
//! `wlan.sa`, `wlan.ssid`, `radiotap.dbm_antsignal`. Probe requests come from
//! stations that are *not* associated to an AP, so every MAC becomes an
//! [`DeviceKind::Adapter`] carrying the SSIDs it asked for.
//! [`sample_tshark_probes`] is the live passive listen. It is optional: a
//! missing `tshark` yields no adapters. The filter is probe requests only.

use std::process::Command;
use std::time::Duration;

use wifi_radar_core::{normalize_mac, DeviceKind, ScanError, WifiDevice};

/// How long the Python worker lets tshark listen (`run_tshark_probes` default).
pub const TSHARK_SAMPLE: Duration = Duration::from_secs(2);

/// RSSI used when tshark prints no (or an unparsable) signal field.
const DEFAULT_PROBE_DBM: f64 = -85.0;

/// Parse tab-separated tshark field output into probe-only adapters.
///
/// Rows for the same MAC are folded into one device: probed SSIDs are
/// de-duplicated in first-seen order and the strongest RSSI wins. A wildcard
/// probe (empty SSID) still registers the adapter but adds no SSID. When a
/// multi-antenna radiotap header yields several comma-separated values the
/// strongest one is used (Python treated that as unparsable).
pub fn parse_tshark_probes(text: &str) -> Vec<WifiDevice> {
    let mut order: Vec<String> = Vec::new();
    let mut by_mac: std::collections::HashMap<String, WifiDevice> =
        std::collections::HashMap::new();

    for line in text.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        let sa = parts[0].trim();
        if sa.is_empty() {
            continue;
        }
        let mac = normalize_mac(sa);
        let ssid = parts.get(1).map(|s| s.trim()).unwrap_or("");
        let rssi = parts
            .get(2)
            .and_then(|s| parse_signal(s))
            .unwrap_or(DEFAULT_PROBE_DBM);

        let dev = by_mac.entry(mac.clone()).or_insert_with(|| {
            order.push(mac.clone());
            let mut d = WifiDevice::hotspot(mac.clone(), rssi);
            d.kind = DeviceKind::Adapter;
            d
        });
        if rssi > dev.rssi_dbm {
            dev.rssi_dbm = rssi;
        }
        if !ssid.is_empty() && !dev.probe_ssids.iter().any(|s| s == ssid) {
            dev.probe_ssids.push(ssid.to_string());
        }
    }

    order
        .into_iter()
        .filter_map(|mac| by_mac.remove(&mac))
        .collect()
}

/// Argv for a passive tshark probe-request sample (no shell, no injection).
pub fn probe_args(mon_iface: &str, seconds: u64) -> Vec<String> {
    vec![
        "-i".into(),
        mon_iface.into(),
        "-a".into(),
        format!("duration:{seconds}"),
        "-Y".into(),
        "wlan.fc.type_subtype == 4".into(),
        "-T".into(),
        "fields".into(),
        "-e".into(),
        "wlan.sa".into(),
        "-e".into(),
        "wlan.ssid".into(),
        "-e".into(),
        "radiotap.dbm_antsignal".into(),
    ]
}

/// Capture probe requests on `mon_iface` for about `sample_for`.
///
/// Returns an empty list when `tshark` is not on `PATH` (Python's worker
/// does the same). A non-zero tshark status still parses stdout: an idle
/// interface often exits without packets. Needs root and a monitor VIF at
/// runtime; unit tests do not open a radio.
pub fn sample_tshark_probes(
    mon_iface: &str,
    sample_for: Duration,
) -> Result<Vec<WifiDevice>, ScanError> {
    sample_with(
        "tshark",
        mon_iface,
        sample_for,
        crate::proc::tool_on_path("tshark"),
    )
}

pub(crate) fn sample_with(
    program: &str,
    mon_iface: &str,
    sample_for: Duration,
    present: bool,
) -> Result<Vec<WifiDevice>, ScanError> {
    if !crate::monitor::valid_iface_name(mon_iface) {
        return Err(ScanError::Message(format!(
            "invalid interface name: {mon_iface:?}"
        )));
    }
    if !present {
        return Ok(Vec::new());
    }
    let secs = sample_for.as_secs().max(1);
    let mut cmd = Command::new(program);
    cmd.args(probe_args(mon_iface, secs));
    match crate::proc::run_with_timeout(&mut cmd, Duration::from_secs(secs + 10)) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(ScanError::Message(format!("{program}: {e}"))),
        Ok(out) => Ok(parse_tshark_probes(&String::from_utf8_lossy(&out.stdout))),
    }
}

/// Strongest value of a possibly multi-valued `radiotap.dbm_antsignal`.
fn parse_signal(field: &str) -> Option<f64> {
    field
        .split(',')
        .filter_map(|v| v.trim().parse::<f64>().ok())
        .fold(None, |best, v| match best {
            Some(b) if b >= v => Some(b),
            _ => Some(v),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../tests/fixtures/tshark_probes.tsv");

    fn find<'a>(devs: &'a [WifiDevice], mac: &str) -> &'a WifiDevice {
        devs.iter().find(|d| d.mac == mac).expect("device present")
    }

    #[test]
    fn folds_rows_per_mac_in_first_seen_order() {
        let devs = parse_tshark_probes(FIXTURE);
        assert_eq!(devs.len(), 5);
        assert_eq!(devs[0].mac, "de:ad:00:00:00:01");
        assert_eq!(devs[1].mac, "de:ad:00:00:00:02");
    }

    #[test]
    fn probe_ssids_deduplicated_and_ordered() {
        let devs = parse_tshark_probes(FIXTURE);
        let d = find(&devs, "de:ad:00:00:00:01");
        assert_eq!(d.kind, DeviceKind::Adapter);
        assert_eq!(d.probe_ssids, vec!["FreeWifi", "CoffeeShop"]);
    }

    #[test]
    fn strongest_rssi_wins() {
        let devs = parse_tshark_probes(FIXTURE);
        let d = find(&devs, "de:ad:00:00:00:01");
        assert!((d.rssi_dbm - (-58.0)).abs() < 1e-9);
    }

    #[test]
    fn wildcard_probe_registers_adapter_without_ssid() {
        let devs = parse_tshark_probes(FIXTURE);
        let d = find(&devs, "de:ad:00:00:00:02");
        assert!(d.probe_ssids.is_empty());
        assert!((d.rssi_dbm - (-75.0)).abs() < 1e-9);
    }

    #[test]
    fn multi_antenna_signal_takes_strongest() {
        let devs = parse_tshark_probes(FIXTURE);
        let d = find(&devs, "de:ad:00:00:00:03");
        assert!((d.rssi_dbm - (-40.0)).abs() < 1e-9);
    }

    #[test]
    fn missing_signal_uses_default() {
        let devs = parse_tshark_probes(FIXTURE);
        assert!((find(&devs, "de:ad:00:00:00:04").rssi_dbm - DEFAULT_PROBE_DBM).abs() < 1e-9);
        // Row with only a source address (no tabs at all).
        let bare = find(&devs, "de:ad:00:00:00:05");
        assert!((bare.rssi_dbm - DEFAULT_PROBE_DBM).abs() < 1e-9);
        assert!(bare.probe_ssids.is_empty());
    }

    #[test]
    fn empty_and_blank_input_yield_nothing() {
        assert!(parse_tshark_probes("").is_empty());
        assert!(parse_tshark_probes("\n\n  \n").is_empty());
    }

    #[test]
    fn probe_argv_is_passive_probe_request_filter() {
        let args = probe_args("wlan0mon", 2);
        assert_eq!(
            args,
            [
                "-i",
                "wlan0mon",
                "-a",
                "duration:2",
                "-Y",
                "wlan.fc.type_subtype == 4",
                "-T",
                "fields",
                "-e",
                "wlan.sa",
                "-e",
                "wlan.ssid",
                "-e",
                "radiotap.dbm_antsignal",
            ]
        );
    }

    #[test]
    fn sample_rejects_unsafe_iface_without_spawning() {
        let err =
            sample_tshark_probes("wl an0", std::time::Duration::from_secs(1)).expect_err("reject");
        assert!(err.to_string().contains("invalid interface"));
    }

    #[test]
    fn absent_tshark_yields_no_adapters() {
        let devs = sample_with(
            "wifi-radar-no-such-program",
            "wlan0mon",
            std::time::Duration::from_millis(50),
            false,
        )
        .expect("missing tshark is empty, not an error");
        assert!(devs.is_empty());
    }
}
