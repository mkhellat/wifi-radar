//! Session rotation calibration (peak-RSSI → bearing while user turns).
//!
//! Parity with Python `BearingCalibrator`. Session-only — does not write
//! [`CalibrationStore`](crate::calibration::CalibrationStore).

use std::collections::HashMap;

use crate::calibration::CalibrationStore;
use crate::models::WifiDevice;
use crate::store::DeviceStore;
use crate::util::normalize_mac;

/// Minimum RSSI spread (dB) required before a bearing is trusted.
pub const MIN_RSSI_SPREAD_DB: f64 = 5.0;
/// Minimum samples per MAC before a bearing is considered.
pub const MIN_SAMPLES: usize = 4;

/// Track RSSI vs heading while the user rotates.
#[derive(Debug, Default)]
pub struct BearingCalibrator {
    active: bool,
    samples: HashMap<String, Vec<(f64, f64)>>,
    last_scan_generation: i64,
}

impl BearingCalibrator {
    /// Create an inactive calibrator.
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether calibration is currently collecting samples.
    pub fn active(&self) -> bool {
        self.active
    }

    /// Start a new calibration session (clears prior samples).
    pub fn start(&mut self) {
        self.active = true;
        self.samples.clear();
        self.last_scan_generation = -1;
    }

    /// Stop collecting without applying.
    pub fn stop(&mut self) {
        self.active = false;
    }

    /// Record one sample per fresh scan generation (not every UI frame).
    pub fn feed(&mut self, heading_deg: f64, devices: &[WifiDevice], scan_generation: u64) {
        if !self.active {
            return;
        }
        let gen = scan_generation as i64;
        if gen == self.last_scan_generation {
            return;
        }
        self.last_scan_generation = gen;
        let heading = heading_deg.rem_euclid(360.0);
        for dev in devices {
            self.samples
                .entry(normalize_mac(&dev.mac))
                .or_default()
                .push((heading, dev.rssi_dbm));
        }
    }

    /// Apply peak-RSSI bearings onto devices that lack a manual pin.
    ///
    /// Skips MACs with a stored manual bearing, and MACs with too few samples
    /// or RSSI spread below [`MIN_RSSI_SPREAD_DB`].
    pub fn apply(&self, store: &mut DeviceStore, calibration: &CalibrationStore) -> usize {
        let mut applied = 0;
        for (mac, points) in &self.samples {
            if calibration.has_manual_bearing(mac) {
                continue;
            }
            if points.len() < MIN_SAMPLES {
                continue;
            }
            let max_rssi = points
                .iter()
                .map(|p| p.1)
                .fold(f64::NEG_INFINITY, f64::max);
            let min_rssi = points
                .iter()
                .map(|p| p.1)
                .fold(f64::INFINITY, f64::min);
            let spread = max_rssi - min_rssi;
            if spread < MIN_RSSI_SPREAD_DB {
                continue;
            }
            let (best_bearing, _) = points
                .iter()
                .copied()
                .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
                .unwrap();
            if let Some(dev) = store.device_mut(mac) {
                dev.bearing_deg = Some(best_bearing);
                dev.bearing_confidence = (spread / 20.0).min(1.0);
                applied += 1;
            }
        }
        applied
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::WifiDevice;

    #[test]
    fn apply_sets_peak_rssi_bearing() {
        let mut cal = BearingCalibrator::new();
        cal.start();
        let d = WifiDevice::hotspot("aa:bb:cc:00:00:01", -60.0);
        cal.feed(10.0, &[d.clone()], 1);
        let mut d2 = d.clone();
        d2.rssi_dbm = -50.0;
        cal.feed(90.0, &[d2.clone()], 2);
        let mut d3 = d.clone();
        d3.rssi_dbm = -55.0;
        cal.feed(180.0, &[d3.clone()], 3);
        let mut d4 = d.clone();
        d4.rssi_dbm = -58.0;
        cal.feed(270.0, &[d4], 4);
        cal.stop();

        let mut store = DeviceStore::new();
        store.merge([WifiDevice::hotspot("aa:bb:cc:00:00:01", -55.0)]);
        let calib = CalibrationStore::memory();
        let n = cal.apply(&mut store, &calib);
        assert_eq!(n, 1);
        let brg = store.device_mut("aa:bb:cc:00:00:01").unwrap().bearing_deg;
        assert!((brg.unwrap() - 90.0).abs() < 1e-9);
    }

    #[test]
    fn skips_insufficient_spread() {
        let mut cal = BearingCalibrator::new();
        cal.start();
        let d = WifiDevice::hotspot("aa:bb:cc:00:00:02", -50.0);
        for gen in 1..=4 {
            cal.feed(gen as f64 * 40.0, &[d.clone()], gen);
        }
        let mut store = DeviceStore::new();
        store.merge([d]);
        let calib = CalibrationStore::memory();
        assert_eq!(cal.apply(&mut store, &calib), 0);
    }

    #[test]
    fn feed_ignores_same_generation() {
        let mut cal = BearingCalibrator::new();
        cal.start();
        let d = WifiDevice::hotspot("aa:bb:cc:00:00:03", -40.0);
        cal.feed(0.0, &[d.clone()], 1);
        cal.feed(90.0, &[d], 1);
        assert_eq!(cal.samples.values().next().unwrap().len(), 1);
    }
}
