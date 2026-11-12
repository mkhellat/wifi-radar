//! Multi-node observation schema (JSON-serializable).
//!
//! Passive fusion input only — no transport or solver here. Public docs must not
//! claim survey-grade or finished 3D localization.

use serde::{Deserialize, Serialize};

use crate::models::{DeviceKind, WifiDevice};
use crate::util::normalize_mac;

/// Schema version for observation payloads.
pub const NODE_OBS_SCHEMA_VERSION: u32 = 1;

/// Pose of a sensing node in a shared local frame (metres, degrees).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodePose {
    /// Local X (metres).
    pub x: f64,
    /// Local Y (metres).
    pub y: f64,
    /// Local Z (metres); 0.0 when unknown / 2D-only.
    pub z: f64,
    /// Compass heading of the node (degrees, \[0, 360)).
    pub heading_deg: f64,
}

impl NodePose {
    /// Construct a pose; heading is normalized to \[0, 360).
    pub fn new(x: f64, y: f64, z: f64, heading_deg: f64) -> Self {
        Self {
            x,
            y,
            z,
            heading_deg: heading_deg.rem_euclid(360.0),
        }
    }
}

/// One device sighting from a sensing node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeviceObservation {
    /// Normalized MAC (lowercase, colon-separated).
    pub mac: String,
    /// Device kind.
    pub kind: DeviceKind,
    /// Smoothed or last RSSI (dBm).
    pub rssi_dbm: f64,
    /// Channel when known (0 if unknown).
    pub channel: u32,
    /// Center frequency in MHz (0 if unknown).
    pub freq_mhz: u32,
    /// Local calibrated / display bearing when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bearing_deg: Option<f64>,
    /// Bearing confidence in \[0, 1\].
    #[serde(default)]
    pub bearing_confidence: f64,
    /// Short display label.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label: String,
    /// OUI vendor string when known.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub vendor: String,
}

impl DeviceObservation {
    /// Build from a [`WifiDevice`] snapshot (normalizes MAC).
    pub fn from_wifi_device(dev: &WifiDevice) -> Self {
        Self {
            mac: normalize_mac(&dev.mac),
            kind: dev.kind,
            rssi_dbm: dev.rssi_dbm,
            channel: dev.channel,
            freq_mhz: dev.freq_mhz,
            bearing_deg: Some(dev.display_bearing()),
            bearing_confidence: dev.bearing_confidence,
            label: dev.label(),
            vendor: dev.vendor.clone(),
        }
    }
}

/// Time-bucketed observation batch from one sensing node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeObservation {
    /// Schema version ([`NODE_OBS_SCHEMA_VERSION`]).
    pub schema_version: u32,
    /// Stable node identifier (hostname, UUID, or operator label).
    pub node_id: String,
    /// Wall-clock Unix seconds when the batch was produced.
    pub timestamp_wall: f64,
    /// Monotonic seconds (node-local) for ordering within one host.
    pub timestamp_mono: f64,
    /// Node pose in the shared frame.
    pub node_pose: NodePose,
    /// Devices seen in this bucket.
    pub device_observations: Vec<DeviceObservation>,
}

impl NodeObservation {
    /// Construct a batch with the current schema version.
    pub fn new(
        node_id: impl Into<String>,
        timestamp_wall: f64,
        timestamp_mono: f64,
        node_pose: NodePose,
        device_observations: Vec<DeviceObservation>,
    ) -> Self {
        Self {
            schema_version: NODE_OBS_SCHEMA_VERSION,
            node_id: node_id.into(),
            timestamp_wall,
            timestamp_mono,
            node_pose,
            device_observations,
        }
    }

    /// Serialize to pretty JSON (+ trailing newline).
    pub fn to_json_pretty(&self) -> Result<String, String> {
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        Ok(text + "\n")
    }

    /// Parse from JSON text.
    pub fn from_json(text: &str) -> Result<Self, String> {
        serde_json::from_str(text).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::WifiDevice;

    #[test]
    fn json_roundtrip_preserves_fields() {
        let pose = NodePose::new(1.0, 2.0, 0.5, 450.0);
        assert!((pose.heading_deg - 90.0).abs() < 1e-9);
        let mut hot = WifiDevice::hotspot("AA:BB:CC:00:00:01", -45.0);
        hot.ssid = "Lab-AP".into();
        hot.channel = 6;
        hot.freq_mhz = 2437;
        hot.vendor = "TestOUI".into();
        hot.bearing_deg = Some(30.0);
        hot.bearing_confidence = 0.8;
        let obs = NodeObservation::new(
            "node-a",
            1_700_000_000.0,
            12.5,
            pose,
            vec![DeviceObservation::from_wifi_device(&hot)],
        );
        let json = obs.to_json_pretty().unwrap();
        assert!(json.contains("\"schema_version\": 1"));
        assert!(json.contains("\"node-a\""));
        let back = NodeObservation::from_json(&json).unwrap();
        assert_eq!(back.schema_version, NODE_OBS_SCHEMA_VERSION);
        assert_eq!(back.node_id, "node-a");
        assert_eq!(back.device_observations.len(), 1);
        assert_eq!(back.device_observations[0].mac, "aa:bb:cc:00:00:01");
        assert_eq!(back.device_observations[0].kind, DeviceKind::Hotspot);
        assert!((back.node_pose.heading_deg - 90.0).abs() < 1e-9);
    }

    #[test]
    fn rejects_invalid_json() {
        assert!(NodeObservation::from_json("{not json").is_err());
    }
}
