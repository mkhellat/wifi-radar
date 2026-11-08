# wifi-radar-core

Domain core for wifi-radar: device models, `DeviceStore` (EMA RSSI + TTL),
`ScanBackend` trait, and MAC/ranging helpers. Calibration / scene correction
and multi-node sync land here later.

```bash
cargo test -p wifi-radar-core
```

Passive observation only — no injection, deauth, or association attacks.
