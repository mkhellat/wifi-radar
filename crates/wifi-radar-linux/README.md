# wifi-radar-linux

Linux RF backends for wifi-radar.

- **Target:** native nl80211 (and later spectral scan)
- **Interim:** `IwCliBackend` (`iw dev … scan`) behind the core `ScanBackend` trait
- **Stub:** `Nl80211Backend` (compile-time placeholder until netlink lands)

```bash
cargo test -p wifi-radar-linux
```

Passive observation only.
