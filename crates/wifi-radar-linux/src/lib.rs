//! Linux RF backends for wifi-radar.
//!
//! Native nl80211 is the intended scan path. [`IwCliBackend`] is the interim
//! shim for parity with the Python `iw` scanner.

#![deny(missing_docs)]

pub mod bss;
pub mod iw;
pub mod nl80211;

pub use bss::{device_from_bss_attrs, devices_from_nl80211_attrs, RawAttr};
pub use iw::IwCliBackend;
pub use nl80211::{trigger_scan, Nl80211Backend, DEFAULT_TRIGGER_WAIT};

use wifi_radar_core::VERSION as CORE_VERSION;

/// Crate version string.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Report linked core version (sanity check for workspace wiring).
pub fn core_version() -> &'static str {
    CORE_VERSION
}
