"""Tests for DeviceStore merge and TTL."""

from __future__ import annotations

import time

from wifi_radar.merge import RSSI_EMA_ALPHA, DeviceStore
from wifi_radar.models import DeviceKind, WifiDevice


def _make_dev(mac: str = "aa:bb:cc:dd:ee:ff", rssi: float = -50.0) -> WifiDevice:
    return WifiDevice(mac=mac, kind=DeviceKind.HOTSPOT, rssi_dbm=rssi)


def test_merge_new_device() -> None:
    store = DeviceStore()
    store.merge([_make_dev()])
    assert len(store.devices) == 1
    assert "aa:bb:cc:dd:ee:ff" in store.devices


def test_merge_ema_rssi() -> None:
    store = DeviceStore()
    store.merge([_make_dev(rssi=-60.0)])
    store.merge([_make_dev(rssi=-40.0)])
    dev = store.devices["aa:bb:cc:dd:ee:ff"]
    expected = RSSI_EMA_ALPHA * (-40.0) + (1 - RSSI_EMA_ALPHA) * (-60.0)
    assert abs(dev.rssi_dbm - expected) < 0.01


def test_rssi_decays_with_weaker_signal() -> None:
    store = DeviceStore()
    store.merge([_make_dev(rssi=-30.0)])
    store.merge([_make_dev(rssi=-70.0)])
    dev = store.devices["aa:bb:cc:dd:ee:ff"]
    assert dev.rssi_dbm < -30.0  # not stuck at max


def test_expire_removes_stale() -> None:
    store = DeviceStore(ttl=0.0)
    store.merge([_make_dev()])
    store.devices["aa:bb:cc:dd:ee:ff"].last_seen = time.time() - 1.0
    expired = store.expire()
    assert "aa:bb:cc:dd:ee:ff" in expired
    assert len(store.devices) == 0


def test_kind_priority() -> None:
    store = DeviceStore()
    store.merge([WifiDevice(mac="aa:bb:cc:dd:ee:ff", kind=DeviceKind.ADAPTER, rssi_dbm=-50.0)])
    store.merge([WifiDevice(mac="aa:bb:cc:dd:ee:ff", kind=DeviceKind.CLIENT, rssi_dbm=-50.0)])
    assert store.devices["aa:bb:cc:dd:ee:ff"].kind == DeviceKind.CLIENT
    store.merge([WifiDevice(mac="aa:bb:cc:dd:ee:ff", kind=DeviceKind.HOTSPOT, rssi_dbm=-50.0)])
    assert store.devices["aa:bb:cc:dd:ee:ff"].kind == DeviceKind.HOTSPOT
    # Hotspot should not be downgraded
    store.merge([WifiDevice(mac="aa:bb:cc:dd:ee:ff", kind=DeviceKind.ADAPTER, rssi_dbm=-50.0)])
    assert store.devices["aa:bb:cc:dd:ee:ff"].kind == DeviceKind.HOTSPOT


def test_snapshot_sorted_by_rssi() -> None:
    store = DeviceStore()
    store.merge([
        _make_dev("aa:bb:cc:dd:ee:01", -80.0),
        _make_dev("aa:bb:cc:dd:ee:02", -30.0),
        _make_dev("aa:bb:cc:dd:ee:03", -55.0),
    ])
    snap = store.snapshot()
    assert snap[0].mac == "aa:bb:cc:dd:ee:02"
    assert snap[-1].mac == "aa:bb:cc:dd:ee:01"


def test_get_normalizes_and_missing() -> None:
    store = DeviceStore()
    store.merge([_make_dev()])
    assert store.get("AA-BB-CC-DD-EE-FF") is not None
    assert store.get("00:00:00:00:00:99") is None


def test_set_and_clear_bearing_write_through() -> None:
    store = DeviceStore()
    store.merge([_make_dev()])
    store.set_bearing("AA:BB:CC:DD:EE:FF", 90.0, confidence=0.5, manual=True)
    dev = store.get("aa:bb:cc:dd:ee:ff")
    assert dev is not None
    assert dev.bearing_deg == 90.0
    assert dev.bearing_manual is True
    assert dev.bearing_confidence == 0.5
    store.clear_bearing("aa:bb:cc:dd:ee:ff")
    dev = store.get("aa:bb:cc:dd:ee:ff")
    assert dev is not None
    assert dev.bearing_deg is None
    assert dev.bearing_manual is False
    assert dev.bearing_confidence == 0.0


def test_set_bearing_unknown_mac_is_noop() -> None:
    store = DeviceStore()
    store.set_bearing("00:00:00:00:00:99", 10.0)  # no device, must not raise
    assert store.get("00:00:00:00:00:99") is None
