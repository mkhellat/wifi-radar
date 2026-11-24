# SPDX-FileCopyrightText: 2026 wifi-radar contributors
# SPDX-License-Identifier: GPL-3.0-or-later

"""Prefer the Rust ``DeviceStore`` when ``wifi_radar_native`` is available.

Mirrors :mod:`wifi_radar.calib_bridge`: :class:`NativeDeviceStore` keeps the
same method surface as :class:`wifi_radar.merge.DeviceStore` (merge / expire /
snapshot plus the ``get`` / ``set_bearing`` / ``clear_bearing`` write-through
helpers) so the curses UI stays unaware of the backend. EMA RSSI merge and TTL
expiry run in the Rust core; per-MAC calibration overlay stays in Python to
match the pure-Python snapshot semantics. Falls back to the Python store when
the extension is missing or lacks the DeviceStore API.
"""

from __future__ import annotations

from typing import Any

from wifi_radar.calib_bridge import make_calibration_store
from wifi_radar.merge import DEFAULT_TTL_SEC, CalibrationBackend, DeviceStore
from wifi_radar.models import DeviceKind, WifiDevice
from wifi_radar.native import available, native
from wifi_radar.util import normalize_mac


def _device_from_row(row: dict[str, Any]) -> WifiDevice:
    """Build a :class:`WifiDevice` from a native snapshot/get dict row."""
    kind_raw = str(row.get("kind", "hotspot"))
    try:
        kind = DeviceKind(kind_raw)
    except ValueError:
        kind = DeviceKind.HOTSPOT
    dev = WifiDevice(mac=str(row["mac"]), kind=kind, rssi_dbm=float(row["rssi_dbm"]))
    dev.ssid = str(row.get("ssid", ""))
    dev.vendor = str(row.get("vendor", ""))
    dev.channel = int(row.get("channel", 0))
    dev.freq_mhz = int(row.get("freq_mhz", 0))
    dev.security = str(row.get("security", ""))
    dev.in_use = bool(row.get("in_use", False))
    dev.associated_bssid = str(row.get("associated_bssid", ""))
    dev.probe_ssids = [str(s) for s in row.get("probe_ssids", [])]
    last_seen = row.get("last_seen")
    if last_seen is not None:
        dev.last_seen = float(last_seen)
    cal_b = row.get("calibrated_bearing_deg")
    dev.bearing_deg = None if cal_b is None else float(cal_b)
    ov = row.get("bearing_override_deg")
    dev.bearing_override_deg = None if ov is None else float(ov)
    dev.bearing_confidence = float(row.get("bearing_confidence", 0.0))
    dev.bearing_manual = bool(row.get("bearing_manual", False))
    dist = row.get("distance_override_m")
    dev.distance_override_m = None if dist is None else float(dist)
    ref = row.get("rssi_at_1m_override")
    dev.rssi_at_1m_override = None if ref is None else float(ref)
    dev.anchor_status = str(row.get("anchor_status", ""))
    dev.correction_note = str(row.get("correction_note", ""))
    return dev


def _row_from_device(dev: WifiDevice) -> dict[str, Any]:
    """Serialize a :class:`WifiDevice` into a native merge dict row."""
    return {
        "mac": dev.mac,
        "kind": dev.kind.value,
        "ssid": dev.ssid,
        "vendor": dev.vendor,
        "channel": dev.channel,
        "freq_mhz": dev.freq_mhz,
        "rssi_dbm": dev.rssi_dbm,
        "security": dev.security,
        "in_use": dev.in_use,
        "associated_bssid": dev.associated_bssid,
        "probe_ssids": dev.probe_ssids,
        "calibrated_bearing_deg": dev.bearing_deg,
        "bearing_confidence": dev.bearing_confidence,
        "bearing_manual": dev.bearing_manual,
    }


class NativeDeviceStore:
    """Rust-backed device registry duck-typing :class:`DeviceStore`.

    EMA RSSI merge and TTL expiry run in ``wifi_radar_native.DeviceStore``; a
    MAC->WifiDevice cache is refreshed after each merge so the UI can read a
    device cheaply, and bearing edits write through to the Rust store.
    """

    def __init__(self, calibration: CalibrationBackend | None = None) -> None:
        assert native is not None
        self._inner = native.DeviceStore()
        self._cache: dict[str, WifiDevice] = {}
        self.calibration = calibration or make_calibration_store()

    @property
    def devices(self) -> dict[str, WifiDevice]:
        return self._cache

    def _refresh_cache(self) -> None:
        self._cache = {
            dev.mac: dev for dev in (_device_from_row(r) for r in self._inner.snapshot())
        }

    def merge(self, incoming: list[WifiDevice]) -> None:
        """Merge a scan batch through the Rust EMA store, refreshing the cache."""
        self._inner.merge([_row_from_device(d) for d in incoming])
        self._refresh_cache()

    def expire(self) -> list[str]:
        """Expire stale entries in the Rust store and prune the cache."""
        removed = [str(mac) for mac in self._inner.expire()]
        for mac in removed:
            self._cache.pop(mac, None)
        return removed

    def snapshot(self) -> list[WifiDevice]:
        """Return calibrated copies sorted strongest-first (Python parity)."""
        out: list[WifiDevice] = []
        for row in self._inner.snapshot():
            dev = _device_from_row(row)
            ref = self.calibration.rssi_at_1m(dev.mac)
            if ref is not None:
                dev.rssi_at_1m_override = ref
            manual = self.calibration.manual_bearing(dev.mac)
            if manual is not None and dev.bearing_deg is None:
                dev.bearing_deg = manual
                dev.bearing_manual = True
                dev.bearing_confidence = 1.0
            out.append(dev)
        return sorted(out, key=lambda d: -d.rssi_dbm)

    def get(self, mac: str) -> WifiDevice | None:
        """Return the cached device for ``mac`` (normalized), or ``None``."""
        return self._cache.get(normalize_mac(mac))

    def set_bearing(
        self,
        mac: str,
        bearing_deg: float | None,
        *,
        confidence: float = 1.0,
        manual: bool = False,
    ) -> None:
        """Write a bearing through to the Rust store and the cache."""
        self._inner.set_bearing(mac, bearing_deg, confidence, manual)
        dev = self._cache.get(normalize_mac(mac))
        if dev is not None:
            dev.bearing_deg = bearing_deg
            dev.bearing_confidence = confidence
            dev.bearing_manual = manual

    def clear_bearing(self, mac: str) -> None:
        """Drop any calibrated/manual bearing on the Rust store and cache."""
        self.set_bearing(mac, None, confidence=0.0, manual=False)


def native_store_ready() -> bool:
    """True when the extension exposes a usable DeviceStore API."""
    if not available() or native is None:
        return False
    store_cls = getattr(native, "DeviceStore", None)
    if store_cls is None:
        return False
    required = ("merge", "get", "set_bearing", "snapshot", "expire")
    return all(hasattr(store_cls, name) for name in required)


DeviceStoreBackend = DeviceStore | NativeDeviceStore


def make_device_store(
    ttl: float = DEFAULT_TTL_SEC,
    calibration: CalibrationBackend | None = None,
) -> DeviceStoreBackend:
    """Return a native-backed device store when possible, else pure Python."""
    calibration = calibration or make_calibration_store()
    if native_store_ready():
        return NativeDeviceStore(calibration=calibration)
    return DeviceStore(ttl=ttl, calibration=calibration)
