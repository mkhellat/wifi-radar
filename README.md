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
./configure          # detect platform, generate Makefile from Makefile.in
make                 # create .venv, install package + dev deps
make check           # lint + type-check + test
sudo .venv/bin/wifi-radar
```

`Makefile.in` is the committed template. `./configure` generates the
machine-specific `Makefile`, following the traditional GNU/FOSS workflow.

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

## Keys (vi-style)

| Key | Action |
|-----|--------|
| `q` | Quit |
| `h` / `←` | Rotate heading left (5°) |
| `l` / `→` | Rotate heading right (5°) |
| `H` / `L` | Rotate heading fast (15°) |
| `j` / `↓` | Select next device |
| `k` / `↑` | Select previous device |
| `g` / `G` | Jump to first / last device |
| `r` | Force rescan |
| `m` | Toggle monitor mode |
| `c` | Start/stop bearing calibration |
| `Esc` | Deselect |
| `?` | Full-screen help overlay |
| `:` | Enter command mode (`:q` `:r` `:m` `:c` `:help`) |

## Distance & direction

- **Distance** — log-distance path-loss model with reference RSSI at 1 m. Indoor accuracy is order-of-magnitude (metres), not GPS. Select a device and press `D` / `d` to calibrate at 0.25 m / 1 m (saved under `~/.cache/wifi_radar/calibration.json`).
- **Direction** — manual pin with `4`/`6`/`8`/`2` (left/right/ahead/behind), rotation calibration (`c`), or MAC-hash placeholder. See [calibration guide](docs/user/calibration.rst) and the full [localization theory](docs/theory/localization.rst) (formulas, EMA, auto-scale, error budget).

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
make distclean            # also remove generated Makefile
```

## Mirrors

- GitHub: https://github.com/mkhellat/wifi-radar
- Codeberg: https://codeberg.org/mkhellat/wifi-radar

## Legal & ethics

Only use on networks and airspace you are authorised to monitor. Passive scanning may still be restricted in some jurisdictions. This tool does not access, associate with, or attack any network.

## License

[GPL-3.0-or-later](LICENSE)
