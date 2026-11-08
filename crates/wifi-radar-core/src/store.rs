//! Device store with EMA RSSI, TTL expiry, and merge (Python DeviceStore parity).

use std::collections::HashMap;

use crate::models::{now_unix, DeviceKind, WifiDevice};
use crate::util::normalize_mac;

/// Default device TTL in seconds.
pub const DEFAULT_TTL_SEC: f64 = 30.0;
/// EMA coefficient for RSSI updates (matches Python `RSSI_EMA_ALPHA`).
pub const RSSI_EMA_ALPHA: f64 = 0.25;

/// In-memory registry of observed devices.
#[derive(Debug, Default)]
pub struct DeviceStore {
    devices: HashMap<String, WifiDevice>,
    ttl: f64,
}

impl DeviceStore {
    /// Create an empty store with the default TTL.
    pub fn new() -> Self {
        Self {
            devices: HashMap::new(),
            ttl: DEFAULT_TTL_SEC,
        }
    }

    /// Create a store with a custom TTL (seconds).
    pub fn with_ttl(ttl: f64) -> Self {
        Self {
            devices: HashMap::new(),
            ttl,
        }
    }

    /// Current device map (keyed by normalized MAC).
    pub fn devices(&self) -> &HashMap<String, WifiDevice> {
        &self.devices
    }

    /// Merge a scan batch into the store.
    pub fn merge(&mut self, incoming: impl IntoIterator<Item = WifiDevice>) {
        let now = now_unix();
        for mut d in incoming {
            d.mac = normalize_mac(&d.mac);
            let mac = d.mac.clone();
            if let Some(existing) = self.devices.get_mut(&mac) {
                existing.last_seen = now;
                existing.rssi_dbm =
                    RSSI_EMA_ALPHA * d.rssi_dbm + (1.0 - RSSI_EMA_ALPHA) * existing.rssi_dbm;
                if !d.ssid.is_empty() {
                    existing.ssid = d.ssid;
                }
                if d.channel != 0 {
                    existing.channel = d.channel;
                }
                if d.freq_mhz != 0 {
                    existing.freq_mhz = d.freq_mhz;
                }
                if !d.security.is_empty() {
                    existing.security = d.security;
                }
                if d.in_use {
                    existing.in_use = true;
                }
                existing.kind = kind_priority(existing.kind, d.kind);
                for s in d.probe_ssids {
                    if !existing.probe_ssids.contains(&s) {
                        existing.probe_ssids.push(s);
                    }
                }
                if !d.associated_bssid.is_empty() {
                    existing.associated_bssid = d.associated_bssid;
                }
                if let Some(b) = d.bearing_deg {
                    existing.bearing_deg = Some(b);
                    existing.bearing_confidence =
                        existing.bearing_confidence.max(d.bearing_confidence);
                }
            } else {
                d.last_seen = now;
                self.devices.insert(mac, d);
            }
        }
    }

    /// Remove devices not seen within TTL. Returns removed MACs.
    pub fn expire(&mut self) -> Vec<String> {
        let now = now_unix();
        let expired: Vec<String> = self
            .devices
            .iter()
            .filter(|(_, d)| now - d.last_seen > self.ttl)
            .map(|(mac, _)| mac.clone())
            .collect();
        for mac in &expired {
            self.devices.remove(mac);
        }
        expired
    }

    /// Expire stale devices and return a snapshot sorted strongest-first.
    pub fn snapshot(&mut self) -> Vec<WifiDevice> {
        self.expire();
        let mut out: Vec<WifiDevice> = self.devices.values().cloned().collect();
        out.sort_by(|a, b| {
            b.rssi_dbm
                .partial_cmp(&a.rssi_dbm)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        out
    }
}

fn kind_priority(existing: DeviceKind, incoming: DeviceKind) -> DeviceKind {
    match (existing, incoming) {
        (_, DeviceKind::Hotspot) => DeviceKind::Hotspot,
        (DeviceKind::Hotspot, _) => DeviceKind::Hotspot,
        (_, DeviceKind::Client) => DeviceKind::Client,
        (DeviceKind::Client, DeviceKind::Adapter) => DeviceKind::Client,
        (a, _) => a,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_new_device() {
        let mut store = DeviceStore::new();
        store.merge([WifiDevice::hotspot("AA:BB:CC:DD:EE:FF", -50.0)]);
        assert_eq!(store.devices().len(), 1);
        assert!(store.devices().contains_key("aa:bb:cc:dd:ee:ff"));
    }

    #[test]
    fn merge_ema_rssi() {
        let mut store = DeviceStore::new();
        store.merge([WifiDevice::hotspot("aa:bb:cc:dd:ee:ff", -40.0)]);
        store.merge([WifiDevice::hotspot("aa:bb:cc:dd:ee:ff", -80.0)]);
        let rssi = store.devices()["aa:bb:cc:dd:ee:ff"].rssi_dbm;
        let expected = RSSI_EMA_ALPHA * -80.0 + (1.0 - RSSI_EMA_ALPHA) * -40.0;
        assert!((rssi - expected).abs() < 1e-9);
    }

    #[test]
    fn expire_removes_stale() {
        let mut store = DeviceStore::with_ttl(0.05);
        store.merge([WifiDevice::hotspot("aa:bb:cc:dd:ee:ff", -50.0)]);
        std::thread::sleep(std::time::Duration::from_millis(80));
        let removed = store.expire();
        assert_eq!(removed, vec!["aa:bb:cc:dd:ee:ff".to_string()]);
        assert!(store.devices().is_empty());
    }

    #[test]
    fn kind_priority_hotspot_wins() {
        let mut store = DeviceStore::new();
        let mut adapter = WifiDevice::hotspot("aa:bb:cc:dd:ee:ff", -50.0);
        adapter.kind = DeviceKind::Adapter;
        store.merge([adapter]);
        let mut hotspot = WifiDevice::hotspot("aa:bb:cc:dd:ee:ff", -55.0);
        hotspot.kind = DeviceKind::Hotspot;
        store.merge([hotspot]);
        assert_eq!(
            store.devices()["aa:bb:cc:dd:ee:ff"].kind,
            DeviceKind::Hotspot
        );
    }

    #[test]
    fn snapshot_sorted_by_rssi() {
        let mut store = DeviceStore::new();
        store.merge([
            WifiDevice::hotspot("aa:bb:cc:dd:ee:01", -70.0),
            WifiDevice::hotspot("aa:bb:cc:dd:ee:02", -40.0),
        ]);
        let snap = store.snapshot();
        assert_eq!(snap[0].mac, "aa:bb:cc:dd:ee:02");
        assert_eq!(snap[1].mac, "aa:bb:cc:dd:ee:01");
    }
}
