"""Prefer Rust ``CalibrationStore`` when ``wifi_radar_native`` is available.

Keeps the same method surface as :class:`wifi_radar.calibration.CalibrationStore`
so :class:`wifi_radar.merge.DeviceStore` and the curses UI can stay unaware of
the backend. Falls back to the pure-Python store when the extension is missing
or lacks the calibration API.
"""

from __future__ import annotations

import tempfile
from pathlib import Path
from typing import Any

from wifi_radar.calibration import CALIB_PATH, CalibrationMode, CalibrationStore
from wifi_radar.native import available, native


class NativeCalibrationStore:
    """Thin adapter around ``wifi_radar_native.CalibrationStore``."""

    def __init__(self, inner: Any) -> None:
        self._inner = inner

    def rssi_at_1m(self, mac: str) -> float | None:
        value = self._inner.rssi_at_1m(mac)
        return None if value is None else float(value)

    def set_distance_reference(
        self,
        mac: str,
        rssi_dbm: float,
        distance_m: float,
        *,
        path_loss_n: float | None = None,
    ) -> float:
        # Rust core uses the indoor path-loss constant; path_loss_n is accepted
        # for call-site compatibility with the Python store.
        _ = path_loss_n
        return float(self._inner.set_distance_reference(mac, rssi_dbm, distance_m))

    def set_manual_bearing(self, mac: str, bearing_deg: float) -> None:
        self._inner.set_manual_bearing(mac, bearing_deg)

    def manual_bearing(self, mac: str) -> float | None:
        value = self._inner.manual_bearing(mac)
        return None if value is None else float(value)

    def distance_anchor_m(self, mac: str) -> float | None:
        value = self._inner.distance_anchor_m(mac)
        return None if value is None else float(value)

    def mode(self) -> CalibrationMode:
        mode = str(self._inner.mode())
        return "anchor" if mode == "anchor" else "honest"

    def set_mode(self, mode: CalibrationMode) -> None:
        self._inner.set_mode("anchor" if mode == "anchor" else "honest")

    def has_distance_calibration(self, mac: str) -> bool:
        return bool(self._inner.has_distance_calibration(mac))

    def has_manual_bearing(self, mac: str) -> bool:
        return bool(self._inner.has_manual_bearing(mac))

    def clear(self, mac: str) -> None:
        self._inner.clear(mac)


def native_calibration_ready() -> bool:
    """True when the extension exposes a usable CalibrationStore API."""
    if not available() or native is None:
        return False
    store_cls = getattr(native, "CalibrationStore", None)
    if store_cls is None:
        return False
    required = (
        "memory",
        "mode",
        "set_mode",
        "set_manual_bearing",
        "manual_bearing",
        "set_distance_reference",
        "clear",
        "rssi_at_1m",
        "distance_anchor_m",
        "has_distance_calibration",
        "has_manual_bearing",
    )
    return all(hasattr(store_cls, name) for name in required)


def make_calibration_store(
    path: Path | None = None,
    *,
    memory: bool = False,
) -> CalibrationStore | NativeCalibrationStore:
    """Return a native-backed store when possible, else pure Python."""
    if native_calibration_ready():
        assert native is not None
        store_cls = native.CalibrationStore
        if memory:
            return NativeCalibrationStore(store_cls.memory())
        # Default native path matches ~/.cache/wifi_radar/calibration.json.
        if path is None or path == CALIB_PATH:
            return NativeCalibrationStore(store_cls())
        # Custom paths stay on the Python implementation (native only
        # exposes the default cache path today).
    if memory:
        tmp = Path(tempfile.mkdtemp(prefix="wifi-radar-calib-")) / "calibration.json"
        return CalibrationStore(path=tmp)
    return CalibrationStore(path=path or CALIB_PATH)
