//! Scene-wide correction heuristics on top of per-MAC calibration.
//!
//! Parity with Python `wifi_radar.correction`.

use std::collections::HashMap;

use crate::calibration::{CalibrationMode, CalibrationStore};
use crate::models::{
    DeviceKind, WifiDevice, PATH_LOSS_EXPONENT_INDOOR, RSSI_AT_1M_AP, RSSI_AT_1M_CLIENT,
};

/// Residuals for one visible calibrated device.
#[derive(Debug, Clone)]
pub struct AnchorDiagnostic {
    /// Device MAC.
    pub mac: String,
    /// Calibrated distance target, if any.
    pub distance_target_m: Option<f64>,
    /// Current path-loss distance, if computed.
    pub current_distance_m: Option<f64>,
    /// Target / default-distance scale factor.
    pub distance_scale: Option<f64>,
    /// Manual bearing target, if any.
    pub bearing_target_deg: Option<f64>,
    /// Offset from placeholder bearing.
    pub bearing_offset_deg: Option<f64>,
    /// Whether this anchor is consistent enough to drive the scene.
    pub consistent: bool,
    /// Short inconsistency reason.
    pub reason: String,
}

/// Display-facing summary of scene correction state.
#[derive(Debug, Clone)]
pub struct CorrectionSummary {
    /// Active calibration mode.
    pub mode: CalibrationMode,
    /// Calibrated devices currently visible.
    pub visible_anchors: usize,
    /// Consistent anchors used for median scale/offset.
    pub active_anchors: usize,
    /// Median distance scale applied to non-anchors.
    pub distance_scale: f64,
    /// Median bearing offset applied in anchor mode.
    pub bearing_offset_deg: f64,
}

impl CorrectionSummary {
    /// Compact status label for headers / footers.
    pub fn short_label(&self) -> String {
        let mode_label = match self.mode {
            CalibrationMode::Honest => "honest",
            CalibrationMode::Anchor => "anchor",
        };
        format!(
            "mode={mode_label} anchors={}/{} distx={:.2} brg={:+.0}°",
            self.active_anchors, self.visible_anchors, self.distance_scale, self.bearing_offset_deg
        )
    }
}

fn wrap_degrees(deg: f64) -> f64 {
    let mut wrapped = (deg + 180.0).rem_euclid(360.0) - 180.0;
    if (wrapped + 180.0).abs() < f64::EPSILON {
        wrapped = 180.0;
    }
    wrapped
}

fn default_ref(dev: &WifiDevice) -> f64 {
    match dev.kind {
        DeviceKind::Hotspot => RSSI_AT_1M_AP,
        DeviceKind::Client | DeviceKind::Adapter => RSSI_AT_1M_CLIENT,
    }
}

fn distance_from_ref(dev: &WifiDevice, ref_dbm: f64) -> f64 {
    let mut n = PATH_LOSS_EXPONENT_INDOOR;
    if dev.freq_mhz > 4000 {
        n += 0.3;
    }
    let exponent = (ref_dbm - dev.rssi_dbm) / (10.0 * n);
    10f64.powf(exponent).clamp(0.1, 120.0)
}

fn median_f64(values: &mut [f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mid = values.len() / 2;
    if values.len() % 2 == 1 {
        Some(values[mid])
    } else {
        Some((values[mid - 1] + values[mid]) / 2.0)
    }
}

fn build_anchor_diagnostic(
    dev: &WifiDevice,
    calibration: &CalibrationStore,
) -> AnchorDiagnostic {
    let distance_target = calibration.distance_anchor_m(&dev.mac);
    let mut distance_scale = None;
    let mut current_distance = None;
    if let Some(target) = distance_target {
        let current = dev.distance_m();
        current_distance = Some(current);
        let raw_default = distance_from_ref(dev, default_ref(dev));
        distance_scale = Some(target / raw_default.max(0.1));
    }

    let bearing_target = calibration.manual_bearing(&dev.mac);
    let bearing_offset =
        bearing_target.map(|t| wrap_degrees(t - dev.placeholder_bearing()));

    let mut consistent = true;
    let mut reason = String::new();
    if let (Some(target), Some(current)) = (distance_target, current_distance) {
        let ratio = current.max(0.1) / target.max(0.1);
        if !(0.55..=1.8).contains(&ratio) {
            consistent = false;
            reason = "distance drift".into();
        }
    }
    if let (Some(target), Some(bearing)) = (bearing_target, dev.bearing_deg) {
        if !dev.bearing_manual {
            let disagreement = wrap_degrees(target - bearing).abs();
            if disagreement > 90.0 {
                consistent = false;
                if reason.is_empty() {
                    reason = "bearing disagreement".into();
                }
            }
        }
    }

    AnchorDiagnostic {
        mac: dev.mac.clone(),
        distance_target_m: distance_target,
        current_distance_m: current_distance,
        distance_scale,
        bearing_target_deg: bearing_target,
        bearing_offset_deg: bearing_offset,
        consistent,
        reason,
    }
}

/// Apply scene correction heuristics to a device snapshot (mutates in place).
pub fn apply_scene_corrections(
    devices: &mut [WifiDevice],
    calibration: &CalibrationStore,
) -> CorrectionSummary {
    let mode = calibration.mode();
    let mut diagnostics = Vec::new();
    for dev in devices.iter() {
        if calibration.has_distance_calibration(&dev.mac)
            || calibration.has_manual_bearing(&dev.mac)
        {
            diagnostics.push(build_anchor_diagnostic(dev, calibration));
        }
    }

    let mut scales: Vec<f64> = diagnostics
        .iter()
        .filter(|d| d.consistent)
        .filter_map(|d| d.distance_scale)
        .collect();
    let mut offsets: Vec<f64> = diagnostics
        .iter()
        .filter(|d| d.consistent)
        .filter_map(|d| d.bearing_offset_deg)
        .collect();
    let had_scales = !scales.is_empty();
    let distance_scale = median_f64(&mut scales).unwrap_or(1.0);
    let bearing_offset = median_f64(&mut offsets).unwrap_or(0.0);
    let summary = CorrectionSummary {
        mode,
        visible_anchors: diagnostics.len(),
        active_anchors: diagnostics.iter().filter(|d| d.consistent).count(),
        distance_scale,
        bearing_offset_deg: bearing_offset,
    };

    let diag_by_mac: HashMap<&str, &AnchorDiagnostic> =
        diagnostics.iter().map(|d| (d.mac.as_str(), d)).collect();

    for dev in devices.iter_mut() {
        if let Some(diag) = diag_by_mac.get(dev.mac.as_str()) {
            if diag.consistent {
                dev.anchor_status = "anchor".into();
            } else {
                dev.anchor_status = "stale-cal".into();
                if !diag.reason.is_empty() {
                    dev.correction_note = diag.reason.clone();
                }
            }
            continue;
        }

        match mode {
            CalibrationMode::Honest => {
                if had_scales {
                    let raw = distance_from_ref(dev, default_ref(dev));
                    dev.distance_override_m = Some(raw * distance_scale);
                    dev.correction_note = "scene distance".into();
                }
            }
            CalibrationMode::Anchor => {
                let raw = distance_from_ref(dev, default_ref(dev));
                dev.distance_override_m = Some(raw * distance_scale);
                // Match Python: placeholder + offset (display bearing normalizes).
                dev.bearing_override_deg = Some(dev.placeholder_bearing() + bearing_offset);
                dev.correction_note = "scene anchor".into();
            }
        }
    }

    summary
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calibration::CalibrationStore;

    #[test]
    fn honest_scales_non_anchors_when_distance_anchor_present() {
        let mut cal = CalibrationStore::memory();
        cal.set_distance_reference("aa:aa:aa:00:00:01", -40.0, 1.0)
            .unwrap();
        let mut anchor = WifiDevice::hotspot("aa:aa:aa:00:00:01", -40.0);
        anchor.rssi_at_1m_override = Some(-40.0);
        let other = WifiDevice::hotspot("aa:aa:aa:00:00:02", -55.0);
        let mut devices = vec![anchor, other];
        cal.apply_to_devices(&mut devices);
        let summary = apply_scene_corrections(&mut devices, &cal);
        assert_eq!(summary.mode, CalibrationMode::Honest);
        assert!(summary.active_anchors >= 1);
        assert_eq!(devices[0].anchor_status, "anchor");
        assert!(devices[1].distance_override_m.is_some());
        assert_eq!(devices[1].correction_note, "scene distance");
    }

    #[test]
    fn anchor_mode_sets_bearing_override() {
        let mut cal = CalibrationStore::memory();
        cal.set_mode(CalibrationMode::Anchor).unwrap();
        cal.set_manual_bearing("aa:aa:aa:00:00:01", 90.0).unwrap();
        let anchor = WifiDevice::hotspot("aa:aa:aa:00:00:01", -45.0);
        let other = WifiDevice::hotspot("aa:aa:aa:00:00:02", -60.0);
        let mut devices = vec![anchor, other];
        cal.apply_to_devices(&mut devices);
        let summary = apply_scene_corrections(&mut devices, &cal);
        assert_eq!(summary.mode, CalibrationMode::Anchor);
        assert!(devices[1].bearing_override_deg.is_some());
        assert_eq!(devices[1].correction_note, "scene anchor");
    }

    #[test]
    fn honest_without_distance_anchor_leaves_others_alone() {
        let mut cal = CalibrationStore::memory();
        cal.set_manual_bearing("aa:aa:aa:00:00:01", 45.0).unwrap();
        let anchor = WifiDevice::hotspot("aa:aa:aa:00:00:01", -45.0);
        let other = WifiDevice::hotspot("aa:aa:aa:00:00:02", -60.0);
        let mut devices = vec![anchor, other];
        cal.apply_to_devices(&mut devices);
        let _ = apply_scene_corrections(&mut devices, &cal);
        assert_eq!(devices[0].anchor_status, "anchor");
        assert!(devices[1].distance_override_m.is_none());
        assert!(devices[1].correction_note.is_empty());
    }

    #[test]
    fn short_label_mentions_mode() {
        let s = CorrectionSummary {
            mode: CalibrationMode::Honest,
            visible_anchors: 1,
            active_anchors: 1,
            distance_scale: 1.25,
            bearing_offset_deg: -10.0,
        };
        let label = s.short_label();
        assert!(label.contains("honest"));
        assert!(label.contains("distx=1.25"));
    }
}
