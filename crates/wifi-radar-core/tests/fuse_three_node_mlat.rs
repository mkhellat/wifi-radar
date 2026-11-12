//! Three-node lab fixtures: RSSI multilateration recovers a known point.

use std::fs;
use std::path::PathBuf;

use wifi_radar_core::{
    fuse_multilateration_2d, NodeObservation, DEFAULT_FUSION_WINDOW_SEC, FuseStatus,
};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

#[test]
fn three_node_lab_multilateration_recovers_known_ap() {
    // Synthetic scene: AP at (2, 1); nodes at (0,0), (4,0), (2,3) with exact ranges.
    let batches: Vec<_> = ["a", "b", "c"]
        .iter()
        .map(|id| {
            let name = format!("three_node_lab_node_{id}.json");
            NodeObservation::from_json(&fs::read_to_string(fixture(&name)).unwrap()).unwrap()
        })
        .collect();

    let fused = fuse_multilateration_2d(&batches, DEFAULT_FUSION_WINDOW_SEC);
    assert_eq!(fused.len(), 1);
    let p = &fused[0];
    assert_eq!(p.mac, "aa:aa:aa:00:00:10");
    assert_eq!(p.status, FuseStatus::Ok);
    assert_eq!(p.contributing_nodes, 3);
    assert!(
        (p.x - 2.0).abs() < 0.05 && (p.y - 1.0).abs() < 0.05,
        "got ({}, {}), residual={}",
        p.x,
        p.y,
        p.residual_rms_m
    );
    assert!(p.residual_rms_m < 0.05);
}
