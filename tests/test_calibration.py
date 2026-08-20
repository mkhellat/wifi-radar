"""Tests for per-device calibration."""

from __future__ import annotations

import math

from wifi_radar.calibration import CalibrationStore
from wifi_radar.merge import DeviceStore
from wifi_radar.models import DeviceKind, WifiDevice
from wifi_radar.ui.radar import distance_to_radius


def test_set_distance_reference(tmp_path) -> None:
    cal = CalibrationStore(path=tmp_path / "cal.json")
    ref = cal.set_distance_reference("aa:bb:cc:dd:ee:ff", -40.0, 0.25)
    assert cal.rssi_at_1m("aa:bb:cc:dd:ee:ff") == ref
    dev = WifiDevice(
        mac="aa:bb:cc:dd:ee:ff",
        kind=DeviceKind.HOTSPOT,
        rssi_dbm=-40.0,
        rssi_at_1m_override=ref,
    )
    assert abs(dev.distance_m() - 0.25) < 0.05


def test_manual_bearing_in_snapshot(tmp_path) -> None:
    cal = CalibrationStore(path=tmp_path / "cal.json")
    cal.set_manual_bearing("aa:bb:cc:dd:ee:ff", 270.0)
    store = DeviceStore(calibration=cal)
    store.merge([WifiDevice(mac="aa:bb:cc:dd:ee:ff", kind=DeviceKind.HOTSPOT, rssi_dbm=-50.0)])
    snap = store.snapshot()
    assert snap[0].bearing_deg == 270.0
    assert snap[0].bearing_manual is True


def test_clear_removes_saved_distance_and_bearing(tmp_path) -> None:
    cal = CalibrationStore(path=tmp_path / "cal.json")
    cal.set_distance_reference("aa:bb:cc:dd:ee:ff", -40.0, 1.0)
    cal.set_manual_bearing("aa:bb:cc:dd:ee:ff", 90.0)
    cal.clear("aa:bb:cc:dd:ee:ff")
    assert cal.rssi_at_1m("aa:bb:cc:dd:ee:ff") is None
    assert cal.manual_bearing("aa:bb:cc:dd:ee:ff") is None
    assert cal.distance_anchor_m("aa:bb:cc:dd:ee:ff") is None


def test_mode_persists(tmp_path) -> None:
    path = tmp_path / "cal.json"
    cal = CalibrationStore(path=path)
    cal.set_mode("anchor")
    reloaded = CalibrationStore(path=path)
    assert reloaded.mode() == "anchor"


def test_distance_to_radius_matches_detail_line() -> None:
    """Plot radius should track distance_m, not a separate RSSI mapping."""
    r = distance_to_radius(3.0, 10.0, 20)
    assert abs(r - 6.0) < 0.01
    assert distance_to_radius(0.0, 10.0, 20) == 0.0


def test_distance_reference_formula() -> None:
    n = 3.0
    rssi = -35.0
    d = 0.25
    ref = rssi + 10.0 * n * math.log10(d)
    dev = WifiDevice(
        mac="aa:bb:cc:dd:ee:ff",
        kind=DeviceKind.HOTSPOT,
        rssi_dbm=rssi,
        rssi_at_1m_override=ref,
    )
    assert abs(dev.distance_m() - d) < 0.01
