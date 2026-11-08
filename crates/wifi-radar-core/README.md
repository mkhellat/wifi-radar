# wifi-radar-core

Domain core for wifi-radar: device models, `DeviceStore` (EMA RSSI + TTL),
`ScanBackend` trait, MAC/ranging helpers, and `CalibrationStore` (JSON under
`~/.cache/wifi_radar/calibration.json`, Python-compatible). Scene correction
engine and multi-node sync land later.

```bash
cargo test -p wifi-radar-core
```

Passive observation only — no injection, deauth, or association attacks.
