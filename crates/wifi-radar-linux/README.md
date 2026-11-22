# wifi-radar-linux

Linux RF backends for wifi-radar.

- **Target:** native nl80211 (and later spectral scan) in `src/nl80211.rs`
- **Interim:** `IwCliBackend` in `src/iw.rs` (`iw dev … scan`) behind `ScanBackend`
- **Decode:** `src/bss.rs` parses nested `NL80211_ATTR_BSS` into `WifiDevice`
- **Monitor:** `MonitorSession` creates `{phy}mon` (`iw` add type monitor,
  `ip link set up`, `iw del` on teardown). `sample_airodump` and
  `sample_tshark_probes` are passive listens on that VIF. Parsers are
  pure and covered by fixtures; the live spawn needs root and the tools.
- **Socket:** genetlink `TRIGGER_SCAN` (ifindex), wait for multicast
  `NEW_SCAN_RESULTS` / `SCAN_ABORTED` on the `nl80211` `scan` group (timed
  sleep fallback if join fails or no event), then `GET_SCAN` dump via `neli`.
  If trigger fails (permissions / busy), dump still runs against the existing
  cache.

```bash
cargo test -p wifi-radar-linux
```

Passive observation only.
