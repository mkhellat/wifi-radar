//! Domain core for wifi-radar (models, store, scan traits).
//!
//! Passive observation only. No deauth, injection, association, or cracking.

#![deny(missing_docs)]

pub mod models;
pub mod scan;
pub mod util;

pub use models::{
    now_unix, placeholder_bearing_for_mac, DeviceKind, WifiDevice, PATH_LOSS_EXPONENT_INDOOR,
    RSSI_AT_1M_AP, RSSI_AT_1M_CLIENT,
};
pub use scan::{ScanBackend, ScanError};
pub use util::normalize_mac;

/// Crate version string (mirrors workspace package version).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
