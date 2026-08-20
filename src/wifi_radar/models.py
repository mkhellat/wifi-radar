"""Core data models for wifi-radar."""

from __future__ import annotations

import hashlib
import time
from dataclasses import dataclass, field
from enum import Enum

PATH_LOSS_EXPONENT_INDOOR = 3.0
RSSI_AT_1M_AP = -30.0  # typical RSSI 1 metre from an AP
RSSI_AT_1M_CLIENT = -35.0  # typical RSSI 1 metre from a phone/laptop


class DeviceKind(Enum):
    HOTSPOT = "hotspot"
    CLIENT = "client"
    ADAPTER = "adapter"


@dataclass
class WifiDevice:
    mac: str
    kind: DeviceKind
    ssid: str = ""
    vendor: str = ""
    channel: int = 0
    freq_mhz: int = 0
    rssi_dbm: float = -90.0
    last_seen: float = field(default_factory=time.time)
    probe_ssids: list[str] = field(default_factory=list)
    associated_bssid: str = ""
    bearing_deg: float | None = None
    bearing_confidence: float = 0.0
    bearing_manual: bool = False
    security: str = ""
    in_use: bool = False

    @property
    def label(self) -> str:
        if self.ssid:
            return self.ssid[:18]
        if self.probe_ssids:
            return self.probe_ssids[0][:18]
        return self.mac[-8:]

    @property
    def rssi_at_1m(self) -> float:
        if self.kind == DeviceKind.HOTSPOT:
            return RSSI_AT_1M_AP
        return RSSI_AT_1M_CLIENT

    rssi_at_1m_override: float | None = None

    def distance_m(self) -> float:
        """Log-distance path-loss estimate (very approximate indoors).

        Uses the formula: d = 10^((RSSI_1m - RSSI) / (10*n))
        where RSSI_1m is a calibration constant for signal at 1 metre.
        """
        ref = self.rssi_at_1m_override if self.rssi_at_1m_override is not None else self.rssi_at_1m
        n = PATH_LOSS_EXPONENT_INDOOR
        if self.freq_mhz > 4000:
            n += 0.3  # 5 GHz attenuates faster through walls
        exponent = (ref - self.rssi_dbm) / (10.0 * n)
        return float(min(120.0, max(0.1, 10.0**exponent)))

    def display_bearing(self) -> float:
        if self.bearing_deg is not None:
            return self.bearing_deg % 360.0
        h = int(hashlib.md5(self.mac.encode()).hexdigest()[:8], 16)  # noqa: S324
        return float(h % 360)
