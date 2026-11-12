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

/// Outcome of a fusion attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FuseStatus {
    /// Enough nodes and geometry to produce a position.
    Ok,
    /// Fewer than the solver's minimum distinct node poses.
    InsufficientNodes,
    /// Enough nodes, but geometry/residuals fail the go/no-go gate (3D).
    InsufficientGeometry,
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
                && (u.node_pose.z - s.node_pose.z).abs() < 1e-6
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

/// Default Tukey biweight cutoff (metres) for robust range residuals.
pub const DEFAULT_TUKEY_DELTA_M: f64 = 3.0;

/// IRLS weight for a residual under Huber loss with threshold `delta`.
///
/// Returns `1` when `|e| ≤ delta`, otherwise `delta / |e|`.
pub fn huber_weight(residual: f64, delta: f64) -> f64 {
    let delta = delta.max(1e-9);
    let ae = residual.abs();
    if ae <= delta {
        1.0
    } else {
        delta / ae
    }
}

/// IRLS weight for Tukey biweight loss (hard reject beyond `delta`).
///
/// Returns `(1 - (e/delta)²)²` when `|e| < delta`, otherwise `0`.
pub fn tukey_weight(residual: f64, delta: f64) -> f64 {
    let delta = delta.max(1e-9);
    let ae = residual.abs();
    if ae >= delta {
        return 0.0;
    }
    let u = ae / delta;
    let t = 1.0 - u * u;
    t * t
}

fn multilateration_2d_irls(target: &AssociatedTarget, tukey_delta: Option<f64>) -> FusedPosition2D {
    let seed = weighted_centroid_2d(target);
    if seed.status != FuseStatus::Ok {
        return seed;
    }
    let used = distinct_sightings(target);
    // Robust IRLS: avoid RSSI-weighted centroid seed (a false close range can
    // dominate). Start from the coordinate-wise median of node poses.
    let (mut x, mut y) = if tukey_delta.is_some() {
        let mut xs: Vec<f64> = used.iter().map(|s| s.node_pose.x).collect();
        let mut ys: Vec<f64> = used.iter().map(|s| s.node_pose.y).collect();
        xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        ys.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let mid = used.len() / 2;
        let mx = if used.len() % 2 == 0 {
            (xs[mid - 1] + xs[mid]) / 2.0
        } else {
            xs[mid]
        };
        let my = if used.len() % 2 == 0 {
            (ys[mid - 1] + ys[mid]) / 2.0
        } else {
            ys[mid]
        };
        (mx, my)
    } else {
        (seed.x, seed.y)
    };

    for _ in 0..MLAT_MAX_ITERS {
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
            let w = match tukey_delta {
                Some(delta) => tukey_weight(e, delta),
                None => 1.0,
            };
            let jx = dx / d;
            let jy = dy / d;
            jtj00 += w * jx * jx;
            jtj01 += w * jx * jy;
            jtj11 += w * jy * jy;
            jte0 += w * jx * (-e);
            jte1 += w * jy * (-e);
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

/// RSSI-only 2D multilateration (Gauss-Newton), seeded by the weighted centroid.
///
/// Minimizes `Σ (‖p − node_i‖ − r_i)²` with unit weights. With only two nodes
/// the problem is underdetermined off the baseline; the seed keeps the solve
/// on that line. Prefer ≥3 non-collinear nodes for a stable 2D fix.
pub fn multilateration_2d(target: &AssociatedTarget) -> FusedPosition2D {
    multilateration_2d_irls(target, None)
}

/// Like [`multilateration_2d`] but with Tukey biweight IRLS on range residuals.
pub fn multilateration_2d_robust(
    target: &AssociatedTarget,
    tukey_delta_m: f64,
) -> FusedPosition2D {
    multilateration_2d_irls(target, Some(tukey_delta_m.max(1e-9)))
}

/// Minimum `bearing_confidence` required to include a bearing residual.
pub const MIN_BEARING_CONFIDENCE: f64 = 0.05;

fn wrap_pi(a: f64) -> f64 {
    let two_pi = 2.0 * std::f64::consts::PI;
    let mut x = (a + std::f64::consts::PI) % two_pi;
    if x < 0.0 {
        x += two_pi;
    }
    x - std::f64::consts::PI
}

/// Absolute bearing in the shared frame: degrees, 0 along +X, increasing CCW
/// ([`f64::atan2`] convention).
pub fn bearing_deg_from_delta(dx: f64, dy: f64) -> f64 {
    dy.atan2(dx).to_degrees().rem_euclid(360.0)
}

fn bearing_residual_m(s: &NodeSighting, x: f64, y: f64) -> Option<(f64, f64, f64)> {
    let conf = s.device.bearing_confidence;
    let Some(bearing_deg) = s.device.bearing_deg else {
        return None;
    };
    if conf < MIN_BEARING_CONFIDENCE {
        return None;
    }
    let dx = x - s.node_pose.x;
    let dy = y - s.node_pose.y;
    let d2 = dx * dx + dy * dy;
    if d2 < 1e-12 {
        return None;
    }
    let d = d2.sqrt();
    let pred = dy.atan2(dx);
    let obs = bearing_deg.to_radians();
    let ang = wrap_pi(pred - obs);
    // Arc-length residual (metres), scaled by confidence.
    let e = ang * d * conf;
    // ∂e/∂x, ∂e/∂y via ∂atan2/∂(dx,dy) * d * conf (+ ang * ∂d term neglected for small steps).
    let jx = (-dy / d2) * d * conf;
    let jy = (dx / d2) * d * conf;
    Some((e, jx, jy))
}

fn hybrid_bearing_range_2d_irls(
    target: &AssociatedTarget,
    tukey_delta: Option<f64>,
) -> FusedPosition2D {
    let seed = match tukey_delta {
        Some(d) => multilateration_2d_robust(target, d),
        None => multilateration_2d(target),
    };
    if seed.status != FuseStatus::Ok {
        return seed;
    }
    let used = distinct_sightings(target);
    let mut x = seed.x;
    let mut y = seed.y;
    let mut bearing_used = 0usize;

    for _ in 0..MLAT_MAX_ITERS {
        let mut jtj00 = 0.0;
        let mut jtj01 = 0.0;
        let mut jtj11 = 0.0;
        let mut jte0 = 0.0;
        let mut jte1 = 0.0;
        bearing_used = 0;

        for s in &used {
            let r = range_m_from_observation(&s.device);
            let dx = x - s.node_pose.x;
            let dy = y - s.node_pose.y;
            let d = (dx * dx + dy * dy).sqrt().max(1e-6);
            let e = d - r;
            let w = match tukey_delta {
                Some(delta) => tukey_weight(e, delta),
                None => 1.0,
            };
            let jx = dx / d;
            let jy = dy / d;
            jtj00 += w * jx * jx;
            jtj01 += w * jx * jy;
            jtj11 += w * jy * jy;
            jte0 += w * jx * (-e);
            jte1 += w * jy * (-e);

            if let Some((eb, bjx, bjy)) = bearing_residual_m(s, x, y) {
                bearing_used += 1;
                let wb = match tukey_delta {
                    Some(delta) => tukey_weight(eb, delta),
                    None => 1.0,
                };
                jtj00 += wb * bjx * bjx;
                jtj01 += wb * bjx * bjy;
                jtj11 += wb * bjy * bjy;
                jte0 += wb * bjx * (-eb);
                jte1 += wb * bjy * (-eb);
            }
        }

        let det = jtj00 * jtj11 - jtj01 * jtj01;
        if det.abs() < 1e-12 {
            break;
        }
        let step_x = (jtj11 * jte0 - jtj01 * jte1) / det;
        let step_y = (jtj00 * jte1 - jtj01 * jte0) / det;
        x += step_x;
        y += step_y;
        if (step_x * step_x + step_y * step_y).sqrt() < MLAT_TOL {
            break;
        }
    }

    let rms = residual_rms(&used, x, y);
    let resid_term = (1.0 / (1.0 + rms / 2.0)).clamp(0.05, 1.0);
    let bearing_term = if bearing_used > 0 { 1.15 } else { 1.0 };
    let confidence =
        (geometry_confidence(&used) * resid_term * bearing_term).clamp(0.0, 1.0);

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

/// Hybrid bearing + RSSI 2D solve (Gauss-Newton).
///
/// Minimizes range residuals plus arc-length bearing residuals when
/// `bearing_deg` is set and `bearing_confidence` ≥ [`MIN_BEARING_CONFIDENCE`].
/// Seeded by [`multilateration_2d`]. Bearings are absolute in the shared frame
/// (0° = +X, CCW). Best-effort 2D only — not survey-grade.
pub fn hybrid_bearing_range_2d(target: &AssociatedTarget) -> FusedPosition2D {
    hybrid_bearing_range_2d_irls(target, None)
}

/// Like [`hybrid_bearing_range_2d`] with Tukey IRLS on range and bearing residuals.
pub fn hybrid_bearing_range_2d_robust(
    target: &AssociatedTarget,
    tukey_delta_m: f64,
) -> FusedPosition2D {
    hybrid_bearing_range_2d_irls(target, Some(tukey_delta_m.max(1e-9)))
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

/// Associate then run hybrid bearing+RSSI fusion for every successful solve.
pub fn fuse_hybrid_bearing_range_2d(
    batches: &[NodeObservation],
    window_sec: f64,
) -> Vec<FusedPosition2D> {
    associate_observations(batches, window_sec)
        .iter()
        .map(hybrid_bearing_range_2d)
        .filter(|p| p.status == FuseStatus::Ok)
        .collect()
}

/// Associate then run Tukey-robust 2D multilateration for every successful solve.
pub fn fuse_multilateration_2d_robust(
    batches: &[NodeObservation],
    window_sec: f64,
    tukey_delta_m: f64,
) -> Vec<FusedPosition2D> {
    associate_observations(batches, window_sec)
        .iter()
        .map(|t| multilateration_2d_robust(t, tukey_delta_m))
        .filter(|p| p.status == FuseStatus::Ok)
        .collect()
}

/// Minimum distinct sensing nodes for a 3D attempt.
pub const MIN_NODES_3D: usize = 3;
/// Minimum vertical span among node poses (metres) for 3D go.
pub const MIN_VERTICAL_SEP_M: f64 = 0.5;
/// Maximum post-solve RMS range residual (metres) for 3D go.
pub const MAX_RESIDUAL_RMS_3D_M: f64 = 1.0;

/// Best-effort 3D fused position (gated).
#[derive(Debug, Clone, PartialEq)]
pub struct FusedPosition3D {
    /// Target MAC.
    pub mac: String,
    /// Estimated X (metres).
    pub x: f64,
    /// Estimated Y (metres).
    pub y: f64,
    /// Estimated Z (metres).
    pub z: f64,
    /// Scalar confidence in \[0, 1\].
    pub confidence: f64,
    /// RMS range residual in metres after the solve.
    pub residual_rms_m: f64,
    /// Number of contributing nodes.
    pub contributing_nodes: usize,
    /// Vertical span of contributing node poses (metres).
    pub vertical_span_m: f64,
    /// Solve / go-no-go status.
    pub status: FuseStatus,
}

fn insufficient_3d(mac: String, n: usize, status: FuseStatus, vertical_span_m: f64) -> FusedPosition3D {
    FusedPosition3D {
        mac,
        x: f64::NAN,
        y: f64::NAN,
        z: f64::NAN,
        confidence: 0.0,
        residual_rms_m: f64::NAN,
        contributing_nodes: n,
        vertical_span_m,
        status,
    }
}

fn vertical_span_m(used: &[&NodeSighting]) -> f64 {
    let mut zmin = f64::INFINITY;
    let mut zmax = f64::NEG_INFINITY;
    for s in used {
        zmin = zmin.min(s.node_pose.z);
        zmax = zmax.max(s.node_pose.z);
    }
    if !zmin.is_finite() {
        return 0.0;
    }
    zmax - zmin
}

fn residual_rms_3d(used: &[&NodeSighting], x: f64, y: f64, z: f64) -> f64 {
    if used.is_empty() {
        return f64::NAN;
    }
    let mut acc = 0.0;
    for s in used {
        let r = range_m_from_observation(&s.device);
        let dx = x - s.node_pose.x;
        let dy = y - s.node_pose.y;
        let dz = z - s.node_pose.z;
        let d = (dx * dx + dy * dy + dz * dz).sqrt();
        let e = d - r;
        acc += e * e;
    }
    (acc / used.len() as f64).sqrt()
}

fn solve_3x3(m: [[f64; 3]; 3], b: [f64; 3]) -> Option<[f64; 3]> {
    let det = m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
        - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
    if det.abs() < 1e-12 {
        return None;
    }
    let inv_det = 1.0 / det;
    let mut adj = [[0.0; 3]; 3];
    adj[0][0] = (m[1][1] * m[2][2] - m[1][2] * m[2][1]) * inv_det;
    adj[0][1] = (m[0][2] * m[2][1] - m[0][1] * m[2][2]) * inv_det;
    adj[0][2] = (m[0][1] * m[1][2] - m[0][2] * m[1][1]) * inv_det;
    adj[1][0] = (m[1][2] * m[2][0] - m[1][0] * m[2][2]) * inv_det;
    adj[1][1] = (m[0][0] * m[2][2] - m[0][2] * m[2][0]) * inv_det;
    adj[1][2] = (m[0][2] * m[1][0] - m[0][0] * m[1][2]) * inv_det;
    adj[2][0] = (m[1][0] * m[2][1] - m[1][1] * m[2][0]) * inv_det;
    adj[2][1] = (m[0][1] * m[2][0] - m[0][0] * m[2][1]) * inv_det;
    adj[2][2] = (m[0][0] * m[1][1] - m[0][1] * m[1][0]) * inv_det;
    Some([
        adj[0][0] * b[0] + adj[0][1] * b[1] + adj[0][2] * b[2],
        adj[1][0] * b[0] + adj[1][1] * b[1] + adj[1][2] * b[2],
        adj[2][0] * b[0] + adj[2][1] * b[1] + adj[2][2] * b[2],
    ])
}

fn multilateration_3d_irls(
    target: &AssociatedTarget,
    tukey_delta: Option<f64>,
) -> FusedPosition3D {
    let mac = target.mac.clone();
    let used = distinct_sightings(target);
    let span = vertical_span_m(&used);
    if used.len() < MIN_NODES_3D {
        return insufficient_3d(mac, used.len(), FuseStatus::InsufficientNodes, span);
    }
    if span < MIN_VERTICAL_SEP_M {
        return insufficient_3d(mac, used.len(), FuseStatus::InsufficientGeometry, span);
    }

    let seed2 = match tukey_delta {
        Some(d) => multilateration_2d_robust(target, d),
        None => multilateration_2d(target),
    };
    let mut x = if seed2.status == FuseStatus::Ok {
        seed2.x
    } else {
        used.iter().map(|s| s.node_pose.x).sum::<f64>() / used.len() as f64
    };
    let mut y = if seed2.status == FuseStatus::Ok {
        seed2.y
    } else {
        used.iter().map(|s| s.node_pose.y).sum::<f64>() / used.len() as f64
    };
    let mut z = used.iter().map(|s| s.node_pose.z).sum::<f64>() / used.len() as f64;

    for _ in 0..MLAT_MAX_ITERS {
        let mut jtj = [[0.0_f64; 3]; 3];
        let mut jte = [0.0_f64; 3];
        for s in &used {
            let r = range_m_from_observation(&s.device);
            let dx = x - s.node_pose.x;
            let dy = y - s.node_pose.y;
            let dz = z - s.node_pose.z;
            let d = (dx * dx + dy * dy + dz * dz).sqrt().max(1e-6);
            let e = d - r;
            let w = match tukey_delta {
                Some(delta) => tukey_weight(e, delta),
                None => 1.0,
            };
            let j = [dx / d, dy / d, dz / d];
            for row in 0..3 {
                for col in 0..3 {
                    jtj[row][col] += w * j[row] * j[col];
                }
                jte[row] += w * j[row] * (-e);
            }
        }
        let Some(step) = solve_3x3(jtj, jte) else {
            break;
        };
        x += step[0];
        y += step[1];
        z += step[2];
        if (step[0] * step[0] + step[1] * step[1] + step[2] * step[2]).sqrt() < MLAT_TOL {
            break;
        }
    }

    let rms = residual_rms_3d(&used, x, y, z);
    if !rms.is_finite() || rms > MAX_RESIDUAL_RMS_3D_M {
        return insufficient_3d(mac, used.len(), FuseStatus::InsufficientGeometry, span);
    }

    let resid_term = (1.0 / (1.0 + rms / 2.0)).clamp(0.05, 1.0);
    let vert_term = (span / MIN_VERTICAL_SEP_M).clamp(0.5, 1.25).min(1.0);
    let confidence = (geometry_confidence(&used) * resid_term * vert_term).clamp(0.0, 1.0);

    FusedPosition3D {
        mac,
        x,
        y,
        z,
        confidence,
        residual_rms_m: rms,
        contributing_nodes: used.len(),
        vertical_span_m: span,
        status: FuseStatus::Ok,
    }
}

/// RSSI-only 3D multilateration with an explicit go/no-go gate.
///
/// **Go** requires ≥[`MIN_NODES_3D`] distinct poses, vertical span
/// ≥[`MIN_VERTICAL_SEP_M`], and post-solve RMS residual
/// ≤[`MAX_RESIDUAL_RMS_3D_M`]. Otherwise returns
/// [`FuseStatus::InsufficientNodes`] or [`FuseStatus::InsufficientGeometry`].
///
/// Research / lab helper only — not a survey-grade or product 3D claim.
pub fn multilateration_3d(target: &AssociatedTarget) -> FusedPosition3D {
    multilateration_3d_irls(target, None)
}

/// Like [`multilateration_3d`] with Tukey biweight IRLS on range residuals.
pub fn multilateration_3d_robust(
    target: &AssociatedTarget,
    tukey_delta_m: f64,
) -> FusedPosition3D {
    multilateration_3d_irls(target, Some(tukey_delta_m.max(1e-9)))
}

/// Associate then run gated 3D multilateration for every successful solve.
pub fn fuse_multilateration_3d(
    batches: &[NodeObservation],
    window_sec: f64,
) -> Vec<FusedPosition3D> {
    associate_observations(batches, window_sec)
        .iter()
        .map(multilateration_3d)
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

    fn obs_with_bearing(
        node_id: &str,
        t: f64,
        nx: f64,
        ny: f64,
        mac: &str,
        rssi: f64,
        bearing_deg: f64,
        bearing_confidence: f64,
    ) -> NodeObservation {
        let mut o = obs(node_id, t, nx, ny, mac, DeviceKind::Hotspot, rssi);
        o.device_observations[0].bearing_deg = Some(bearing_deg);
        o.device_observations[0].bearing_confidence = bearing_confidence;
        o
    }

    #[test]
    fn hybrid_lifts_two_node_solve_off_baseline_with_bearings() {
        // Target (2, 1); two nodes on x-axis. Pure mlat stays on y≈0; bearings lift it.
        let tx = 2.0_f64;
        let ty = 1.0_f64;
        let nodes = [("a", 0.0, 0.0), ("b", 4.0, 0.0)];
        let batches: Vec<_> = nodes
            .iter()
            .map(|(id, nx, ny)| {
                let d = (tx - nx).hypot(ty - ny);
                obs_with_bearing(
                    id,
                    100.0,
                    *nx,
                    *ny,
                    "aa:bb:00:00:00:01",
                    rssi_for_hotspot_range(d),
                    bearing_deg_from_delta(tx - nx, ty - ny),
                    0.9,
                )
            })
            .collect();

        let mlat = fuse_multilateration_2d(&batches, DEFAULT_FUSION_WINDOW_SEC);
        assert_eq!(mlat.len(), 1);
        assert!(
            mlat[0].y.abs() < 0.2,
            "mlat should stay near baseline, y={}",
            mlat[0].y
        );

        let hybrid = fuse_hybrid_bearing_range_2d(&batches, DEFAULT_FUSION_WINDOW_SEC);
        assert_eq!(hybrid.len(), 1);
        assert_eq!(hybrid[0].status, FuseStatus::Ok);
        assert!(
            (hybrid[0].x - tx).abs() < 0.15 && (hybrid[0].y - ty).abs() < 0.15,
            "got ({}, {})",
            hybrid[0].x,
            hybrid[0].y
        );
        assert!(hybrid[0].y > 0.5, "bearings should pull off baseline");
    }

    #[test]
    fn hybrid_without_bearings_matches_multilateration() {
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
                let d = (tx - nx).hypot(ty - ny);
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
        let mlat = fuse_multilateration_2d(&batches, DEFAULT_FUSION_WINDOW_SEC);
        let hybrid = fuse_hybrid_bearing_range_2d(&batches, DEFAULT_FUSION_WINDOW_SEC);
        assert_eq!(mlat.len(), 1);
        assert_eq!(hybrid.len(), 1);
        assert!((mlat[0].x - hybrid[0].x).abs() < 1e-6);
        assert!((mlat[0].y - hybrid[0].y).abs() < 1e-6);
    }

    fn obs_xyz(
        node_id: &str,
        t: f64,
        x: f64,
        y: f64,
        z: f64,
        mac: &str,
        rssi: f64,
    ) -> NodeObservation {
        let mut o = obs(node_id, t, x, y, mac, DeviceKind::Hotspot, rssi);
        o.node_pose.z = z;
        o
    }

    fn dist3(ax: f64, ay: f64, az: f64, bx: f64, by: f64, bz: f64) -> f64 {
        ((ax - bx).hypot(ay - by)).hypot(az - bz)
    }

    #[test]
    fn multilateration_3d_recovers_known_point_with_vertical_span() {
        let (tx, ty, tz) = (2.0_f64, 1.0_f64, 1.5_f64);
        let nodes = [
            ("a", 0.0, 0.0, 0.5),
            ("b", 4.0, 0.0, 0.5),
            ("c", 2.0, 3.0, 2.5),
        ];
        let batches: Vec<_> = nodes
            .iter()
            .map(|(id, nx, ny, nz)| {
                let d = dist3(tx, ty, tz, *nx, *ny, *nz);
                obs_xyz(id, 100.0, *nx, *ny, *nz, "aa:bb:00:00:00:01", rssi_for_hotspot_range(d))
            })
            .collect();
        let fused = fuse_multilateration_3d(&batches, DEFAULT_FUSION_WINDOW_SEC);
        assert_eq!(fused.len(), 1);
        let p = &fused[0];
        assert_eq!(p.status, FuseStatus::Ok);
        assert!(
            (p.x - tx).abs() < 0.1 && (p.y - ty).abs() < 0.1 && (p.z - tz).abs() < 0.15,
            "got ({}, {}, {}), residual={}",
            p.x,
            p.y,
            p.z,
            p.residual_rms_m
        );
        assert!(p.vertical_span_m >= MIN_VERTICAL_SEP_M);
        assert!(p.residual_rms_m <= MAX_RESIDUAL_RMS_3D_M);
    }

    #[test]
    fn multilateration_3d_no_go_without_vertical_span() {
        let (tx, ty, tz) = (2.0_f64, 1.0_f64, 1.0_f64);
        let nodes = [
            ("a", 0.0, 0.0, 1.0),
            ("b", 4.0, 0.0, 1.0),
            ("c", 2.0, 3.0, 1.0),
        ];
        let batches: Vec<_> = nodes
            .iter()
            .map(|(id, nx, ny, nz)| {
                let d = dist3(tx, ty, tz, *nx, *ny, *nz);
                obs_xyz(id, 100.0, *nx, *ny, *nz, "aa:bb:00:00:00:01", rssi_for_hotspot_range(d))
            })
            .collect();
        let targets = associate_observations(&batches, DEFAULT_FUSION_WINDOW_SEC);
        let p = multilateration_3d(&targets[0]);
        assert_eq!(p.status, FuseStatus::InsufficientGeometry);
        assert!(p.vertical_span_m < MIN_VERTICAL_SEP_M);
    }

    #[test]
    fn multilateration_3d_needs_three_nodes() {
        let a = obs_xyz("a", 100.0, 0.0, 0.0, 0.5, "aa:bb:00:00:00:01", -40.0);
        let b = obs_xyz("b", 100.0, 4.0, 0.0, 2.0, "aa:bb:00:00:00:01", -45.0);
        let targets = associate_observations(&[a, b], DEFAULT_FUSION_WINDOW_SEC);
        let p = multilateration_3d(&targets[0]);
        assert_eq!(p.status, FuseStatus::InsufficientNodes);
    }

    #[test]
    fn huber_and_tukey_weights_behave() {
        assert!((huber_weight(0.5, 2.0) - 1.0).abs() < 1e-12);
        assert!((huber_weight(4.0, 2.0) - 0.5).abs() < 1e-12);
        assert!((tukey_weight(0.0, 3.0) - 1.0).abs() < 1e-12);
        assert!(tukey_weight(3.0, 3.0).abs() < 1e-12);
        assert!(tukey_weight(4.0, 3.0).abs() < 1e-12);
    }

    #[test]
    fn robust_mlat_resists_one_outlier_better_than_l2() {
        // Three clean nodes for target (2,1); fourth far node claims the target is 0.5 m away.
        let tx = 2.0_f64;
        let ty = 1.0_f64;
        let clean = [
            ("a", 0.0, 0.0),
            ("b", 4.0, 0.0),
            ("c", 2.0, 3.0),
        ];
        let mut batches: Vec<_> = clean
            .iter()
            .map(|(id, nx, ny)| {
                let d = (tx - nx).hypot(ty - ny);
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
        batches.push(obs(
            "outlier",
            100.0,
            12.0,
            12.0,
            "aa:bb:00:00:00:01",
            DeviceKind::Hotspot,
            rssi_for_hotspot_range(0.5),
        ));

        let l2 = fuse_multilateration_2d(&batches, DEFAULT_FUSION_WINDOW_SEC);
        let robust = fuse_multilateration_2d_robust(
            &batches,
            DEFAULT_FUSION_WINDOW_SEC,
            DEFAULT_TUKEY_DELTA_M,
        );
        assert_eq!(l2.len(), 1);
        assert_eq!(robust.len(), 1);
        let err_l2 = (l2[0].x - tx).hypot(l2[0].y - ty);
        let err_r = (robust[0].x - tx).hypot(robust[0].y - ty);
        assert!(
            err_r + 1e-9 < err_l2,
            "robust err={err_r} should beat l2 err={err_l2} (l2=({}, {}) robust=({}, {}))",
            l2[0].x,
            l2[0].y,
            robust[0].x,
            robust[0].y
        );
        assert!(err_r < 0.15, "robust should stay near truth, err={err_r}");
    }
}
