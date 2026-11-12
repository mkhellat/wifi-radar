# wifi-radar-core

Domain core for wifi-radar: device models, `DeviceStore` (EMA RSSI + TTL),
`ScanBackend` trait, MAC/ranging helpers, `CalibrationStore` (JSON under
`~/.cache/wifi_radar/calibration.json`, Python-compatible), scene
`correction` (`apply_scene_corrections`, honest/anchor modes), and a
JSON `NodeObservation` schema, TCP NDJSON helpers (`obs_transport`), and
best-effort 2D fusion (`associate_observations`, `weighted_centroid_2d`,
`multilateration_2d`, `hybrid_bearing_range_2d`). Not survey-grade; no 3D
product claims.

```bash
cargo test -p wifi-radar-core
```

Passive observation only — no injection, deauth, or association attacks.
