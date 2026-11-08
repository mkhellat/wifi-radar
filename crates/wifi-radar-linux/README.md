# wifi-radar-linux

Linux RF backends for wifi-radar.

- **Target:** native nl80211 (and later spectral scan) in `src/nl80211.rs`
- **Interim:** `IwCliBackend` in `src/iw.rs` (`iw dev … scan`) behind `ScanBackend`
- **Decode:** `src/bss.rs` parses nested `NL80211_ATTR_BSS` into `WifiDevice`
- **Socket:** genetlink `GET_SCAN` dump via `neli` (`Nl80211Backend::scan`);
  no `TRIGGER_SCAN` yet (cached results only, like `iw scan dump`)

```bash
cargo test -p wifi-radar-linux
```

Passive observation only.
