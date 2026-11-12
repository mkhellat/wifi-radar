//! Multi-node observation association and 2D fusion helpers.
//!
//! Passive best-effort RF localization only — not survey-grade. Public docs
//! must not claim finished triangulation or 3D product capability.

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

/// Outcome of a 2D fusion attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FuseStatus {
    /// Enough nodes to produce a position.
    Ok,
    /// Fewer than two distinct node poses.
    InsufficientNodes,
}

/// Best-effort 2D fused position.
#[derive(Debug, Clone, PartialEq)]
pub struct FusedPosition2D {
    /// Target MAC.
    pub mac: String,
    /// Estimated X (metres).
    pub x: f64,
    /// Estimated Y (metres).
    pub y: f64,
    /// Scalar confidence in \[0, 1\] (geometry-, count-, and residual-aware).
    pub confidence: f64,
    /// RMS range residual in metres after the solve (0 for pure centroid).
    pub residual_rms_m: f64,
    /// Number of contributing nodes.
    pub contributing_nodes: usize,
    /// Solve status.
    pub status: FuseStatus,
}

fn insufficient(mac: String, n: usize) -> FusedPosition2D {
    FusedPosition2D {
        mac,
        x: f64::NAN,
        y: f64::NAN,
        confidence: 0.0,
        residual_rms_m: f64::NAN,
        contributing_nodes: n,
        status: FuseStatus::InsufficientNodes,
    }
}

fn distinct_sightings(target: &AssociatedTarget) -> Vec<&NodeSighting> {
    let mut used: Vec<&NodeSighting> = Vec::new();
    for s in &target.sightings {
        let dup = used.iter().any(|u| {
            (u.node_pose.x - s.node_pose.x).abs() < 1e-6
                && (u.node_pose.y - s.node_pose.y).abs() < 1e-6
        });
        if !dup {
            used.push(s);
        }
    }
    used
}

fn geometry_confidence(used: &[&NodeSighting]) -> f64 {
    let n = used.len();
    let mut max_sep: f64 = 0.0;
    for i in 0..n {
        for j in (i + 1)..n {
            let dx = used[i].node_pose.x - used[j].node_pose.x;
            let dy = used[i].node_pose.y - used[j].node_pose.y;
            max_sep = max_sep.max((dx * dx + dy * dy).sqrt());
        }
    }
    let count_term = match n {
        2 => 0.45,
        3 => 0.70,
        _ => 0.85,
    };
    let geom_term = (max_sep / 3.0).clamp(0.15, 1.0);
    (count_term * geom_term).clamp(0.0, 1.0)
}

fn residual_rms(used: &[&NodeSighting], x: f64, y: f64) -> f64 {
    if used.is_empty() {
        return f64::NAN;
    }
    let mut acc = 0.0;
    for s in used {
        let r = range_m_from_observation(&s.device);
        let dx = x - s.node_pose.x;
        let dy = y - s.node_pose.y;
        let d = (dx * dx + dy * dy).sqrt();
        let e = d - r;
        acc += e * e;
    }
    (acc / used.len() as f64).sqrt()
}

/// RSSI-weighted centroid in the XY plane.
///
/// Weight for node *i* is `1 / max(range_m, 0.5)^2` so stronger (closer)
/// sightings pull harder. Needs ≥2 distinct node positions; otherwise
/// returns [`FuseStatus::InsufficientNodes`] with NaN coordinates.
pub fn weighted_centroid_2d(target: &AssociatedTarget) -> FusedPosition2D {
    let mac = target.mac.clone();
    if target.sightings.len() < 2 {
        return insufficient(mac, target.sightings.len());
    }
    let used = distinct_sightings(target);
    if used.len() < 2 {
        return insufficient(mac, used.len());
    }

    let mut sum_w = 0.0;
    let mut sum_x = 0.0;
    let mut sum_y = 0.0;
    for s in &used {
        let r = range_m_from_observation(&s.device).max(0.5);
        let w = 1.0 / (r * r);
        sum_w += w;
        sum_x += w * s.node_pose.x;
        sum_y += w * s.node_pose.y;
    }
    let x = sum_x / sum_w;
    let y = sum_y / sum_w;
    let n = used.len();

    FusedPosition2D {
        mac,
        x,
        y,
        confidence: geometry_confidence(&used),
        residual_rms_m: residual_rms(&used, x, y),
        contributing_nodes: n,
        status: FuseStatus::Ok,
    }
}

/// Max Gauss-Newton iterations for RSSI multilateration.
const MLAT_MAX_ITERS: usize = 25;
/// Relative step size stop for multilateration.
const MLAT_TOL: f64 = 1e-6;

/// RSSI-only 2D multilateration (Gauss-Newton), seeded by the weighted centroid.
///
/// Minimizes `Σ (‖p − node_i‖ − r_i)²` with unit weights. With only two nodes
/// the problem is underdetermined off the baseline; the seed keeps the solve
/// on that line. Prefer ≥3 non-collinear nodes for a stable 2D fix.
pub fn multilateration_2d(target: &AssociatedTarget) -> FusedPosition2D {
    let seed = weighted_centroid_2d(target);
    if seed.status != FuseStatus::Ok {
        return seed;
    }
    let used = distinct_sightings(target);
    let mut x = seed.x;
    let mut y = seed.y;

    for _ in 0..MLAT_MAX_ITERS {
        // Normal equations JᵀJ δ = Jᵀ(-e) for 2 unknowns.
        let mut jtj00 = 0.0;
        let mut jtj01 = 0.0;
        let mut jtj11 = 0.0;
        let mut jte0 = 0.0;
        let mut jte1 = 0.0;

        for s in &used {
            let r = range_m_from_observation(&s.device);
            let dx = x - s.node_pose.x;
            let dy = y - s.node_pose.y;
            let d = (dx * dx + dy * dy).sqrt().max(1e-6);
            let e = d - r;
            let jx = dx / d;
            let jy = dy / d;
            jtj00 += jx * jx;
            jtj01 += jx * jy;
            jtj11 += jy * jy;
            jte0 += jx * (-e);
            jte1 += jy * (-e);
        }

        let det = jtj00 * jtj11 - jtj01 * jtj01;
        if det.abs() < 1e-12 {
            break;
        }
        let dx = (jtj11 * jte0 - jtj01 * jte1) / det;
        let dy = (jtj00 * jte1 - jtj01 * jte0) / det;
        x += dx;
        y += dy;
        if (dx * dx + dy * dy).sqrt() < MLAT_TOL {
            break;
        }
    }

    let rms = residual_rms(&used, x, y);
    let resid_term = (1.0 / (1.0 + rms / 2.0)).clamp(0.05, 1.0);
    let confidence = (geometry_confidence(&used) * resid_term).clamp(0.0, 1.0);

    FusedPosition2D {
        mac: target.mac.clone(),
        x,
        y,
        confidence,
        residual_rms_m: rms,
        contributing_nodes: used.len(),
        status: FuseStatus::Ok,
    }
}

/// Associate then run weighted centroid for every target with a successful solve.
pub fn fuse_weighted_centroids(
    batches: &[NodeObservation],
    window_sec: f64,
) -> Vec<FusedPosition2D> {
    associate_observations(batches, window_sec)
        .iter()
        .map(weighted_centroid_2d)
        .filter(|p| p.status == FuseStatus::Ok)
        .collect()
}

/// Associate then run RSSI multilateration for every target with a successful solve.
pub fn fuse_multilateration_2d(
    batches: &[NodeObservation],
    window_sec: f64,
) -> Vec<FusedPosition2D> {
    associate_observations(batches, window_sec)
        .iter()
        .map(multilateration_2d)
        .filter(|p| p.status == FuseStatus::Ok)
        .collect()
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

    #[test]
    fn centroid_needs_two_nodes() {
        let a = obs(
            "a",
            100.0,
            0.0,
            0.0,
            "aa:bb:00:00:00:01",
            DeviceKind::Hotspot,
            -40.0,
        );
        let targets = associate_observations(&[a], DEFAULT_FUSION_WINDOW_SEC);
        let fused = weighted_centroid_2d(&targets[0]);
        assert_eq!(fused.status, FuseStatus::InsufficientNodes);
        assert_eq!(fused.contributing_nodes, 1);
    }

    #[test]
    fn centroid_pulls_toward_stronger_rssi_node() {
        let a = obs(
            "a",
            100.0,
            0.0,
            0.0,
            "aa:bb:00:00:00:01",
            DeviceKind::Hotspot,
            -35.0,
        );
        let b = obs(
            "b",
            100.0,
            4.0,
            0.0,
            "aa:bb:00:00:00:01",
            DeviceKind::Hotspot,
            -70.0,
        );
        let fused = fuse_weighted_centroids(&[a, b], DEFAULT_FUSION_WINDOW_SEC);
        assert_eq!(fused.len(), 1);
        assert_eq!(fused[0].status, FuseStatus::Ok);
        assert!(
            fused[0].x < 2.0,
            "expected pull toward a, got x={}",
            fused[0].x
        );
        assert!(fused[0].x > 0.0);
        assert!(fused[0].confidence > 0.0);
    }

    /// Inverse of [`range_m_from_observation`] for hotspot @ 2437 MHz.
    fn rssi_for_hotspot_range(d_m: f64) -> f64 {
        let n = PATH_LOSS_EXPONENT_INDOOR;
        DeviceKind::Hotspot.rssi_at_1m() - 10.0 * n * d_m.log10()
    }

    #[test]
    fn multilateration_recovers_known_point_with_three_nodes() {
        // Target at (2, 1); nodes at (0,0), (4,0), (2,3) with exact ranges.
        let tx = 2.0_f64;
        let ty = 1.0_f64;
        let nodes = [
            ("a", 0.0, 0.0),
            ("b", 4.0, 0.0),
            ("c", 2.0, 3.0),
        ];
        let batches: Vec<_> = nodes
            .iter()
            .map(|(id, nx, ny)| {
                let d = ((tx - nx).hypot(ty - ny)).max(0.1);
                obs(
                    id,
                    100.0,
                    *nx,
                    *ny,
                    "aa:bb:00:00:00:01",
                    DeviceKind::Hotspot,
                    rssi_for_hotspot_range(d),
                )
            })
            .collect();
        let fused = fuse_multilateration_2d(&batches, DEFAULT_FUSION_WINDOW_SEC);
        assert_eq!(fused.len(), 1);
        assert_eq!(fused[0].status, FuseStatus::Ok);
        assert!(
            (fused[0].x - tx).abs() < 0.05 && (fused[0].y - ty).abs() < 0.05,
            "got ({}, {}), residual={}",
            fused[0].x,
            fused[0].y,
            fused[0].residual_rms_m
        );
        assert!(fused[0].residual_rms_m < 0.05);
        assert_eq!(fused[0].contributing_nodes, 3);
    }

    #[test]
    fn multilateration_needs_two_nodes() {
        let a = obs(
            "a",
            100.0,
            0.0,
            0.0,
            "aa:bb:00:00:00:01",
            DeviceKind::Hotspot,
            -40.0,
        );
        let targets = associate_observations(&[a], DEFAULT_FUSION_WINDOW_SEC);
        let fused = multilateration_2d(&targets[0]);
        assert_eq!(fused.status, FuseStatus::InsufficientNodes);
    }
}
