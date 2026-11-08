# wifi-radar-linux

Linux RF backends for wifi-radar.

- **Target:** native nl80211 (and later spectral scan) in `src/nl80211.rs`
- **Interim:** `IwCliBackend` in `src/iw.rs` (`iw dev … scan`) behind `ScanBackend`
- **Stub:** `Nl80211Backend` still returns an error until netlink scan lands

```bash
cargo test -p wifi-radar-linux
```

Passive observation only.
