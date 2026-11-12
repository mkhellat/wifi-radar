//! Sensing node agent helpers: build and emit [`NodeObservation`] batches.
//!
//! Passive observation only — no association, deauth, or injection.

#![deny(missing_docs)]

use std::time::Instant;

use wifi_radar_core::{
    now_unix, DeviceObservation, NodeObservation, NodePose, ScanBackend, WifiDevice,
};

/// Configuration for one observation publish.
#[derive(Debug, Clone)]
pub struct NodeConfig {
    /// Stable node id (hostname, label, …).
    pub node_id: String,
    /// Pose in the shared local frame.
    pub pose: NodePose,
}

impl NodeConfig {
    /// Construct a node config.
    pub fn new(node_id: impl Into<String>, pose: NodePose) -> Self {
        Self {
            node_id: node_id.into(),
            pose,
        }
    }
}

/// Build a [`NodeObservation`] from scanned devices.
pub fn observation_from_devices(
    cfg: &NodeConfig,
    devices: &[WifiDevice],
    mono_start: Instant,
) -> NodeObservation {
    let device_observations: Vec<_> = devices
        .iter()
        .map(DeviceObservation::from_wifi_device)
        .collect();
    NodeObservation::new(
        cfg.node_id.clone(),
        now_unix(),
        mono_start.elapsed().as_secs_f64(),
        cfg.pose.clone(),
        device_observations,
    )
}

/// Run one scan via a [`ScanBackend`] and build an observation batch.
pub fn scan_once(
    cfg: &NodeConfig,
    backend: &mut dyn ScanBackend,
    mono_start: Instant,
) -> Result<NodeObservation, String> {
    let devices = backend.scan().map_err(|e| e.to_string())?;
    Ok(observation_from_devices(cfg, &devices, mono_start))
}

/// Compact NDJSON line (+ newline) for TCP publish / stdout.
pub fn observation_to_ndjson(obs: &NodeObservation) -> Result<String, String> {
    let line = serde_json::to_string(obs).map_err(|e| e.to_string())?;
    Ok(line + "\n")
}

/// Demo / offline devices (no radio) for CI and dry runs.
pub fn demo_devices() -> Vec<WifiDevice> {
    let mut a = WifiDevice::hotspot("aa:aa:aa:00:00:01", -42.0);
    a.ssid = "Demo-AP".into();
    a.channel = 6;
    a.freq_mhz = 2437;
    let mut b = WifiDevice::hotspot("aa:aa:aa:00:00:02", -68.0);
    b.ssid = "Far-AP".into();
    b.channel = 11;
    b.freq_mhz = 2462;
    let mut c = WifiDevice::hotspot("aa:aa:aa:00:00:03", -55.0);
    c.ssid = "Mid-AP".into();
    c.channel = 1;
    c.freq_mhz = 2412;
    vec![a, b, c]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_observation_roundtrips_ndjson() {
        let cfg = NodeConfig::new("node-test", NodePose::new(0.0, 0.0, 1.0, 0.0));
        let start = Instant::now();
        let obs = observation_from_devices(&cfg, &demo_devices(), start);
        assert_eq!(obs.node_id, "node-test");
        assert_eq!(obs.device_observations.len(), 3);
        let line = observation_to_ndjson(&obs).unwrap();
        let back = NodeObservation::from_json(line.trim()).unwrap();
        assert_eq!(back.device_observations.len(), 3);
        assert_eq!(back.node_pose.x, 0.0);
    }
}
