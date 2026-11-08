# wifi-radar-linux

Linux RF backends for wifi-radar.

- **Target:** native nl80211 (and later spectral scan) in `src/nl80211.rs`
- **Interim:** `IwCliBackend` in `src/iw.rs` (`iw dev … scan`) behind `ScanBackend`
- **Decode:** `src/bss.rs` parses nested `NL80211_ATTR_BSS` into `WifiDevice`
- **Socket:** genetlink `TRIGGER_SCAN` (ifindex) then `GET_SCAN` dump via
  `neli`. If trigger fails (permissions / busy), dump still runs against the
  existing cache.

```bash
cargo test -p wifi-radar-linux
```

Passive observation only.
