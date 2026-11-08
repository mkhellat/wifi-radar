//! Linux RF backends for wifi-radar.
//!
//! Native nl80211 is the intended scan path. [`IwCliBackend`] is the interim
//! shim for parity with the Python `iw` scanner.

#![deny(missing_docs)]

pub mod iw;

pub use iw::{IwCliBackend, Nl80211Backend};

use wifi_radar_core::VERSION as CORE_VERSION;

/// Crate version string.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Report linked core version (sanity check for workspace wiring).
pub fn core_version() -> &'static str {
    CORE_VERSION
}
