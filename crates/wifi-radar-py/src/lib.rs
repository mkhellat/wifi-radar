//! PyO3 façade for wifi-radar (`wifi_radar_native`).
//!
//! Exposes core version and a thin [`DeviceStore`] wrapper for scripting.
//! Passive observation only — no injection, deauth, or association.

#![deny(missing_docs)]

use pyo3::prelude::*;
use pyo3::types::PyDict;
use wifi_radar_core::{DeviceStore as CoreStore, WifiDevice, VERSION as CORE_VERSION};

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
        let mut out = Vec::with_capacity(devices.len());
        for d in devices {
            out.push(device_to_dict(py, &d)?);
        }
        Ok(out)
    }
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
    dict.set_item("bearing_deg", d.display_bearing())?;
    dict.set_item("label", d.label())?;
    Ok(dict)
}

/// Python module `wifi_radar_native`.
#[pymodule]
fn wifi_radar_native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(version, m)?)?;
    m.add_class::<PyDeviceStore>()?;
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
            // Strongest first; MAC normalized.
            let mac: String = snap[0]
                .get_item("mac")
                .unwrap()
                .unwrap()
                .extract()
                .unwrap();
            assert_eq!(mac, "aa:bb:cc:dd:ee:02");
            let ssid: String = snap[0]
                .get_item("ssid")
                .unwrap()
                .unwrap()
                .extract()
                .unwrap();
            assert_eq!(ssid, "near");
        });
    }
}
