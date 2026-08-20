# Changelog

## 0.1.0 — 2026-08-19

Initial structured release (from prototype checkpoint 0).

### Added

- Package layout (`src/wifi_radar/`, `pyproject.toml`, console script `wifi-radar`)
- `iw scan` parser (state machine, freq→channel, golden-file tested)
- `airodump-ng` CSV parser (APs + clients/adapters, probe SSIDs)
- `tshark` probe-request parser
- `DeviceStore` with EMA RSSI (α=0.4) and configurable TTL expiry
- Distance model: reference RSSI at 1 m, path-loss exponent n=3.0, 5 GHz correction
- Background scan worker thread (UI never blocks on subprocess)
- Curses polar radar with `addstr` glyphs, heading rotation, select-by-MAC
- CLI: `--fetch-oui` exits cleanly, `--no-monitor` skips airodump/tshark,
  auto-detect interface via `iw dev`, tool checks only for what's needed
- OUI vendor lookup from IEEE CSV cache
- Interface discovery and monitor-mode lifecycle (`iw` + `ip`)
- RF theory and capability assessment (Sphinx Theory section under ``docs/theory/``)
- GPL-3.0-or-later license

### Fixed (from checkpoint 0 defects)

- BSSID parse: replaced broken `nmcli -t` split with `iw scan` state machine
- Unicode glyphs: `addstr` instead of `addch`
- RSSI latch: EMA replaces `max()` forever; devices expire via TTL
- UI freeze: scans run in background thread
- Selection: bound to MAC, not volatile list index
- Heading: plot rotates (`bearing - heading`) so forward is always up
- `--no-monitor` no longer requires `airodump-ng`
- `--fetch-oui` downloads and exits (does not launch UI)
- Default interface auto-detected (not hardcoded `wlan0`)
- Station BSSID `(not associated)` filtered out
