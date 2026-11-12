//! Four-node outlier scene: Tukey-robust mlat beats L2.

use std::fs;
use std::path::PathBuf;

use wifi_radar_core::{
    fuse_multilateration_2d, fuse_multilateration_2d_robust, NodeObservation,
    DEFAULT_FUSION_WINDOW_SEC, DEFAULT_TUKEY_DELTA_M,
};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

#[test]
fn four_node_outlier_robust_beats_l2() {
    // Known AP at (2,1); nodes a/b/c exact, node-bad at (12,12) claims 0.5 m range.
    let ids = ["a", "b", "c", "bad"];
    let batches: Vec<_> = ids
        .iter()
        .map(|id| {
            let name = format!("four_node_outlier_node_{id}.json");
            NodeObservation::from_json(&fs::read_to_string(fixture(&name)).unwrap()).unwrap()
        })
        .collect();

    let l2 = fuse_multilateration_2d(&batches, DEFAULT_FUSION_WINDOW_SEC);
    let robust =
        fuse_multilateration_2d_robust(&batches, DEFAULT_FUSION_WINDOW_SEC, DEFAULT_TUKEY_DELTA_M);
    assert_eq!(l2.len(), 1);
    assert_eq!(robust.len(), 1);
    assert_eq!(robust[0].mac, "aa:aa:aa:00:00:40");

    let tx = 2.0;
    let ty = 1.0;
    let err_l2 = (l2[0].x - tx).hypot(l2[0].y - ty);
    let err_r = (robust[0].x - tx).hypot(robust[0].y - ty);
    assert!(err_r + 1e-9 < err_l2, "robust={err_r} l2={err_l2}");
    assert!(err_r < 0.15, "robust err={err_r}");
}
