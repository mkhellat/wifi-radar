# wifi-radar-py (`wifi_radar_native`)

Thin PyO3 / Maturin façade exposing the Rust core for scripting.

## Build / install (editable)

From this directory (requires [maturin](https://www.maturin.rs/) and a Python 3.11+ env):

```bash
pip install maturin
maturin develop --features extension-module
```

## API

```python
import wifi_radar_native as wrn

assert wrn.version() == wrn.__version__

store = wrn.DeviceStore()
store.merge_hotspot("aa:bb:cc:dd:ee:01", -42.0, ssid="Demo", channel=6)
cal = wrn.CalibrationStore.memory()
cal.set_distance_reference("aa:bb:cc:dd:ee:01", -42.0, 1.0)
cal.set_mode("honest")
assert cal.has_distance_calibration("aa:bb:cc:dd:ee:01")
rows, summary = store.snapshot_calibrated(cal)
print(summary["short_label"])
for row in rows:
    print(row["mac"], row["distance_m"], row.get("correction_note"))

# Or correct an already-built snapshot list of dicts:
rows, summary = wrn.apply_scene_corrections(rows, cal)
```

`CalibrationStore()` loads `~/.cache/wifi_radar/calibration.json` (same file as
the TUI / Python curses app). Use `.memory()` for tests. The Python package
prefers this façade via ``wifi_radar.calib_bridge`` when the extension is
installed.

Passive observation helpers only — no injection, deauth, or association.
The full interactive app remains the Python curses `wifi-radar` script and the
Ratatui `wifi-radar-tui` binary until packaging is unified.
