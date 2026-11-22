//! Parse tshark probe-request field output (parity with Python
//! `scan/tshark.py`).
//!
//! The capture command prints one tab-separated row per probe request:
//! `wlan.sa`, `wlan.ssid`, `radiotap.dbm_antsignal`. Probe requests come from
//! stations that are *not* associated to an AP, so every MAC becomes an
//! [`DeviceKind::Adapter`] carrying the SSIDs it asked for. Parsing is pure;
//! the live capture is a separate, later step (it needs root and `tshark`).

use wifi_radar_core::{normalize_mac, DeviceKind, WifiDevice};

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
}
