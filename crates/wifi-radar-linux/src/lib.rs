//! Linux RF backends for wifi-radar.
//!
//! Native nl80211 is the intended scan path. An `iw` CLI shim may exist for
//! early parity while nl80211 lands.

#![deny(missing_docs)]

use wifi_radar_core::VERSION as CORE_VERSION;

/// Crate version string.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Report linked core version (sanity check for workspace wiring).
pub fn core_version() -> &'static str {
    CORE_VERSION
}
