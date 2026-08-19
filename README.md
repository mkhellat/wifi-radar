# wifi-radar

[![License: GPL v3+](https://img.shields.io/badge/license-GPL%20v3+-green.svg)](https://www.gnu.org/licenses/gpl-3.0)
[![Python: 3.11+](https://img.shields.io/badge/python-3.11+-blue.svg)](https://www.python.org)
[![OS: Linux](https://img.shields.io/badge/os-Linux-yellow.svg)](https://kernel.org)

Interactive terminal radar that maps nearby **WiFi access points**, **associated clients**, and **probe-only adapters** on a polar display. Estimates distance from RSSI and direction from calibration or MAC-hash placeholder.

> **Passive observation only.** No deauth, injection, association, or cracking.

## What it detects

| Glyph | Kind | Source |
|-------|------|--------|
| ◉ | Hotspot / AP | `iw scan` (managed) + `airodump-ng` beacons |
| ◎ | Associated client | `airodump-ng` stations |
| ○ | Probe-only adapter | `tshark` probe requests |

## Requirements

**Linux** with nl80211-compatible WiFi hardware.

System tools (must be on `$PATH`):

| Tool | Required | Purpose |
|------|----------|---------|
| `iw` | Always | AP scan, interface discovery |
| `ip` | For monitor | Bring up monitor VIF |
| `airodump-ng` | Optional | Client/probe passive capture |
| `tshark` | Optional | Probe-request enrichment |

Root (`sudo`) is needed for monitor-mode features.

Python 3.11 or later (stdlib only — no pip dependencies at runtime).

## Quick start

```bash
git clone https://github.com/mkhellat/wifi-radar.git
cd wifi-radar
./configure          # detect Python, system tools, write config.mk
make                 # create .venv, install package + dev deps
make check           # lint + type-check + test
sudo .venv/bin/wifi-radar
```

Run `./configure --help` for options and `make help` for all targets.

## Install (pip)

```bash
# From PyPI (when published)
pip install wifi-radar

# Editable install (without the build system)
pip install -e ".[dev]"
```

## Usage

```bash
# Auto-detect interface, full scan (AP + monitor)
sudo wifi-radar

# Specify interface
sudo wifi-radar -i wlan0

# AP scan only (no monitor mode, no root needed)
wifi-radar --no-monitor

# Download IEEE OUI vendor database
wifi-radar --fetch-oui

# Show version
wifi-radar --version
```

## Keys

| Key | Action |
|-----|--------|
| `q` / `Esc` | Quit |
| `r` | Force rescan |
| `m` | Toggle monitor-mode sampling |
| `c` | Start/stop bearing calibration |
| `←` `→` | Rotate heading (plot rotates with you) |
| `↑` `↓` | Select device in list |
| `Enter` | Show detail for selected device |

## Distance & direction

- **Distance** — log-distance path-loss model with reference RSSI at 1 m. Indoor accuracy is order-of-magnitude (metres), not GPS.
- **Direction** — requires calibration: press `c`, slowly rotate your laptop, use `←`/`→` to set heading. The tool records RSSI vs heading and places devices toward their strongest signal. Without calibration, devices sit at a stable pseudo-angle from MAC hash.

## Architecture

```
src/wifi_radar/
  cli.py          CLI entry point
  models.py       WifiDevice, DeviceKind
  merge.py        DeviceStore (EMA RSSI, TTL expiry)
  worker.py       Background scan thread
  iface.py        Interface discovery, MonitorSession
  oui.py          OUI vendor lookup/fetch
  util.py         MAC normalization
  scan/
    iw.py         Parse `iw dev <iface> scan`
    airodump.py   Parse airodump-ng CSV
    tshark.py     Parse tshark probe fields
  ui/
    radar.py      Polar drawing (curses)
    app.py        Main loop, keys, calibration
```

## Development

```bash
./configure --with-docs   # include Sphinx deps
make                      # venv + editable install + [dev] + [docs]
make check                # ruff + mypy + pytest
make docs                 # build HTML docs in docs/_build/html/
make format               # auto-format with ruff
make clean                # remove venv and caches
make distclean            # also remove config.mk
```

## Mirrors

- GitHub: https://github.com/mkhellat/wifi-radar
- Codeberg: https://codeberg.org/mkhellat/wifi-radar

## Legal & ethics

Only use on networks and airspace you are authorised to monitor. Passive scanning may still be restricted in some jurisdictions. This tool does not access, associate with, or attack any network.

## License

[GPL-3.0-or-later](LICENSE)
