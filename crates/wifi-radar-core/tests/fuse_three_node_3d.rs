//! 3D go/no-go fixtures: vertical span required for Ok.

use std::fs;
use std::path::PathBuf;

use wifi_radar_core::{
    associate_observations, fuse_multilateration_3d, multilateration_3d, NodeObservation,
    DEFAULT_FUSION_WINDOW_SEC, FuseStatus, MIN_VERTICAL_SEP_M,
};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn load_triplet(prefix: &str) -> Vec<NodeObservation> {
    ["a", "b", "c"]
        .iter()
        .map(|id| {
            let name = format!("{prefix}_node_{id}.json");
            NodeObservation::from_json(&fs::read_to_string(fixture(&name)).unwrap()).unwrap()
        })
        .collect()
}

#[test]
fn three_node_3d_go_recovers_known_ap() {
    // Known AP at (2, 1, 1.5); nodes at z=0.5/0.5/2.5.
    let batches = load_triplet("three_node_3d_go");
    let fused = fuse_multilateration_3d(&batches, DEFAULT_FUSION_WINDOW_SEC);
    assert_eq!(fused.len(), 1);
    let p = &fused[0];
    assert_eq!(p.mac, "aa:aa:aa:00:00:30");
    assert_eq!(p.status, FuseStatus::Ok);
    assert!(
        (p.x - 2.0).abs() < 0.1 && (p.y - 1.0).abs() < 0.1 && (p.z - 1.5).abs() < 0.15,
        "got ({}, {}, {})",
        p.x,
        p.y,
        p.z
    );
    assert!(p.vertical_span_m >= MIN_VERTICAL_SEP_M);
}

#[test]
fn three_node_3d_flat_is_no_go() {
    let batches = load_triplet("three_node_3d_flat");
    let targets = associate_observations(&batches, DEFAULT_FUSION_WINDOW_SEC);
    assert_eq!(targets.len(), 1);
    let p = multilateration_3d(&targets[0]);
    assert_eq!(p.status, FuseStatus::InsufficientGeometry);
    assert!(p.vertical_span_m < MIN_VERTICAL_SEP_M);
    assert!(fuse_multilateration_3d(&batches, DEFAULT_FUSION_WINDOW_SEC).is_empty());
}
