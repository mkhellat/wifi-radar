//! Fusion over the two-node lab fixtures (associate + weighted centroid).

use std::fs;
use std::path::PathBuf;

use wifi_radar_core::{
    fuse_weighted_centroids, NodeObservation, DEFAULT_FUSION_WINDOW_SEC, FuseStatus,
};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

#[test]
fn two_node_lab_fused_centroids() {
    let a = NodeObservation::from_json(
        &fs::read_to_string(fixture("two_node_lab_node_a.json")).unwrap(),
    )
    .unwrap();
    let b = NodeObservation::from_json(
        &fs::read_to_string(fixture("two_node_lab_node_b.json")).unwrap(),
    )
    .unwrap();

    let fused = fuse_weighted_centroids(&[a, b], DEFAULT_FUSION_WINDOW_SEC);
    assert_eq!(fused.len(), 2, "Lab-AP and phone should both fuse");

    for p in &fused {
        assert_eq!(p.status, FuseStatus::Ok);
        assert_eq!(p.contributing_nodes, 2);
        assert!(p.confidence > 0.0);
        // Nodes sit on x=0 and x=4; centroid must lie strictly between them.
        assert!(p.x > 0.0 && p.x < 4.0, "mac={} x={}", p.mac, p.x);
        assert!(p.y.abs() < 1e-9);
    }

    let ap = fused
        .iter()
        .find(|p| p.mac == "aa:aa:aa:00:00:01")
        .expect("Lab-AP");
    let phone = fused
        .iter()
        .find(|p| p.mac == "aa:aa:aa:00:00:02")
        .expect("phone");
    // AP is stronger at node-a (−45 vs −52); phone stronger at node-b (−48 vs −60).
    assert!(
        ap.x < phone.x,
        "AP should sit closer to node-a than phone does (ap={} phone={})",
        ap.x,
        phone.x
    );
}
