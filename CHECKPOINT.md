# Checkpoint 0 — baseline review

**Date:** 2026-08-11  
**Path:** `/home/desadm/Projects/__1__small___/wifi-radar/`  
**Status:** prototype, not a professional tree  
**Inventory:** `wifi_radar.py`, `README.md` (plus this checkpoint and agent memory)

This file is the starting point for later sessions. Do not treat the
current script as correct just because it runs.

---

## Intended behavior

| Glyph | Kind | Source |
|-------|------|--------|
| ◉ | Hotspot / AP | `nmcli` managed scan + `airodump-ng` beacons |
| ◎ | Associated client | `airodump-ng` stations |
| ○ | Probe-only adapter | `tshark` probe requests |

- Distance: log-distance path loss (`TX_POWER_DBM=20`, `n=2.7`), capped 0.5–120 m
- Bearing: `hashlib.md5(mac)` until calibration (`c` + `←`/`→`); peak RSSI wins
- Keys: `q` quit, `r` rescan, `m` monitor toggle, `c` calibrate, arrows, Enter

## What is already sound

- Device model: `WifiDevice` / `DeviceKind`, merge priority hotspot > client > adapter
- Subprocess calls use argument lists (no `shell=True`)
- Monitor iface teardown in `finally`; temp dump dir removed
- airodump CSV column indices match current `airodump-ng` (AP ESSID=13, station BSSID=5)
- OUI cache + locally-administered MAC bit is the right vendor approach
- README legal note must stay in front of any later packaging

---

## Defects (reviewed 2026-08-11)

### Critical

1. **`scan_access_points` BSSID parse is broken**  
   `nmcli -t` escapes colons as `\:`. `line.split(":")` yields `AA\`, not a MAC.
   `parts[1].replace("\\", "")` becomes `"AA"`. Channel/signal fall back to
   `0` / `-75 dBm`. Primary AP path produces garbage. SSIDs containing `:`
   make it worse. Need an unescape splitter (or drop nmcli and trust airodump).

2. **Unicode via `curses.addch`**  
   `◉` / `◎` / `○` are not Latin-1 `chtype`s. Use `addstr`.

3. **RSSI latch**  
   `existing.rssi_dbm = max(...)` — blips never recede. Need EMA / last-N
   median and a `last_seen` TTL so ghosts expire.

### High

4. Scans run on the curses thread (`sleep` + 4s airodump + 2s tshark). UI freezes.
5. `selected` is an index into a list re-sorted by RSSI every frame. Bind to MAC.
6. Heading only moves the sweep line; device bearings stay fixed. Either rotate
   the plot by `-heading` or stop calling the sweep “heading.”
7. Calibration `feed()` records the same stale RSSI every 200 ms while scans
   run every ~8–12 s. Peak-RSSI→bearing is mostly knob noise.
8. `require_tools()` always demands `airodump-ng`, including `--no-monitor`.
9. `--fetch-oui` downloads then still launches the UI.
10. Default iface `wlan0` is not discovered or validated. No NM “unmanaged”
    handshake; concurrent managed + monitor VIFs fail on many drivers.

### Medium

11. `FREQ_MHZ_2G4` is unused; 2.4 and 5 GHz use the same path-loss.
12. 20 dBm TX assumed for phones/clients — ranges systematically long.
13. Devices never expire; own MAC / associated BSSID not filtered.
14. Station BSSID `(not associated)` is passed through `normalize_mac`.
15. airodump CSV split on `\n\n` is fragile if the file is not flushed.
16. `IN-USE` / `SECURITY` are requested from nmcli and then ignored.
17. Tiny terminals can make `max_r <= 0`.

---

## Not in the tree yet

No git metadata required here, but professionally missing:

- tests, `pyproject.toml` / packaging, `LICENSE` (GPL-3.0-or-later intended)
- logging, iface auto-detect, non-blocking scan worker
- structured package (`wifi_radar/` modules) instead of one 727-line file

## Resume order (when work continues)

1. Unescape `nmcli` (or stop using it for BSSID)
2. `addstr` for glyphs
3. Decaying RSSI + device TTL
4. Select-by-MAC
5. Scan off the UI thread

Do not add attack features. Do not scan other directories in this workspace.
