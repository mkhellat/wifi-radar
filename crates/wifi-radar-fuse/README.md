# wifi-radar-fuse

Passive multi-node **fusion aggregator** CLI. Loads `NodeObservation` JSON
files, associates them in a wall-clock window, and prints fused 2D tracks as
NDJSON.

Best-effort RF localization only — **not** survey-grade; **not** a product 3D
claim. Pair with `wifi-radar-node` (`--publish` / `--interval`) for live
observation ingest.

```bash
cargo run -p wifi-radar-fuse -- --solver mlat \
  crates/wifi-radar-core/tests/fixtures/three_node_lab_node_a.json \
  crates/wifi-radar-core/tests/fixtures/three_node_lab_node_b.json \
  crates/wifi-radar-core/tests/fixtures/three_node_lab_node_c.json

# Continuous TCP (default): accept many clients; fuse after idle silence.
cargo run -p wifi-radar-fuse -- --listen 127.0.0.1:9800 --solver mlat
# Optional: --idle-ms 500  (silence before an epoch fuse)
# Optional: --oneshot      (accept one client, fuse once, exit)

# View epochs in Ratatui (keyboard on /dev/tty; stdin is the pipe)
cargo run -p wifi-radar-fuse -- --listen 127.0.0.1:9800 --solver mlat \
  | cargo run -p wifi-radar-tui -- --fused
```

Solvers: `centroid`, `mlat` (default), `hybrid`, `mlat-robust`.
