//! Interim `iw` CLI scan backend (parity with Python `scan/iw.py`).
//!
//! Native nl80211 will replace this as the default Linux path.

use std::process::Command;
use std::time::Duration;

use wifi_radar_core::{normalize_mac, DeviceKind, ScanBackend, ScanError, WifiDevice};

/// Parse and run `iw dev <iface> scan` (managed-mode BSS dump).
pub struct IwCliBackend {
    iface: String,
    timeout: Duration,
}

impl IwCliBackend {
    /// Create a backend for `iface` (e.g. `wlan0`).
    pub fn new(iface: impl Into<String>) -> Self {
        Self {
            iface: iface.into(),
            timeout: Duration::from_secs(30),
        }
    }

    /// Override the subprocess timeout.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Parse `iw … scan` text output into hotspot devices.
    pub fn parse_scan_text(text: &str) -> Vec<WifiDevice> {
        parse_iw_scan(text)
    }
}

impl ScanBackend for IwCliBackend {
    fn name(&self) -> &'static str {
        "iw-cli"
    }

    fn scan(&mut self) -> Result<Vec<WifiDevice>, ScanError> {
        let output = run_iw(&self.iface, false, self.timeout)?;
        if !output.status.success() {
            let dump = run_iw(&self.iface, true, self.timeout)?;
            if !dump.status.success() {
                let stderr = String::from_utf8_lossy(&dump.stderr);
                return Err(ScanError::Message(format!(
                    "iw scan failed on {}: {stderr}",
                    self.iface
                )));
            }
            let text = String::from_utf8_lossy(&dump.stdout);
            return Ok(parse_iw_scan(&text));
        }
        let text = String::from_utf8_lossy(&output.stdout);
        Ok(parse_iw_scan(&text))
    }
}

fn run_iw(
    iface: &str,
    dump: bool,
    _timeout: Duration,
) -> Result<std::process::Output, ScanError> {
    // Note: std::process::Command has no built-in timeout on stable without
    // extra crates; callers should prefer short iface scans. The Python app
    // used subprocess timeout=30; we keep the field for API parity and future
    // hardening (e.g. wait-timeout crate).
    let mut cmd = Command::new("iw");
    cmd.arg("dev").arg(iface).arg("scan");
    if dump {
        cmd.arg("dump");
    }
    cmd.stdin(std::process::Stdio::null());
    cmd.output().map_err(ScanError::from)
}

fn freq_to_channel(freq: u32) -> u32 {
    if (2412..=2484).contains(&freq) {
        if freq == 2484 {
            return 14;
        }
        return (freq - 2407) / 5;
    }
    if (5180..=5825).contains(&freq) {
        return (freq - 5000) / 5;
    }
    if (5955..=7115).contains(&freq) {
        return (freq - 5950) / 5;
    }
    0
}

fn parse_iw_scan(text: &str) -> Vec<WifiDevice> {
    let mut devices = Vec::new();
    let mut current_mac: Option<String> = None;
    let mut freq = 0u32;
    let mut signal = -90.0f64;
    let mut ssid = String::new();
    let mut channel = 0u32;

    let flush = |devices: &mut Vec<WifiDevice>,
                 current_mac: &mut Option<String>,
                 freq: &mut u32,
                 signal: &mut f64,
                 ssid: &mut String,
                 channel: &mut u32| {
        let Some(mac) = current_mac.take() else {
            return;
        };
        if *channel == 0 && *freq > 0 {
            *channel = freq_to_channel(*freq);
        }
        let mut dev = WifiDevice::hotspot(normalize_mac(&mac), *signal);
        dev.kind = DeviceKind::Hotspot;
        dev.ssid = ssid.clone();
        dev.channel = *channel;
        dev.freq_mhz = *freq;
        devices.push(dev);
        *freq = 0;
        *signal = -90.0;
        ssid.clear();
        *channel = 0;
    };

    for line in text.lines() {
        if let Some(mac) = parse_bss_line(line) {
            flush(
                &mut devices,
                &mut current_mac,
                &mut freq,
                &mut signal,
                &mut ssid,
                &mut channel,
            );
            current_mac = Some(mac);
            continue;
        }
        if current_mac.is_none() {
            continue;
        }
        if let Some(rest) = line.trim_start().strip_prefix("freq:") {
            if let Ok(v) = rest.trim().parse::<u32>() {
                freq = v;
            }
            continue;
        }
        if let Some(rest) = line.trim_start().strip_prefix("signal:") {
            let num = rest.trim().trim_end_matches("dBm").trim();
            if let Ok(v) = num.parse::<f64>() {
                signal = v;
            }
            continue;
        }
        if let Some(rest) = line.trim_start().strip_prefix("SSID:") {
            ssid = rest.trim().to_string();
            continue;
        }
        if let Some(rest) = line
            .trim_start()
            .strip_prefix("DS Parameter set: channel ")
        {
            if let Ok(v) = rest.trim().parse::<u32>() {
                channel = v;
            }
            continue;
        }
        if channel == 0 {
            if let Some(rest) = line.trim_start().strip_prefix("* primary channel:") {
                if let Ok(v) = rest.trim().parse::<u32>() {
                    channel = v;
                }
            }
        }
    }
    flush(
        &mut devices,
        &mut current_mac,
        &mut freq,
        &mut signal,
        &mut ssid,
        &mut channel,
    );
    devices
}

fn parse_bss_line(line: &str) -> Option<String> {
    // BSS aa:bb:cc:dd:ee:ff(on wlan0)
    let rest = line.strip_prefix("BSS ")?;
    let mac = rest.split('(').next()?.trim();
    if mac.len() == 17 {
        Some(mac.to_ascii_lowercase())
    } else {
        None
    }
}

/// Placeholder for the native nl80211 backend (tracked target).
pub struct Nl80211Backend {
    _iface: String,
}

impl Nl80211Backend {
    /// Create a stub nl80211 backend for `iface`.
    pub fn new(iface: impl Into<String>) -> Self {
        Self {
            _iface: iface.into(),
        }
    }
}

impl ScanBackend for Nl80211Backend {
    fn name(&self) -> &'static str {
        "nl80211"
    }

    fn scan(&mut self) -> Result<Vec<WifiDevice>, ScanError> {
        Err(ScanError::Message(
            "nl80211 backend not implemented yet; use IwCliBackend for now".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
BSS aa:bb:cc:dd:ee:01(on wlan0)
	freq: 2437
	signal: -45.00 dBm
	SSID: TestNet
	DS Parameter set: channel 6
BSS 11:22:33:44:55:66(on wlan0)
	freq: 5180
	signal: -60.00 dBm
	SSID: FiveG
	* primary channel: 36
";

    #[test]
    fn parses_two_bss() {
        let devices = parse_iw_scan(SAMPLE);
        assert_eq!(devices.len(), 2);
        assert_eq!(devices[0].mac, "aa:bb:cc:dd:ee:01");
        assert_eq!(devices[0].ssid, "TestNet");
        assert_eq!(devices[0].channel, 6);
        assert_eq!(devices[0].freq_mhz, 2437);
        assert!((devices[0].rssi_dbm - (-45.0)).abs() < 0.01);
        assert_eq!(devices[1].mac, "11:22:33:44:55:66");
        assert_eq!(devices[1].channel, 36);
        assert_eq!(devices[1].freq_mhz, 5180);
    }

    #[test]
    fn nl80211_stub_reports_name() {
        let mut b = Nl80211Backend::new("wlan0");
        assert_eq!(b.name(), "nl80211");
        assert!(b.scan().is_err());
    }
}
