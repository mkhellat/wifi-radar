"""Device store with EMA RSSI, TTL expiry, and merge logic."""

from __future__ import annotations

import time

from wifi_radar.models import DeviceKind, WifiDevice
from wifi_radar.util import normalize_mac

DEFAULT_TTL_SEC = 30.0
RSSI_EMA_ALPHA = 0.4  # weight of new sample (higher = more responsive)


class DeviceStore:
    """Thread-safe-ish device registry with decaying RSSI and TTL."""

    def __init__(self, ttl: float = DEFAULT_TTL_SEC) -> None:
        self._devices: dict[str, WifiDevice] = {}
        self._ttl = ttl

    @property
    def devices(self) -> dict[str, WifiDevice]:
        return self._devices

    def merge(self, incoming: list[WifiDevice]) -> None:
        """Merge a batch of scanned devices into the store."""
        now = time.time()
        for d in incoming:
            d.mac = normalize_mac(d.mac)
            existing = self._devices.get(d.mac)
            if existing is None:
                d.last_seen = now
                self._devices[d.mac] = d
                continue

            existing.last_seen = now
            # EMA for RSSI instead of max-forever
            existing.rssi_dbm = (
                RSSI_EMA_ALPHA * d.rssi_dbm + (1 - RSSI_EMA_ALPHA) * existing.rssi_dbm
            )

            if d.ssid:
                existing.ssid = d.ssid
            if d.channel:
                existing.channel = d.channel
            if d.freq_mhz:
                existing.freq_mhz = d.freq_mhz
            if d.security:
                existing.security = d.security
            if d.in_use:
                existing.in_use = True

            # Kind priority: HOTSPOT > CLIENT > ADAPTER
            if d.kind == DeviceKind.HOTSPOT:
                existing.kind = DeviceKind.HOTSPOT
            elif d.kind == DeviceKind.CLIENT and existing.kind != DeviceKind.HOTSPOT:
                existing.kind = DeviceKind.CLIENT

            for s in d.probe_ssids:
                if s not in existing.probe_ssids:
                    existing.probe_ssids.append(s)
            if d.associated_bssid:
                existing.associated_bssid = d.associated_bssid
            if d.bearing_deg is not None:
                existing.bearing_deg = d.bearing_deg
                existing.bearing_confidence = max(existing.bearing_confidence, d.bearing_confidence)

    def expire(self) -> list[str]:
        """Remove devices not seen within TTL. Returns removed MACs."""
        now = time.time()
        expired = [mac for mac, dev in self._devices.items() if now - dev.last_seen > self._ttl]
        for mac in expired:
            del self._devices[mac]
        return expired

    def snapshot(self) -> list[WifiDevice]:
        """Return a sorted copy of current devices (strongest first)."""
        self.expire()
        return sorted(self._devices.values(), key=lambda d: -d.rssi_dbm)
