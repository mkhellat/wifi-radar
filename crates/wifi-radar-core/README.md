# wifi-radar-core

Domain core for wifi-radar: device models, `DeviceStore` (EMA RSSI + TTL),
`ScanBackend` trait, MAC/ranging helpers, `CalibrationStore` (JSON under
`~/.cache/wifi_radar/calibration.json`, Python-compatible), scene
`correction` (`apply_scene_corrections`, honest/anchor modes), and a
JSON `NodeObservation` schema for multi-node fusion, plus line-delimited
JSON observation helpers over TCP (`obs_transport`; no fusion solver).

```bash
cargo test -p wifi-radar-core
```

Passive observation only — no injection, deauth, or association attacks.
