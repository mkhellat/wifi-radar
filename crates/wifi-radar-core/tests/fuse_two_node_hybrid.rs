//! Two-node hybrid fixtures: bearings lift the fix off the baseline.

use std::fs;
use std::path::PathBuf;

use wifi_radar_core::{
    fuse_hybrid_bearing_range_2d, fuse_multilateration_2d, NodeObservation,
    DEFAULT_FUSION_WINDOW_SEC, FuseStatus,
};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn load_pair() -> [NodeObservation; 2] {
    let a = NodeObservation::from_json(
        &fs::read_to_string(fixture("two_node_hybrid_node_a.json")).unwrap(),
    )
    .unwrap();
    let b = NodeObservation::from_json(
        &fs::read_to_string(fixture("two_node_hybrid_node_b.json")).unwrap(),
    )
    .unwrap();
    [a, b]
}

#[test]
fn two_node_hybrid_lifts_known_ap_off_baseline() {
    // Known AP at (2, 1); nodes on the x-axis at 0 and 4 with exact range+bearing.
    let batches = load_pair();
    let mlat = fuse_multilateration_2d(&batches, DEFAULT_FUSION_WINDOW_SEC);
    assert_eq!(mlat.len(), 1);
    assert!(mlat[0].y.abs() < 0.2, "mlat y={}", mlat[0].y);

    let hybrid = fuse_hybrid_bearing_range_2d(&batches, DEFAULT_FUSION_WINDOW_SEC);
    assert_eq!(hybrid.len(), 1);
    let p = &hybrid[0];
    assert_eq!(p.mac, "aa:aa:aa:00:00:20");
    assert_eq!(p.status, FuseStatus::Ok);
    assert!(
        (p.x - 2.0).abs() < 0.15 && (p.y - 1.0).abs() < 0.15,
        "got ({}, {})",
        p.x,
        p.y
    );
    assert!(p.y > 0.5);
}
