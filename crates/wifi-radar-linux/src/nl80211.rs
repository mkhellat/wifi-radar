//! Native nl80211 scan backend.
//!
//! BSS attribute decoding lives in [`crate::bss`]. Live genetlink GET_SCAN
//! lands in a follow-up PR; this backend still errors on [`ScanBackend::scan`]
//! so callers keep using [`crate::IwCliBackend`] until then.

use wifi_radar_core::{ScanBackend, ScanError, WifiDevice};

pub use crate::bss::{
    device_from_bss_attrs, devices_from_nl80211_attrs, encode_nla, parse_nla_stream, RawAttr,
};

/// Native nl80211 backend (decode ready; socket dump not wired yet).
pub struct Nl80211Backend {
    iface: String,
}

impl Nl80211Backend {
    /// Create an nl80211 backend for `iface`.
    pub fn new(iface: impl Into<String>) -> Self {
        Self {
            iface: iface.into(),
        }
    }

    /// Interface name this backend will scan.
    pub fn iface(&self) -> &str {
        &self.iface
    }
}

impl ScanBackend for Nl80211Backend {
    fn name(&self) -> &'static str {
        "nl80211"
    }

    fn scan(&mut self) -> Result<Vec<WifiDevice>, ScanError> {
        Err(ScanError::Message(format!(
            "nl80211 genetlink dump not wired yet on {}; BSS decode is ready in wifi_radar_linux::bss",
            self.iface
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_reports_name_and_iface() {
        let mut b = Nl80211Backend::new("wlan0");
        assert_eq!(b.name(), "nl80211");
        assert_eq!(b.iface(), "wlan0");
        let err = b.scan().expect_err("socket dump not wired");
        assert!(err.to_string().contains("wlan0"));
    }
}
