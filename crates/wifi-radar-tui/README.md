# wifi-radar-tui

Primary interactive UI for wifi-radar (Ratatui MVP).

## Run

```bash
# Synthetic devices (no root / no radio)
cargo run -p wifi-radar-tui -- --demo

# Live scan: prefer nl80211 GET_SCAN dump, fall back to iw-cli
cargo run -p wifi-radar-tui -- -i wlan0
```

Keys: `h`/`l` heading (±5°), `H`/`L` fast (±15°), `j`/`k` select,
`g`/`G` first/last, `Esc` deselect, `r` rescan, `q` quit, `:` command.

Commands: `:mode honest` / `:mode anchor` (persisted), `:r` rescan,
`:q` quit.

With a device selected: `8`/`4`/`6`/`2` pin ahead/left/right/behind,
`D`/`d` distance calib at 0.25 m / 1 m, `x` clear calibration
(persisted under `~/.cache/wifi_radar/calibration.json` in live mode).
Header shows scene-correction status (`mode=… anchors=… distx=…`).

Live mode tries genetlink `nl80211` first; if that errors, the same pass
falls back to `iw-cli`. Status line shows which backend won.

The UI shows a **polar canvas** (rings, sweep, device glyphs) above the
device table. Selection is by MAC (stable across RSSI re-sorts).
Pin/distance calibration uses the shared Rust `CalibrationStore`; each
snapshot also runs `apply_scene_corrections` (honest distance scale /
anchor bearing propagation).

Passive observation only — no injection, deauth, or association.
