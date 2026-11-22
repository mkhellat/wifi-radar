//! Parse `airodump-ng --output-format csv` files (parity with Python
//! `scan/airodump.py`).
//!
//! The file holds two comma-separated tables separated by a blank line:
//! access points (header starts `BSSID`) and stations (header starts
//! `Station MAC`). airodump does not quote fields, so an ESSID that itself
//! contains commas is split across several columns; the AP parser rejoins
//! them. Parsing itself never touches the radio.
//! [`sample_airodump`] is the live passive listen (root, `airodump-ng`, and a
//! monitor VIF). It writes a CSV and does not inject, deauth, or associate.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use wifi_radar_core::{normalize_mac, DeviceKind, ScanError, WifiDevice};

/// How long the Python worker lets airodump-ng listen (`MONITOR_SAMPLE_SEC`).
pub const AIRODUMP_SAMPLE: Duration = Duration::from_secs(4);

/// RSSI used when airodump reports no signal (`-1`) or a blank power field.
const NO_SIGNAL_DBM: f64 = -90.0;

/// Parse an airodump-ng CSV file from disk.
///
/// Returns empty vectors when the file is missing or unreadable (airodump
/// writes the file lazily, so "not there yet" is a normal outcome).
pub fn parse_airodump_file(path: &Path) -> (Vec<WifiDevice>, Vec<WifiDevice>) {
    match std::fs::read(path) {
        Ok(bytes) => parse_airodump_csv(&String::from_utf8_lossy(&bytes)),
        Err(_) => (Vec::new(), Vec::new()),
    }
}

/// Parse airodump-ng CSV text into `(access_points, stations)`.
///
/// Stations with an associated BSSID become [`DeviceKind::Client`]; those
/// shown as `(not associated)` become probe-only [`DeviceKind::Adapter`]s.
pub fn parse_airodump_csv(text: &str) -> (Vec<WifiDevice>, Vec<WifiDevice>) {
    let mut aps = Vec::new();
    let mut clients = Vec::new();

    #[derive(Clone, Copy, PartialEq)]
    enum Section {
        None,
        Aps,
        Stations,
    }
    let mut section = Section::None;

    for raw in text.lines() {
        // `lines()` already strips the `\r` of airodump's CRLF endings.
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        let raw: Vec<&str> = line.split(',').collect();
        let fields: Vec<&str> = raw.iter().map(|f| f.trim()).collect();
        match fields[0] {
            "BSSID" => {
                section = Section::Aps;
                continue;
            }
            "Station MAC" => {
                section = Section::Stations;
                continue;
            }
            _ => {}
        }
        match section {
            Section::Aps => {
                if let Some(ap) = parse_ap_row(&raw, &fields) {
                    aps.push(ap);
                }
            }
            Section::Stations => {
                if let Some(st) = parse_station_row(&fields) {
                    clients.push(st);
                }
            }
            Section::None => {}
        }
    }
    (aps, clients)
}

/// Center frequency (MHz) for a 2.4 / 5 GHz channel as reported by airodump
/// (`--band abg`); 0 when the channel is unknown.
fn channel_to_freq(channel: u32) -> u32 {
    match channel {
        1..=13 => 2407 + 5 * channel,
        14 => 2484,
        32..=177 => 5000 + 5 * channel,
        _ => 0,
    }
}

fn parse_power(field: &str) -> f64 {
    match field.trim().parse::<i32>() {
        Ok(-1) | Err(_) => NO_SIGNAL_DBM,
        Ok(p) => f64::from(p),
    }
}

/// Argv for a passive `airodump-ng` CSV sample (no shell, no injection flags).
pub fn sample_args(mon_iface: &str, write_prefix: &Path) -> Vec<String> {
    vec![
        mon_iface.to_string(),
        "--band".into(),
        "abg".into(),
        "--write".into(),
        write_prefix.display().to_string(),
        "--output-format".into(),
        "csv".into(),
        "--write-interval".into(),
        "1".into(),
    ]
}

/// Listen on `mon_iface` for `sample_for`, then parse the CSV airodump wrote.
///
/// The child is stopped with SIGINT so it flushes `prefix-01.csv` (Python's
/// worker does the same). A missing binary is an error. A short or empty
/// capture is an empty device list, not an error: airodump exits non-zero
/// on SIGINT even when the CSV is usable. Requires root and a monitor VIF
/// at runtime; unit tests do not open a radio.
pub fn sample_airodump(
    mon_iface: &str,
    sample_for: Duration,
) -> Result<(Vec<WifiDevice>, Vec<WifiDevice>), ScanError> {
    sample_with("airodump-ng", mon_iface, sample_for)
}

pub(crate) fn sample_with(
    program: &str,
    mon_iface: &str,
    sample_for: Duration,
) -> Result<(Vec<WifiDevice>, Vec<WifiDevice>), ScanError> {
    if !crate::monitor::valid_iface_name(mon_iface) {
        return Err(ScanError::Message(format!(
            "invalid interface name: {mon_iface:?}"
        )));
    }
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("wifi_radar_{}_{stamp}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    let _cleanup = DirGuard(dir.clone());
    let prefix = dir.join("dump");
    let mut cmd = Command::new(program);
    cmd.args(sample_args(mon_iface, &prefix));
    crate::proc::spawn_then_interrupt(&mut cmd, sample_for, Duration::from_secs(5))
        .map_err(|e| ScanError::Message(format!("{program}: {e}")))?;
    let csv = PathBuf::from(format!("{}-01.csv", prefix.display()));
    Ok(parse_airodump_file(&csv))
}

struct DirGuard(PathBuf);

impl Drop for DirGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn looks_like_mac(s: &str) -> bool {
    s.len() == 17 && s.split(':').count() == 6
}

fn parse_ap_row(raw: &[&str], fields: &[&str]) -> Option<WifiDevice> {
    // BSSID, first, last, channel, speed, privacy, cipher, auth, power,
    // beacons, IV, LAN IP, ID-length, ESSID, Key
    if fields.len() < 14 || !looks_like_mac(fields[0]) {
        return None;
    }
    // airodump prints `-1` for an unknown channel; that parses as 0 here.
    let channel = fields[3].parse::<u32>().unwrap_or(0);
    // The trailing Key column is always empty in practice; everything between
    // the ESSID column and it belongs to an ESSID containing commas.
    let essid_end = if fields.len() >= 15 {
        fields.len() - 1
    } else {
        fields.len()
    };
    // Join the untrimmed columns so "Cafe, Bar" keeps its inner space.
    let ssid = raw[13..essid_end].join(",").trim().to_string();

    let mut dev = WifiDevice::hotspot(normalize_mac(fields[0]), parse_power(fields[8]));
    dev.ssid = ssid;
    dev.channel = channel;
    dev.freq_mhz = channel_to_freq(channel);
    Some(dev)
}

fn parse_station_row(fields: &[&str]) -> Option<WifiDevice> {
    // Station MAC, first, last, power, packets, BSSID, probed ESSIDs...
    if fields.len() < 6 || !looks_like_mac(fields[0]) {
        return None;
    }
    let bssid_raw = fields[5];
    let bssid = if bssid_raw.is_empty() || bssid_raw.contains("(not associated)") {
        String::new()
    } else {
        normalize_mac(bssid_raw)
    };
    let kind = if bssid.is_empty() {
        DeviceKind::Adapter
    } else {
        DeviceKind::Client
    };
    let mut dev = WifiDevice::hotspot(normalize_mac(fields[0]), parse_power(fields[3]));
    dev.kind = kind;
    dev.associated_bssid = bssid;
    dev.probe_ssids = fields[6..]
        .iter()
        .filter(|s| !s.is_empty())
        .map(|s| (*s).to_string())
        .collect();
    Some(dev)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../tests/fixtures/airodump.csv");

    #[test]
    fn parses_access_points() {
        let (aps, _) = parse_airodump_csv(FIXTURE);
        assert_eq!(aps.len(), 4);
        assert_eq!(aps[0].mac, "aa:bb:cc:dd:ee:01");
        assert_eq!(aps[0].kind, DeviceKind::Hotspot);
        assert_eq!(aps[0].ssid, "HomeNetwork");
        assert_eq!(aps[0].channel, 6);
        assert_eq!(aps[0].freq_mhz, 2437);
        assert!((aps[0].rssi_dbm - (-45.0)).abs() < 1e-9);
        assert_eq!(aps[1].mac, "11:22:33:44:55:66");
        assert_eq!(aps[1].channel, 36);
        assert_eq!(aps[1].freq_mhz, 5180);
    }

    #[test]
    fn ap_power_minus_one_maps_to_floor() {
        let (aps, _) = parse_airodump_csv(FIXTURE);
        assert!((aps[2].rssi_dbm - NO_SIGNAL_DBM).abs() < 1e-9);
    }

    #[test]
    fn essid_with_comma_is_rejoined() {
        let (aps, _) = parse_airodump_csv(FIXTURE);
        assert_eq!(aps[2].ssid, "Cafe, Bar");
    }

    #[test]
    fn hidden_ap_has_empty_ssid() {
        let (aps, _) = parse_airodump_csv(FIXTURE);
        assert_eq!(aps[3].ssid, "");
        assert_eq!(aps[3].channel, 1);
    }

    #[test]
    fn associated_station_is_client() {
        let (_, st) = parse_airodump_csv(FIXTURE);
        assert_eq!(st[0].mac, "aa:bb:cc:11:22:33");
        assert_eq!(st[0].kind, DeviceKind::Client);
        assert_eq!(st[0].associated_bssid, "aa:bb:cc:dd:ee:01");
        assert!((st[0].rssi_dbm - (-55.0)).abs() < 1e-9);
        assert!(st[0].probe_ssids.is_empty());
    }

    #[test]
    fn unassociated_station_is_adapter_with_probes() {
        let (_, st) = parse_airodump_csv(FIXTURE);
        assert_eq!(st.len(), 3);
        assert_eq!(st[1].mac, "de:ad:00:00:00:01");
        assert_eq!(st[1].kind, DeviceKind::Adapter);
        assert!(st[1].associated_bssid.is_empty());
        assert_eq!(st[1].probe_ssids, vec!["FreeWifi", "CoffeeShop"]);
        assert!((st[2].rssi_dbm - NO_SIGNAL_DBM).abs() < 1e-9);
    }

    #[test]
    fn crlf_line_endings_parse_identically() {
        let crlf = FIXTURE.replace('\n', "\r\n");
        let (aps, st) = parse_airodump_csv(&crlf);
        let (aps_lf, st_lf) = parse_airodump_csv(FIXTURE);
        assert_eq!(aps.len(), aps_lf.len());
        assert_eq!(st.len(), st_lf.len());
        assert_eq!(aps[0].ssid, "HomeNetwork");
        assert_eq!(st[1].probe_ssids, vec!["FreeWifi", "CoffeeShop"]);
    }

    #[test]
    fn garbage_and_empty_input_yield_nothing() {
        assert_eq!(parse_airodump_csv("").0.len(), 0);
        let (aps, st) = parse_airodump_csv("not a csv\nat all\n");
        assert!(aps.is_empty() && st.is_empty());
    }

    #[test]
    fn ap_only_file_has_no_stations() {
        let ap_only: String = FIXTURE
            .lines()
            .take_while(|l| !l.trim().is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        let (aps, st) = parse_airodump_csv(&ap_only);
        assert_eq!(aps.len(), 4);
        assert!(st.is_empty());
    }

    #[test]
    fn missing_file_is_empty_not_error() {
        let (aps, st) = parse_airodump_file(Path::new("/nonexistent/wifi-radar/dump-01.csv"));
        assert!(aps.is_empty() && st.is_empty());
    }

    #[test]
    fn channel_freq_map() {
        assert_eq!(channel_to_freq(1), 2412);
        assert_eq!(channel_to_freq(14), 2484);
        assert_eq!(channel_to_freq(149), 5745);
        assert_eq!(channel_to_freq(0), 0);
    }

    #[test]
    fn sample_argv_is_passive_csv_write() {
        let args = sample_args("wlan0mon", Path::new("/tmp/wifi_radar_dump"));
        assert_eq!(
            args,
            [
                "wlan0mon",
                "--band",
                "abg",
                "--write",
                "/tmp/wifi_radar_dump",
                "--output-format",
                "csv",
                "--write-interval",
                "1",
            ]
        );
    }

    #[test]
    fn sample_rejects_unsafe_iface_without_spawning() {
        let err = sample_airodump("-h", Duration::from_secs(1)).expect_err("reject");
        assert!(err.to_string().contains("invalid interface"));
    }

    #[test]
    fn sample_missing_program_does_not_need_radio() {
        let err = sample_with(
            "wifi-radar-no-such-program",
            "wlan0mon",
            Duration::from_millis(50),
        )
        .expect_err("missing binary");
        let msg = err.to_string();
        assert!(
            msg.contains("airodump") || msg.contains("no-such-program") || msg.contains("os error"),
            "{msg}"
        );
    }
}
