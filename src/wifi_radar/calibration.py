"""Persistent per-device calibration (distance reference, manual bearing)."""

from __future__ import annotations

import json
import math
from pathlib import Path

from wifi_radar.models import PATH_LOSS_EXPONENT_INDOOR

CALIB_PATH = Path.home() / ".cache" / "wifi_radar" / "calibration.json"


class CalibrationStore:
    """Load/save per-MAC RSSI-at-1m overrides and manual bearings."""

    def __init__(self, path: Path = CALIB_PATH) -> None:
        self._path = path
        self._rssi_at_1m: dict[str, float] = {}
        self._manual_bearing: dict[str, float] = {}
        self.load()

    def load(self) -> None:
        if not self._path.exists():
            return
        try:
            data = json.loads(self._path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            return
        self._rssi_at_1m = {k.lower(): float(v) for k, v in data.get("rssi_at_1m", {}).items()}
        self._manual_bearing = {
            k.lower(): float(v) for k, v in data.get("manual_bearing", {}).items()
        }

    def save(self) -> None:
        self._path.parent.mkdir(parents=True, exist_ok=True)
        payload = {
            "rssi_at_1m": self._rssi_at_1m,
            "manual_bearing": self._manual_bearing,
        }
        self._path.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")

    def rssi_at_1m(self, mac: str) -> float | None:
        return self._rssi_at_1m.get(mac.lower())

    def set_distance_reference(
        self,
        mac: str,
        rssi_dbm: float,
        distance_m: float,
        *,
        path_loss_n: float = PATH_LOSS_EXPONENT_INDOOR,
    ) -> float:
        """Calibrate so current RSSI maps to the given distance. Returns new ref."""
        distance_m = max(0.1, distance_m)
        ref = rssi_dbm + 10.0 * path_loss_n * math.log10(distance_m)
        self._rssi_at_1m[mac.lower()] = ref
        self.save()
        return ref

    def set_manual_bearing(self, mac: str, bearing_deg: float) -> None:
        self._manual_bearing[mac.lower()] = bearing_deg % 360.0
        self.save()

    def manual_bearing(self, mac: str) -> float | None:
        val = self._manual_bearing.get(mac.lower())
        return None if val is None else val % 360.0

    def clear(self, mac: str) -> None:
        key = mac.lower()
        self._rssi_at_1m.pop(key, None)
        self._manual_bearing.pop(key, None)
        self.save()
