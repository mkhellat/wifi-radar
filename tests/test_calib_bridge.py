"""Native / Python calibration store bridge."""

from __future__ import annotations

from wifi_radar.calib_bridge import (
    NativeCalibrationStore,
    make_calibration_store,
    native_calibration_ready,
    native_scene_ready,
)
from wifi_radar.calibration import CalibrationStore
from wifi_radar.merge import DeviceStore
from wifi_radar.native import describe


def test_make_store_memory_duck_types() -> None:
    store = make_calibration_store(memory=True)
    store.set_manual_bearing("aa:bb:cc:dd:ee:01", 90.0)
    assert store.manual_bearing("AA:BB:CC:DD:EE:01") == 90.0
    store.set_mode("anchor")
    assert store.mode() == "anchor"
    store.clear("aa:bb:cc:dd:ee:01")
    assert store.manual_bearing("aa:bb:cc:dd:ee:01") is None


def test_device_store_default_calibration_backend() -> None:
    ds = DeviceStore()
    assert isinstance(ds.calibration, (CalibrationStore, NativeCalibrationStore))


def test_describe_includes_calibration_flag() -> None:
    info = describe()
    assert "calibration_store" in info
    assert info["calibration_store"] is native_calibration_ready()
    assert "scene_corrections" in info
    assert info["scene_corrections"] is native_scene_ready()
