//! Native nl80211 scan backend (stub until netlink scan lands).
//!
//! Kept in its own module so the interim [`crate::iw::IwCliBackend`] stays a
//! pure CLI shim while this path grows netlink trigger/dump parsing.

use wifi_radar_core::{ScanBackend, ScanError, WifiDevice};

/// Placeholder for the native nl80211 backend (tracked target).
pub struct Nl80211Backend {
    iface: String,
}

impl Nl80211Backend {
    /// Create a stub nl80211 backend for `iface`.
    pub fn new(iface: impl Into<String>) -> Self {
        Self {
            iface: iface.into(),
        }
    }

    /// Interface name this backend will scan (once implemented).
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
            "nl80211 backend not implemented yet on {}; use IwCliBackend for now",
            self.iface
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stub_reports_name_and_iface() {
        let mut b = Nl80211Backend::new("wlan0");
        assert_eq!(b.name(), "nl80211");
        assert_eq!(b.iface(), "wlan0");
        let err = b.scan().expect_err("stub must not scan yet");
        let msg = err.to_string();
        assert!(msg.contains("nl80211"));
        assert!(msg.contains("wlan0"));
    }
}
