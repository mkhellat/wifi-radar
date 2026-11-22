"""Native / Python device store bridge."""

from __future__ import annotations

import pytest

from wifi_radar.calib_bridge import make_calibration_store
from wifi_radar.merge import DeviceStore
from wifi_radar.models import DeviceKind, WifiDevice
from wifi_radar.store_bridge import (
    NativeDeviceStore,
    make_device_store,
    native_store_ready,
)


def _dev(mac: str = "aa:bb:cc:dd:ee:01", rssi: float = -50.0) -> WifiDevice:
    return WifiDevice(mac=mac, kind=DeviceKind.HOTSPOT, rssi_dbm=rssi)


def test_make_device_store_duck_types() -> None:
    store = make_device_store()
    assert isinstance(store, (DeviceStore, NativeDeviceStore))
    for name in ("merge", "expire", "snapshot", "get", "set_bearing", "clear_bearing"):
        assert hasattr(store, name)
    assert store.calibration is not None


def test_make_device_store_respects_backend_choice() -> None:
    store = make_device_store()
    if native_store_ready():
        assert isinstance(store, NativeDeviceStore)
    else:
        assert isinstance(store, DeviceStore)


def test_native_store_ready_returns_bool() -> None:
    assert isinstance(native_store_ready(), bool)


@pytest.mark.skipif(not native_store_ready(), reason="wifi_radar_native not built")
def test_native_store_merge_snapshot_and_write_through() -> None:
    store = NativeDeviceStore(calibration=make_calibration_store(memory=True))
    store.merge([_dev(rssi=-40.0), _dev("aa:bb:cc:dd:ee:02", -70.0)])
    snap = store.snapshot()
    assert [d.mac for d in snap] == ["aa:bb:cc:dd:ee:01", "aa:bb:cc:dd:ee:02"]

    # Second observation pulls RSSI down via the Rust EMA merge.
    store.merge([_dev(rssi=-80.0)])
    dev = store.get("AA:BB:CC:DD:EE:01")
    assert dev is not None
    assert dev.rssi_dbm < -40.0

    # Bearing write-through reaches the Rust store and the cache.
    store.set_bearing("aa:bb:cc:dd:ee:01", 120.0, confidence=0.6, manual=True)
    dev = store.get("aa:bb:cc:dd:ee:01")
    assert dev is not None
    assert dev.bearing_deg == 120.0
    assert dev.bearing_manual is True
    store.clear_bearing("aa:bb:cc:dd:ee:01")
    dev = store.get("aa:bb:cc:dd:ee:01")
    assert dev is not None
    assert dev.bearing_deg is None
