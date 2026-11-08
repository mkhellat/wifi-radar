# wifi-radar-tui

Primary interactive UI for wifi-radar (Ratatui MVP).

## Run

```bash
# Synthetic devices (no root / no radio)
cargo run -p wifi-radar-tui -- --demo

# Live scan: prefer nl80211 GET_SCAN dump, fall back to iw-cli
cargo run -p wifi-radar-tui -- -i wlan0
```

Keys: `j`/`k` select, `r` rescan, `q` quit.

Live mode tries genetlink `nl80211` first; if that errors, the same pass
falls back to `iw-cli`. Status line shows which backend won.

The UI shows a **polar canvas** (rings, sweep, device glyphs) above the
device table. Heading rotation and calibration keys land in follow-up PRs.

Passive observation only — no injection, deauth, or association.
