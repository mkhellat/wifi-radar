# wifi-radar-py (`wifi_radar_native`)

Thin PyO3 / Maturin façade exposing the Rust core for scripting.

## Build / install (editable)

From this directory (requires [maturin](https://www.maturin.rs/) and a Python 3.11+ env):

```bash
pip install maturin
maturin develop --features extension-module
```

## API (MVP)

```python
import wifi_radar_native as wrn

assert wrn.version() == wrn.__version__

store = wrn.DeviceStore()
store.merge_hotspot("aa:bb:cc:dd:ee:01", -42.0, ssid="Demo", channel=6)
for row in store.snapshot():
    print(row["mac"], row["rssi_dbm"], row["distance_m"], row["bearing_deg"])
```

Passive observation helpers only — no injection, deauth, or association.
The full interactive app remains the Python curses `wifi-radar` script and the
Ratatui `wifi-radar-tui` binary until packaging is unified.
