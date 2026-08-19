# WiFi Radar

Interactive terminal tool that maps nearby **WiFi hotspots** (APs), **client adapters**, and **probe-only devices** (phones/laptops searching for networks) on a polar radar display.

## What it detects

| Symbol | Type | Source |
|--------|------|--------|
| ◉ | Hotspot / listening AP | `nmcli` managed scan + `airodump-ng` beacons |
| ◎ | Client adapter | `airodump-ng` associated stations |
| ○ | Normal WiFi adapter (unassociated) | Probe requests via `tshark` / passive monitor |

## Distance & direction (estimates)

- **Distance** — derived from RSSI using a log-distance path-loss model. Indoors this is only a rough order-of-magnitude (meters), not GPS accuracy.
- **Direction** — true bearing needs either a directional antenna or **calibration**: press `c`, slowly rotate your laptop, and use `←` `→` to match your physical heading. The tool records RSSI vs. heading and places each device toward its strongest signal. Without calibration, devices are placed at a stable pseudo-angle by MAC hash (marked *uncalibrated* in the detail line).

## Requirements

Assumed on PATH:

- `iw`, `ip`, `nmcli` — interface & AP scan
- `airodump-ng` — passive monitor scan (APs + clients)
- `tshark` — optional probe-request / adapter enrichment
- `sudo` — for monitor interface (`wlan0mon`) creation

## Usage

```bash
# Full scan (AP + monitor clients/adapters)
sudo ./wifi_radar.py

# Different interface
sudo ./wifi_radar.py -i wlan0

# AP hotspots only (no monitor mode)
./wifi_radar.py --no-monitor

# Refresh vendor names (IEEE OUI database)
./wifi_radar.py --fetch-oui
```

### Keys

| Key | Action |
|-----|--------|
| `q` | Quit |
| `r` | Force rescan |
| `m` | Toggle monitor-mode sampling |
| `c` | Start/stop bearing calibration (rotate laptop while calibrating) |
| `←` `→` | Adjust heading indicator / calibration bearing |
| `↑` `↓` | Select device in list |
| `Enter` | Show detail for selected device |

## Legal & ethics

Only use on networks and airspace you are allowed to monitor. Passive scanning may still be restricted in some jurisdictions. Do not use this tool to access networks without authorization.

## Limitations

- Single radio cannot measure true angle without rotation calibration or directional hardware.
- Randomized MACs (modern phones) appear as changing addresses; vendor detection may show “randomized MAC”.
- 5 GHz and 2.4 GHz require channel hopping; `airodump-ng --band abg` covers both but brief samples may miss quiet devices.
- Your own machine’s traffic dominates when associated to an AP on the same channel.
