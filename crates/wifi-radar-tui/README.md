# wifi-radar-tui

Primary interactive UI for wifi-radar (Ratatui MVP).

## Run

```bash
# Synthetic devices (no root / no radio)
cargo run -p wifi-radar-tui -- --demo

# Live iw scan (often needs root / CAP_NET_ADMIN)
cargo run -p wifi-radar-tui -- -i wlan0
```

Keys: `j`/`k` select, `r` rescan, `q` quit.

Passive observation only — no injection, deauth, or association.
