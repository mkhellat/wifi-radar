//! Replayable multi-node observation fixtures (synthetic lab scene).

use std::fs;
use std::path::PathBuf;

use wifi_radar_core::{NodeObservation, NODE_OBS_SCHEMA_VERSION};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

#[test]
fn two_node_lab_fixtures_parse_and_align() {
    let a_text = fs::read_to_string(fixture("two_node_lab_node_a.json")).unwrap();
    let b_text = fs::read_to_string(fixture("two_node_lab_node_b.json")).unwrap();
    let a = NodeObservation::from_json(&a_text).expect("node-a");
    let b = NodeObservation::from_json(&b_text).expect("node-b");

    assert_eq!(a.schema_version, NODE_OBS_SCHEMA_VERSION);
    assert_eq!(b.schema_version, NODE_OBS_SCHEMA_VERSION);
    assert_eq!(a.node_id, "node-a");
    assert_eq!(b.node_id, "node-b");
    assert_eq!(a.device_observations.len(), 2);
    assert_eq!(b.device_observations.len(), 2);

    // Same MACs observed from both nodes (fusion association key).
    let a_macs: Vec<_> = a.device_observations.iter().map(|d| d.mac.as_str()).collect();
    let b_macs: Vec<_> = b.device_observations.iter().map(|d| d.mac.as_str()).collect();
    assert_eq!(a_macs, b_macs);

    // Wall clocks within a ±1s fusion window.
    assert!((a.timestamp_wall - b.timestamp_wall).abs() <= 1.0);

    // Nodes are separated along X for later multilateration fixtures.
    assert!((b.node_pose.x - a.node_pose.x).abs() >= 3.0);
}
