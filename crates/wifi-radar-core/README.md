# wifi-radar-core

Domain core for wifi-radar: device models, `DeviceStore` (EMA RSSI + TTL),
`ScanBackend` trait, MAC/ranging helpers, `CalibrationStore` (JSON under
`~/.cache/wifi_radar/calibration.json`, Python-compatible), and scene
`correction` (`apply_scene_corrections`, honest/anchor modes). Multi-node
sync lands later.

```bash
cargo test -p wifi-radar-core
```

Passive observation only — no injection, deauth, or association attacks.
