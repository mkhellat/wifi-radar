//! PyO3 façade for wifi-radar (`wifi_radar_native`).
//!
//! Exposes core version, [`DeviceStore`], [`CalibrationStore`], and scene
//! correction. Passive observation only — no injection, deauth, or association.

#![deny(missing_docs)]

use pyo3::prelude::*;
use pyo3::types::PyDict;
use wifi_radar_core::{
    apply_scene_corrections as apply_scene_corrections_core, CalibrationMode,
    CalibrationStore as CoreCalib, DeviceStore as CoreStore, WifiDevice,
    VERSION as CORE_VERSION,
};

/// Linked Rust core version string.
#[pyfunction]
fn version() -> &'static str {
    CORE_VERSION
}

/// In-memory device registry with EMA RSSI merge (Rust core).
#[pyclass(name = "DeviceStore")]
struct PyDeviceStore {
    inner: CoreStore,
}

#[pymethods]
impl PyDeviceStore {
    /// Create an empty store with the default TTL.
    #[new]
    fn new() -> Self {
        Self {
            inner: CoreStore::new(),
        }
    }

    /// Merge one hotspot observation (SSID / channel / freq optional).
    #[pyo3(signature = (mac, rssi_dbm, ssid=None, channel=None, freq_mhz=None))]
    fn merge_hotspot(
        &mut self,
        mac: &str,
        rssi_dbm: f64,
        ssid: Option<&str>,
        channel: Option<u32>,
        freq_mhz: Option<u32>,
    ) {
        let mut d = WifiDevice::hotspot(mac, rssi_dbm);
        if let Some(s) = ssid {
            d.ssid = s.to_string();
        }
        if let Some(c) = channel {
            d.channel = c;
        }
        if let Some(f) = freq_mhz {
            d.freq_mhz = f;
        }
        self.inner.merge([d]);
    }

    /// Number of devices currently in the store (before expiry).
    fn len(&self) -> usize {
        self.inner.devices().len()
    }

    /// Expire stale entries and return devices as dicts (strongest RSSI first).
    fn snapshot<'py>(&mut self, py: Python<'py>) -> PyResult<Vec<Bound<'py, PyDict>>> {
        let devices = self.inner.snapshot();
        devices_to_dicts(py, &devices)
    }

    /// Snapshot with per-MAC calibration and scene correction applied.
    ///
    /// Returns `(rows, summary)` where `summary` matches
    /// [`CorrectionSummary::short_label`](wifi_radar_core::CorrectionSummary)
    /// fields as a dict.
    fn snapshot_calibrated<'py>(
        &mut self,
        py: Python<'py>,
        calibration: &PyCalibrationStore,
    ) -> PyResult<(Vec<Bound<'py, PyDict>>, Bound<'py, PyDict>)> {
        let mut devices = self.inner.snapshot();
        calibration.inner.apply_to_devices(&mut devices);
        let summary = apply_scene_corrections_core(&mut devices, &calibration.inner);
        let rows = devices_to_dicts(py, &devices)?;
        let s = summary_to_dict(py, &summary)?;
        Ok((rows, s))
    }
}

/// Persistent / in-memory per-MAC calibration (Python JSON-compatible).
#[pyclass(name = "CalibrationStore")]
struct PyCalibrationStore {
    inner: CoreCalib,
}

#[pymethods]
impl PyCalibrationStore {
    /// Load from the default cache path (`~/.cache/wifi_radar/calibration.json`).
    #[new]
    fn new() -> Self {
        Self {
            inner: CoreCalib::new(CoreCalib::default_path()),
        }
    }

    /// In-memory store (no persistence).
    #[staticmethod]
    fn memory() -> Self {
        Self {
            inner: CoreCalib::memory(),
        }
    }

    /// Active mode (`honest` or `anchor`).
    fn mode(&self) -> &'static str {
        match self.inner.mode() {
            CalibrationMode::Honest => "honest",
            CalibrationMode::Anchor => "anchor",
        }
    }

    /// Set mode (`honest` / `anchor`).
    fn set_mode(&mut self, mode: &str) -> PyResult<()> {
        let parsed = match mode.trim().to_ascii_lowercase().as_str() {
            "honest" => CalibrationMode::Honest,
            "anchor" => CalibrationMode::Anchor,
            other => {
                return Err(pyo3::exceptions::PyValueError::new_err(format!(
                    "unknown mode: {other}"
                )))
            }
        };
        self.inner
            .set_mode(parsed)
            .map_err(pyo3::exceptions::PyRuntimeError::new_err)
    }

    /// Pin a world bearing in degrees.
    fn set_manual_bearing(&mut self, mac: &str, bearing_deg: f64) -> PyResult<()> {
        self.inner
            .set_manual_bearing(mac, bearing_deg)
            .map_err(pyo3::exceptions::PyRuntimeError::new_err)
    }

    /// Manual bearing for `mac`, if any.
    fn manual_bearing(&self, mac: &str) -> Option<f64> {
        self.inner.manual_bearing(mac)
    }

    /// True when `mac` has a stored manual bearing pin.
    fn has_manual_bearing(&self, mac: &str) -> bool {
        self.inner.has_manual_bearing(mac)
    }

    /// RSSI-at-1m override for `mac`, if any.
    fn rssi_at_1m(&self, mac: &str) -> Option<f64> {
        self.inner.rssi_at_1m(mac)
    }

    /// Distance anchor metres for `mac`, if any.
    fn distance_anchor_m(&self, mac: &str) -> Option<f64> {
        self.inner.distance_anchor_m(mac)
    }

    /// True when `mac` has a stored distance / RSSI-at-1m calibration.
    fn has_distance_calibration(&self, mac: &str) -> bool {
        self.inner.has_distance_calibration(mac)
    }

    /// Calibrate distance so current RSSI maps to `distance_m`. Returns 1 m ref.
    fn set_distance_reference(
        &mut self,
        mac: &str,
        rssi_dbm: f64,
        distance_m: f64,
    ) -> PyResult<f64> {
        self.inner
            .set_distance_reference(mac, rssi_dbm, distance_m)
            .map_err(pyo3::exceptions::PyRuntimeError::new_err)
    }

    /// Clear pin + distance for one MAC.
    fn clear(&mut self, mac: &str) -> PyResult<()> {
        self.inner
            .clear(mac)
            .map_err(pyo3::exceptions::PyRuntimeError::new_err)
    }
}

fn devices_to_dicts<'py>(
    py: Python<'py>,
    devices: &[WifiDevice],
) -> PyResult<Vec<Bound<'py, PyDict>>> {
    let mut out = Vec::with_capacity(devices.len());
    for d in devices {
        out.push(device_to_dict(py, d)?);
    }
    Ok(out)
}

fn device_to_dict<'py>(py: Python<'py>, d: &WifiDevice) -> PyResult<Bound<'py, PyDict>> {
    let dict = PyDict::new(py);
    dict.set_item("mac", d.mac.as_str())?;
    dict.set_item(
        "kind",
        match d.kind {
            wifi_radar_core::DeviceKind::Hotspot => "hotspot",
            wifi_radar_core::DeviceKind::Client => "client",
            wifi_radar_core::DeviceKind::Adapter => "adapter",
        },
    )?;
    dict.set_item("ssid", d.ssid.as_str())?;
    dict.set_item("channel", d.channel)?;
    dict.set_item("freq_mhz", d.freq_mhz)?;
    dict.set_item("rssi_dbm", d.rssi_dbm)?;
    dict.set_item("distance_m", d.distance_m())?;
    // `bearing_deg` stays display-facing for snapshot consumers; raw pin in
    // `calibrated_bearing_deg` for apply_scene_corrections round-trips.
    dict.set_item("bearing_deg", d.display_bearing())?;
    dict.set_item("calibrated_bearing_deg", d.bearing_deg)?;
    dict.set_item("bearing_override_deg", d.bearing_override_deg)?;
    dict.set_item("bearing_confidence", d.bearing_confidence)?;
    dict.set_item("bearing_manual", d.bearing_manual)?;
    dict.set_item("distance_override_m", d.distance_override_m)?;
    dict.set_item("rssi_at_1m_override", d.rssi_at_1m_override)?;
    dict.set_item("label", d.label())?;
    dict.set_item("anchor_status", d.anchor_status.as_str())?;
    dict.set_item("correction_note", d.correction_note.as_str())?;
    Ok(dict)
}

fn opt_f64(dict: &Bound<'_, PyDict>, key: &str) -> PyResult<Option<f64>> {
    match dict.get_item(key)? {
        None => Ok(None),
        Some(v) if v.is_none() => Ok(None),
        Some(v) => Ok(Some(v.extract::<f64>()?)),
    }
}

fn device_from_dict(dict: &Bound<'_, PyDict>) -> PyResult<WifiDevice> {
    let mac: String = dict
        .get_item("mac")?
        .ok_or_else(|| pyo3::exceptions::PyKeyError::new_err("mac"))?
        .extract()?;
    let rssi_dbm: f64 = dict
        .get_item("rssi_dbm")?
        .ok_or_else(|| pyo3::exceptions::PyKeyError::new_err("rssi_dbm"))?
        .extract()?;
    let kind_raw: String = match dict.get_item("kind")? {
        Some(v) => v.extract().unwrap_or_else(|_| "hotspot".into()),
        None => "hotspot".into(),
    };
    let mut d = match kind_raw.as_str() {
        "client" => {
            let mut c = WifiDevice::hotspot(&mac, rssi_dbm);
            c.kind = wifi_radar_core::DeviceKind::Client;
            c
        }
        "adapter" => {
            let mut a = WifiDevice::hotspot(&mac, rssi_dbm);
            a.kind = wifi_radar_core::DeviceKind::Adapter;
            a
        }
        _ => WifiDevice::hotspot(&mac, rssi_dbm),
    };
    if let Some(v) = dict.get_item("ssid")? {
        if !v.is_none() {
            d.ssid = v.extract()?;
        }
    }
    if let Some(v) = dict.get_item("channel")? {
        if !v.is_none() {
            d.channel = v.extract()?;
        }
    }
    if let Some(v) = dict.get_item("freq_mhz")? {
        if !v.is_none() {
            d.freq_mhz = v.extract()?;
        }
    }
    d.bearing_deg = opt_f64(dict, "calibrated_bearing_deg")?;
    d.bearing_override_deg = opt_f64(dict, "bearing_override_deg")?;
    d.distance_override_m = opt_f64(dict, "distance_override_m")?;
    d.rssi_at_1m_override = opt_f64(dict, "rssi_at_1m_override")?;
    if let Some(v) = dict.get_item("bearing_confidence")? {
        if !v.is_none() {
            d.bearing_confidence = v.extract()?;
        }
    }
    if let Some(v) = dict.get_item("bearing_manual")? {
        if !v.is_none() {
            d.bearing_manual = v.extract()?;
        }
    }
    // Accept legacy input key only when it is an explicit pin (not display bearing).
    if d.bearing_deg.is_none() && d.bearing_manual {
        d.bearing_deg = opt_f64(dict, "bearing_deg")?;
    }
    if let Some(v) = dict.get_item("anchor_status")? {
        if !v.is_none() {
            d.anchor_status = v.extract()?;
        }
    }
    if let Some(v) = dict.get_item("correction_note")? {
        if !v.is_none() {
            d.correction_note = v.extract()?;
        }
    }
    Ok(d)
}

fn summary_to_dict<'py>(
    py: Python<'py>,
    summary: &wifi_radar_core::CorrectionSummary,
) -> PyResult<Bound<'py, PyDict>> {
    let s = PyDict::new(py);
    s.set_item(
        "mode",
        match summary.mode {
            CalibrationMode::Honest => "honest",
            CalibrationMode::Anchor => "anchor",
        },
    )?;
    s.set_item("visible_anchors", summary.visible_anchors)?;
    s.set_item("active_anchors", summary.active_anchors)?;
    s.set_item("distance_scale", summary.distance_scale)?;
    s.set_item("bearing_offset_deg", summary.bearing_offset_deg)?;
    s.set_item("short_label", summary.short_label())?;
    Ok(s)
}

/// Apply per-MAC calibration overrides then scene correction heuristics.
///
/// `devices` is a list of dicts (see snapshot row shape, plus optional
/// `rssi_at_1m_override` / `bearing_deg` / override fields). Returns
/// `(rows, summary)` like [`PyDeviceStore::snapshot_calibrated`].
#[pyfunction]
fn apply_scene_corrections<'py>(
    py: Python<'py>,
    devices: Vec<Bound<'py, PyDict>>,
    calibration: &PyCalibrationStore,
) -> PyResult<(Vec<Bound<'py, PyDict>>, Bound<'py, PyDict>)> {
    let mut rust_devs = Vec::with_capacity(devices.len());
    for d in &devices {
        rust_devs.push(device_from_dict(d)?);
    }
    calibration.inner.apply_to_devices(&mut rust_devs);
    let summary = apply_scene_corrections_core(&mut rust_devs, &calibration.inner);
    let rows = devices_to_dicts(py, &rust_devs)?;
    let s = summary_to_dict(py, &summary)?;
    Ok((rows, s))
}

/// Python module `wifi_radar_native`.
#[pymodule]
fn wifi_radar_native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(version, m)?)?;
    m.add_function(wrap_pyfunction!(apply_scene_corrections, m)?)?;
    m.add_class::<PyDeviceStore>()?;
    m.add_class::<PyCalibrationStore>()?;
    m.add("__version__", CORE_VERSION)?;
    Ok(())
}

/// Re-export for non-Python smoke tests.
pub fn core_version() -> &'static str {
    CORE_VERSION
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn core_version_nonempty() {
        assert!(!core_version().is_empty());
    }

    #[test]
    fn store_merge_snapshot_dicts_shape() {
        pyo3::prepare_freethreaded_python();
        Python::with_gil(|py| {
            let mut store = PyDeviceStore::new();
            store.merge_hotspot("AA:BB:CC:DD:EE:01", -70.0, Some("far"), Some(6), Some(2437));
            store.merge_hotspot("aa:bb:cc:dd:ee:02", -40.0, Some("near"), None, None);
            let snap = store.snapshot(py).expect("snapshot");
            assert_eq!(snap.len(), 2);
            let mac: String = snap[0]
                .get_item("mac")
                .unwrap()
                .unwrap()
                .extract()
                .unwrap();
            assert_eq!(mac, "aa:bb:cc:dd:ee:02");
        });
    }

    #[test]
    fn snapshot_calibrated_marks_anchor() {
        pyo3::prepare_freethreaded_python();
        Python::with_gil(|py| {
            let mut store = PyDeviceStore::new();
            store.merge_hotspot("aa:aa:aa:00:00:01", -40.0, Some("a"), None, None);
            store.merge_hotspot("aa:aa:aa:00:00:02", -55.0, Some("b"), None, None);
            let mut cal = PyCalibrationStore::memory();
            cal.set_distance_reference("aa:aa:aa:00:00:01", -40.0, 1.0)
                .unwrap();
            assert!(cal.has_distance_calibration("aa:aa:aa:00:00:01"));
            assert!(cal.rssi_at_1m("aa:aa:aa:00:00:01").is_some());
            let (rows, summary) = store.snapshot_calibrated(py, &cal).unwrap();
            assert_eq!(rows.len(), 2);
            let mode: String = summary
                .get_item("mode")
                .unwrap()
                .unwrap()
                .extract()
                .unwrap();
            assert_eq!(mode, "honest");
            let note: String = rows[1]
                .get_item("correction_note")
                .unwrap()
                .unwrap()
                .extract()
                .unwrap();
            // Strongest first: index 0 is -40 anchor; index 1 is -55 other.
            let status0: String = rows[0]
                .get_item("anchor_status")
                .unwrap()
                .unwrap()
                .extract()
                .unwrap();
            assert_eq!(status0, "anchor");
            assert_eq!(note, "scene distance");
        });
    }

    #[test]
    fn apply_scene_corrections_fn_scales_newcomer() {
        pyo3::prepare_freethreaded_python();
        Python::with_gil(|py| {
            let mut cal = PyCalibrationStore::memory();
            cal.set_distance_reference("aa:aa:aa:00:00:01", -40.0, 1.0)
                .unwrap();
            let anchor = PyDict::new(py);
            anchor.set_item("mac", "aa:aa:aa:00:00:01").unwrap();
            anchor.set_item("kind", "hotspot").unwrap();
            anchor.set_item("rssi_dbm", -40.0).unwrap();
            anchor.set_item("rssi_at_1m_override", -40.0).unwrap();
            let other = PyDict::new(py);
            other.set_item("mac", "aa:aa:aa:00:00:02").unwrap();
            other.set_item("kind", "hotspot").unwrap();
            other.set_item("rssi_dbm", -55.0).unwrap();
            let (rows, summary) =
                apply_scene_corrections(py, vec![anchor, other], &cal).unwrap();
            assert_eq!(rows.len(), 2);
            let mode: String = summary
                .get_item("mode")
                .unwrap()
                .unwrap()
                .extract()
                .unwrap();
            assert_eq!(mode, "honest");
            let status0: String = rows[0]
                .get_item("anchor_status")
                .unwrap()
                .unwrap()
                .extract()
                .unwrap();
            assert_eq!(status0, "anchor");
            let note: String = rows[1]
                .get_item("correction_note")
                .unwrap()
                .unwrap()
                .extract()
                .unwrap();
            assert_eq!(note, "scene distance");
            let override_m = rows[1]
                .get_item("distance_override_m")
                .unwrap()
                .unwrap();
            assert!(!override_m.is_none());
        });
    }
}
