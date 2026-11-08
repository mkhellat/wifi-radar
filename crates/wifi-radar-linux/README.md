# wifi-radar-linux

Linux RF backends for wifi-radar.

- **Target:** native nl80211 (and later spectral scan)
- **Interim:** `iw` CLI shim behind the core `ScanBackend` trait

Passive observation only.
