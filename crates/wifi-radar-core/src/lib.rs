//! Domain core for wifi-radar (models, store, scan traits).
//!
//! Passive observation only. No deauth, injection, association, or cracking.

#![deny(missing_docs)]

pub mod bearing_calib;
pub mod calibration;
pub mod correction;
pub mod models;
pub mod node_obs;
pub mod scan;
pub mod store;
pub mod util;

pub use bearing_calib::{BearingCalibrator, MIN_RSSI_SPREAD_DB, MIN_SAMPLES};
pub use calibration::{CalibrationMode, CalibrationStore};
pub use correction::{apply_scene_corrections, AnchorDiagnostic, CorrectionSummary};
pub use models::{
    now_unix, placeholder_bearing_for_mac, DeviceKind, WifiDevice, PATH_LOSS_EXPONENT_INDOOR,
    RSSI_AT_1M_AP, RSSI_AT_1M_CLIENT,
};
pub use node_obs::{
    DeviceObservation, NodeObservation, NodePose, NODE_OBS_SCHEMA_VERSION,
};
pub use scan::{ScanBackend, ScanError};
pub use store::{DeviceStore, DEFAULT_TTL_SEC, RSSI_EMA_ALPHA};
pub use util::normalize_mac;

/// Crate version string (mirrors workspace package version).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
