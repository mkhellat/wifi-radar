# wifi-radar-fuse

Passive multi-node **fusion aggregator** CLI. Loads `NodeObservation` JSON
files, associates them in a wall-clock window, and prints fused 2D tracks as
NDJSON.

Best-effort RF localization only — **not** survey-grade; **not** a product 3D
claim. A scanning node-agent binary is still separate follow-up work.

```bash
cargo run -p wifi-radar-fuse -- --solver mlat \
  crates/wifi-radar-core/tests/fixtures/three_node_lab_node_a.json \
  crates/wifi-radar-core/tests/fixtures/three_node_lab_node_b.json \
  crates/wifi-radar-core/tests/fixtures/three_node_lab_node_c.json

# Oneshot TCP: accept one client, read NDJSON observations until EOF, fuse.
cargo run -p wifi-radar-fuse -- --listen 127.0.0.1:9800 --solver mlat
```

Solvers: `centroid`, `mlat` (default), `hybrid`, `mlat-robust`.
