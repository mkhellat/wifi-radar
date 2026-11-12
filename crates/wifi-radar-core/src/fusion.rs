//! Multi-node observation association helpers.
//!
//! Passive best-effort RF localization input only — not survey-grade. Public
//! docs must not claim finished triangulation or 3D product capability.

use std::collections::BTreeMap;

use crate::models::{DeviceKind, PATH_LOSS_EXPONENT_INDOOR};
use crate::node_obs::{DeviceObservation, NodeObservation, NodePose};
use crate::util::normalize_mac;

/// Default fusion wall-clock window (± seconds around the epoch center).
pub const DEFAULT_FUSION_WINDOW_SEC: f64 = 1.0;

/// One device sighting from a single sensing node inside a fusion epoch.
#[derive(Debug, Clone, PartialEq)]
pub struct NodeSighting {
    /// Sensing node id.
    pub node_id: String,
    /// Batch wall-clock time (Unix seconds).
    pub timestamp_wall: f64,
    /// Node pose in the shared frame.
    pub node_pose: NodePose,
    /// Device fields as reported by that node.
    pub device: DeviceObservation,
}

/// All in-window sightings of one MAC for a fusion epoch.
#[derive(Debug, Clone, PartialEq)]
pub struct AssociatedTarget {
    /// Normalized MAC.
    pub mac: String,
    /// Dominant device kind across sightings (hotspot > client > adapter).
    pub kind: DeviceKind,
    /// Contributing node sightings (deduped by `node_id`, strongest RSSI kept).
    pub sightings: Vec<NodeSighting>,
    /// Wall-clock center used for the epoch window.
    pub window_center_wall: f64,
}

/// Associate [`NodeObservation`] batches by MAC within a wall-clock window.
///
/// Epoch center is the median `timestamp_wall` of the input batches. Batches
/// outside ±`window_sec` are dropped. Per MAC, one sighting per `node_id`
/// (strongest RSSI wins). Results are sorted by MAC.
pub fn associate_observations(
    batches: &[NodeObservation],
    window_sec: f64,
) -> Vec<AssociatedTarget> {
    if batches.is_empty() {
        return Vec::new();
    }
    let window_sec = window_sec.max(0.0);
    let mut times: Vec<f64> = batches.iter().map(|b| b.timestamp_wall).collect();
    times.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mid = times.len() / 2;
    let center = if times.len() % 2 == 0 {
        (times[mid - 1] + times[mid]) / 2.0
    } else {
        times[mid]
    };

    // mac -> node_id -> sighting (keep strongest RSSI)
    let mut by_mac: BTreeMap<String, BTreeMap<String, NodeSighting>> = BTreeMap::new();

    for batch in batches {
        if (batch.timestamp_wall - center).abs() > window_sec {
            continue;
        }
        for dev in &batch.device_observations {
            let mac = normalize_mac(&dev.mac);
            let sighting = NodeSighting {
                node_id: batch.node_id.clone(),
                timestamp_wall: batch.timestamp_wall,
                node_pose: batch.node_pose.clone(),
                device: DeviceObservation {
                    mac: mac.clone(),
                    ..dev.clone()
                },
            };
            let nodes = by_mac.entry(mac).or_default();
            match nodes.get(&batch.node_id) {
                Some(prev) if prev.device.rssi_dbm >= sighting.device.rssi_dbm => {}
                _ => {
                    nodes.insert(batch.node_id.clone(), sighting);
                }
            }
        }
    }

    by_mac
        .into_iter()
        .map(|(mac, nodes)| {
            let mut sightings: Vec<_> = nodes.into_values().collect();
            sightings.sort_by(|a, b| a.node_id.cmp(&b.node_id));
            let kind = dominant_kind(&sightings);
            AssociatedTarget {
                mac,
                kind,
                sightings,
                window_center_wall: center,
            }
        })
        .collect()
}

fn dominant_kind(sightings: &[NodeSighting]) -> DeviceKind {
    let mut best = DeviceKind::Adapter;
    let mut best_rank = 0u8;
    for s in sightings {
        let rank = match s.device.kind {
            DeviceKind::Hotspot => 3,
            DeviceKind::Client => 2,
            DeviceKind::Adapter => 1,
        };
        if rank > best_rank {
            best_rank = rank;
            best = s.device.kind;
        }
    }
    best
}

/// Log-distance range estimate (metres) from a device observation.
pub fn range_m_from_observation(dev: &DeviceObservation) -> f64 {
    let ref_dbm = dev.kind.rssi_at_1m();
    let mut n = PATH_LOSS_EXPONENT_INDOOR;
    if dev.freq_mhz > 4000 {
        n += 0.3;
    }
    let exponent = (ref_dbm - dev.rssi_dbm) / (10.0 * n);
    10f64.powf(exponent).clamp(0.1, 120.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obs(
        node_id: &str,
        t: f64,
        x: f64,
        y: f64,
        mac: &str,
        kind: DeviceKind,
        rssi: f64,
    ) -> NodeObservation {
        NodeObservation::new(
            node_id,
            t,
            0.0,
            NodePose::new(x, y, 0.0, 0.0),
            vec![DeviceObservation {
                mac: mac.into(),
                kind,
                rssi_dbm: rssi,
                channel: 6,
                freq_mhz: 2437,
                bearing_deg: None,
                bearing_confidence: 0.0,
                label: String::new(),
                vendor: String::new(),
            }],
        )
    }

    #[test]
    fn associate_groups_by_mac_within_window() {
        let a = obs(
            "a",
            100.0,
            0.0,
            0.0,
            "AA:BB:00:00:00:01",
            DeviceKind::Hotspot,
            -40.0,
        );
        let b = obs(
            "b",
            100.5,
            4.0,
            0.0,
            "aa:bb:00:00:00:01",
            DeviceKind::Hotspot,
            -50.0,
        );
        let targets = associate_observations(&[a, b], DEFAULT_FUSION_WINDOW_SEC);
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].mac, "aa:bb:00:00:00:01");
        assert_eq!(targets[0].sightings.len(), 2);
        assert_eq!(targets[0].kind, DeviceKind::Hotspot);
    }

    #[test]
    fn associate_drops_batches_outside_window() {
        let a = obs(
            "a",
            100.0,
            0.0,
            0.0,
            "aa:bb:00:00:00:01",
            DeviceKind::Client,
            -40.0,
        );
        let b = obs(
            "b",
            110.0,
            4.0,
            0.0,
            "aa:bb:00:00:00:01",
            DeviceKind::Client,
            -50.0,
        );
        let targets = associate_observations(&[a, b], DEFAULT_FUSION_WINDOW_SEC);
        // Median of [100, 110] = 105; both |Δ|=5 > 1 → dropped.
        assert!(targets.is_empty());
    }

    #[test]
    fn associate_keeps_strongest_per_node() {
        let a1 = obs(
            "a",
            100.0,
            0.0,
            0.0,
            "aa:bb:00:00:00:01",
            DeviceKind::Client,
            -60.0,
        );
        let a2 = obs(
            "a",
            100.2,
            0.0,
            0.0,
            "aa:bb:00:00:00:01",
            DeviceKind::Client,
            -40.0,
        );
        let targets = associate_observations(&[a1, a2], DEFAULT_FUSION_WINDOW_SEC);
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].sightings.len(), 1);
        assert!((targets[0].sightings[0].device.rssi_dbm - (-40.0)).abs() < 1e-9);
    }

    #[test]
    fn range_helper_stronger_is_closer() {
        let strong = DeviceObservation {
            mac: "aa:bb:00:00:00:01".into(),
            kind: DeviceKind::Hotspot,
            rssi_dbm: -35.0,
            channel: 6,
            freq_mhz: 2437,
            bearing_deg: None,
            bearing_confidence: 0.0,
            label: String::new(),
            vendor: String::new(),
        };
        let weak = DeviceObservation {
            rssi_dbm: -70.0,
            ..strong.clone()
        };
        assert!(range_m_from_observation(&strong) < range_m_from_observation(&weak));
    }
}
