//! Persistent per-device calibration (distance reference, manual bearing).
//!
//! File format matches Python `~/.cache/wifi_radar/calibration.json`.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::models::{WifiDevice, PATH_LOSS_EXPONENT_INDOOR};
use crate::util::normalize_mac;

/// Scene correction mode (Python `CalibrationMode` parity).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum CalibrationMode {
    /// Only apply explicit per-MAC anchors; no scene propagation.
    #[default]
    Honest,
    /// Heuristic distance/bearing propagation via scene correction.
    Anchor,
}

/// On-disk JSON payload (Python-compatible keys).
#[derive(Debug, Default, Serialize, Deserialize)]
struct CalibrationFile {
    #[serde(default)]
    rssi_at_1m: HashMap<String, f64>,
    #[serde(default)]
    manual_bearing: HashMap<String, f64>,
    #[serde(default)]
    distance_anchor_m: HashMap<String, f64>,
    #[serde(default)]
    mode: CalibrationMode,
}

/// Load/save per-MAC RSSI-at-1m overrides and manual bearings.
#[derive(Debug, Clone)]
pub struct CalibrationStore {
    path: PathBuf,
    rssi_at_1m: HashMap<String, f64>,
    manual_bearing: HashMap<String, f64>,
    distance_anchor_m: HashMap<String, f64>,
    mode: CalibrationMode,
}

impl CalibrationStore {
    /// Default cache path: `$HOME/.cache/wifi_radar/calibration.json`.
    pub fn default_path() -> PathBuf {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        home.join(".cache").join("wifi_radar").join("calibration.json")
    }

    /// Create a store for `path` and load if the file exists.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        let mut store = Self {
            path: path.into(),
            rssi_at_1m: HashMap::new(),
            manual_bearing: HashMap::new(),
            distance_anchor_m: HashMap::new(),
            mode: CalibrationMode::Honest,
        };
        let _ = store.load();
        store
    }

    /// In-memory store with no persistence (tests / ephemeral sessions).
    pub fn memory() -> Self {
        Self {
            path: PathBuf::new(),
            rssi_at_1m: HashMap::new(),
            manual_bearing: HashMap::new(),
            distance_anchor_m: HashMap::new(),
            mode: CalibrationMode::Honest,
        }
    }

    /// Path used for load/save (empty when [`Self::memory`]).
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Load from disk; ignores missing/corrupt files.
    pub fn load(&mut self) -> bool {
        if self.path.as_os_str().is_empty() || !self.path.exists() {
            return false;
        }
        let Ok(text) = fs::read_to_string(&self.path) else {
            return false;
        };
        let Ok(data) = serde_json::from_str::<CalibrationFile>(&text) else {
            return false;
        };
        self.rssi_at_1m = data
            .rssi_at_1m
            .into_iter()
            .map(|(k, v)| (normalize_mac(&k), v))
            .collect();
        self.manual_bearing = data
            .manual_bearing
            .into_iter()
            .map(|(k, v)| (normalize_mac(&k), v.rem_euclid(360.0)))
            .collect();
        self.distance_anchor_m = data
            .distance_anchor_m
            .into_iter()
            .map(|(k, v)| (normalize_mac(&k), v))
            .collect();
        self.mode = data.mode;
        true
    }

    /// Persist to disk (no-op for memory stores).
    pub fn save(&self) -> Result<(), String> {
        if self.path.as_os_str().is_empty() {
            return Ok(());
        }
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let payload = CalibrationFile {
            rssi_at_1m: self.rssi_at_1m.clone(),
            manual_bearing: self.manual_bearing.clone(),
            distance_anchor_m: self.distance_anchor_m.clone(),
            mode: self.mode,
        };
        let text = serde_json::to_string_pretty(&payload).map_err(|e| e.to_string())?;
        fs::write(&self.path, text + "\n").map_err(|e| e.to_string())
    }

    /// RSSI-at-1m override for `mac`, if any.
    pub fn rssi_at_1m(&self, mac: &str) -> Option<f64> {
        self.rssi_at_1m.get(&normalize_mac(mac)).copied()
    }

    /// Calibrate so current RSSI maps to `distance_m`. Returns new 1 m reference.
    pub fn set_distance_reference(
        &mut self,
        mac: &str,
        rssi_dbm: f64,
        distance_m: f64,
    ) -> Result<f64, String> {
        let distance_m = distance_m.max(0.1);
        let ref_dbm = rssi_dbm + 10.0 * PATH_LOSS_EXPONENT_INDOOR * distance_m.log10();
        let key = normalize_mac(mac);
        self.rssi_at_1m.insert(key.clone(), ref_dbm);
        self.distance_anchor_m.insert(key, distance_m);
        self.save()?;
        Ok(ref_dbm)
    }

    /// Pin a world bearing (degrees) for `mac`.
    pub fn set_manual_bearing(&mut self, mac: &str, bearing_deg: f64) -> Result<(), String> {
        self.manual_bearing
            .insert(normalize_mac(mac), bearing_deg.rem_euclid(360.0));
        self.save()
    }

    /// Manual bearing for `mac`, if any.
    pub fn manual_bearing(&self, mac: &str) -> Option<f64> {
        self.manual_bearing
            .get(&normalize_mac(mac))
            .map(|b| b.rem_euclid(360.0))
    }

    /// Recorded distance anchor metres for `mac`, if any.
    pub fn distance_anchor_m(&self, mac: &str) -> Option<f64> {
        self.distance_anchor_m.get(&normalize_mac(mac)).copied()
    }

    /// Active scene mode.
    pub fn mode(&self) -> CalibrationMode {
        self.mode
    }

    /// Set scene mode and persist.
    pub fn set_mode(&mut self, mode: CalibrationMode) -> Result<(), String> {
        self.mode = mode;
        self.save()
    }

    /// Clear pin + distance calibration for one MAC.
    pub fn clear(&mut self, mac: &str) -> Result<(), String> {
        let key = normalize_mac(mac);
        self.rssi_at_1m.remove(&key);
        self.manual_bearing.remove(&key);
        self.distance_anchor_m.remove(&key);
        self.save()
    }

    /// True if `mac` has a stored RSSI-at-1m / distance anchor.
    pub fn has_distance_calibration(&self, mac: &str) -> bool {
        self.rssi_at_1m.contains_key(&normalize_mac(mac))
    }

    /// True if `mac` has a stored manual bearing pin.
    pub fn has_manual_bearing(&self, mac: &str) -> bool {
        self.manual_bearing.contains_key(&normalize_mac(mac))
    }

    /// Apply stored overrides onto a device snapshot (mutates in place).
    pub fn apply_to_devices(&self, devices: &mut [WifiDevice]) {
        for d in devices {
            if let Some(b) = self.manual_bearing(&d.mac) {
                d.bearing_deg = Some(b);
                d.bearing_manual = true;
                d.bearing_confidence = 1.0;
            }
            if let Some(r) = self.rssi_at_1m(&d.mac) {
                d.rssi_at_1m_override = Some(r);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn tmp_path(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("wifi-radar-calib-{name}-{nanos}.json"))
    }

    #[test]
    fn distance_reference_roundtrip() {
        let path = tmp_path("dist");
        let mut store = CalibrationStore::new(&path);
        let ref_dbm = store
            .set_distance_reference("AA:BB:CC:DD:EE:01", -40.0, 1.0)
            .unwrap();
        assert!((ref_dbm - (-40.0)).abs() < 1e-9);
        let mut store2 = CalibrationStore::new(&path);
        assert!((store2.rssi_at_1m("aa:bb:cc:dd:ee:01").unwrap() - (-40.0)).abs() < 1e-9);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn manual_bearing_and_clear() {
        let mut store = CalibrationStore::memory();
        store.set_manual_bearing("aa:bb:cc:dd:ee:02", 450.0).unwrap();
        assert!((store.manual_bearing("AA:BB:CC:DD:EE:02").unwrap() - 90.0).abs() < 1e-9);
        store.clear("aa:bb:cc:dd:ee:02").unwrap();
        assert!(store.manual_bearing("aa:bb:cc:dd:ee:02").is_none());
    }

    #[test]
    fn apply_to_devices_sets_fields() {
        let mut store = CalibrationStore::memory();
        store.set_manual_bearing("aa:bb:cc:dd:ee:03", 45.0).unwrap();
        store
            .set_distance_reference("aa:bb:cc:dd:ee:03", -50.0, 0.25)
            .unwrap();
        let mut dev = WifiDevice::hotspot("aa:bb:cc:dd:ee:03", -50.0);
        store.apply_to_devices(std::slice::from_mut(&mut dev));
        assert!(dev.bearing_manual);
        assert!((dev.bearing_deg.unwrap() - 45.0).abs() < 1e-9);
        assert!(dev.rssi_at_1m_override.is_some());
    }
}
