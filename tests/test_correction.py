"""Tests for scene-wide correction modes."""

from __future__ import annotations

import pytest

from wifi_radar.calib_bridge import NativeCalibrationStore, native_scene_ready
from wifi_radar.calibration import CalibrationStore
from wifi_radar.correction import (
    apply_scene_corrections,
    device_from_native_row,
    device_to_native_dict,
)
from wifi_radar.models import DeviceKind, WifiDevice
from wifi_radar.native import native


def test_honest_mode_applies_distance_scale_only(tmp_path) -> None:
    cal = CalibrationStore(path=tmp_path / "cal.json")
    cal.set_mode("honest")
    cal.set_distance_reference("aa:aa:aa:aa:aa:01", -50.0, 1.0)
    cal.set_manual_bearing("aa:aa:aa:aa:aa:01", 90.0)
    anchor = WifiDevice(
        mac="aa:aa:aa:aa:aa:01",
        kind=DeviceKind.HOTSPOT,
        rssi_dbm=-50.0,
        rssi_at_1m_override=cal.rssi_at_1m("aa:aa:aa:aa:aa:01"),
        bearing_deg=90.0,
        bearing_manual=True,
    )
    newcomer = WifiDevice(
        mac="aa:aa:aa:aa:aa:02",
        kind=DeviceKind.HOTSPOT,
        rssi_dbm=-60.0,
    )

    corrected, summary = apply_scene_corrections([anchor, newcomer], cal)
    assert summary.mode == "honest"
    assert summary.active_anchors == 1
    assert corrected[0].anchor_status == "anchor"
    assert corrected[1].distance_m() < 10.0
    assert corrected[1].bearing_override_deg is None


def test_anchor_mode_propagates_bearing_and_distance(tmp_path) -> None:
    cal = CalibrationStore(path=tmp_path / "cal.json")
    cal.set_mode("anchor")
    cal.set_distance_reference("aa:aa:aa:aa:aa:01", -50.0, 1.0)
    cal.set_manual_bearing("aa:aa:aa:aa:aa:01", 90.0)
    anchor = WifiDevice(
        mac="aa:aa:aa:aa:aa:01",
        kind=DeviceKind.HOTSPOT,
        rssi_dbm=-50.0,
        rssi_at_1m_override=cal.rssi_at_1m("aa:aa:aa:aa:aa:01"),
        bearing_deg=90.0,
        bearing_manual=True,
    )
    newcomer = WifiDevice(
        mac="aa:aa:aa:aa:aa:02",
        kind=DeviceKind.HOTSPOT,
        rssi_dbm=-60.0,
    )
    expected_bearing = newcomer.placeholder_bearing() + summary_offset(anchor)

    corrected, summary = apply_scene_corrections([anchor, newcomer], cal)
    assert summary.mode == "anchor"
    assert corrected[1].distance_m() < 10.0
    assert corrected[1].bearing_override_deg is not None
    assert round(corrected[1].display_bearing(), 6) == round(expected_bearing % 360.0, 6)


def summary_offset(anchor: WifiDevice) -> float:
    return (90.0 - anchor.placeholder_bearing() + 180.0) % 360.0 - 180.0


def test_inconsistent_anchor_is_excluded_from_scene_correction(tmp_path) -> None:
    cal = CalibrationStore(path=tmp_path / "cal.json")
    cal.set_mode("honest")
    cal.set_distance_reference("aa:aa:aa:aa:aa:01", -50.0, 1.0)
    anchor = WifiDevice(
        mac="aa:aa:aa:aa:aa:01",
        kind=DeviceKind.HOTSPOT,
        rssi_dbm=-70.0,
        rssi_at_1m_override=cal.rssi_at_1m("aa:aa:aa:aa:aa:01"),
    )
    newcomer = WifiDevice(
        mac="aa:aa:aa:aa:aa:02",
        kind=DeviceKind.HOTSPOT,
        rssi_dbm=-60.0,
    )

    corrected, summary = apply_scene_corrections([anchor, newcomer], cal)
    assert summary.active_anchors == 0
    assert corrected[0].anchor_status == "stale-cal"
    assert corrected[1].distance_override_m is None


def test_device_native_dict_roundtrip_preserves_overrides() -> None:
    dev = WifiDevice(
        mac="aa:aa:aa:aa:aa:03",
        kind=DeviceKind.HOTSPOT,
        rssi_dbm=-55.0,
        bearing_deg=12.0,
        bearing_manual=True,
        distance_override_m=4.5,
        rssi_at_1m_override=-40.0,
        correction_note="scene distance",
        anchor_status="anchor",
    )
    row = device_to_native_dict(dev)
    assert row["calibrated_bearing_deg"] == 12.0
    assert row["distance_override_m"] == 4.5
    back = device_from_native_row(
        {
            **row,
            "distance_override_m": 5.0,
            "correction_note": "scene anchor",
        },
        WifiDevice(mac=dev.mac, kind=dev.kind, rssi_dbm=dev.rssi_dbm),
    )
    assert back.distance_override_m == 5.0
    assert back.correction_note == "scene anchor"
    assert back.bearing_deg == 12.0


@pytest.mark.skipif(not native_scene_ready(), reason="wifi_radar_native scene API missing")
def test_native_scene_path_honest_scales_newcomer() -> None:
    assert native is not None
    cal = NativeCalibrationStore(native.CalibrationStore.memory())
    cal.set_mode("honest")
    cal.set_distance_reference("aa:aa:aa:aa:aa:01", -50.0, 1.0)
    cal.set_manual_bearing("aa:aa:aa:aa:aa:01", 90.0)
    anchor = WifiDevice(
        mac="aa:aa:aa:aa:aa:01",
        kind=DeviceKind.HOTSPOT,
        rssi_dbm=-50.0,
        rssi_at_1m_override=cal.rssi_at_1m("aa:aa:aa:aa:aa:01"),
        bearing_deg=90.0,
        bearing_manual=True,
    )
    newcomer = WifiDevice(
        mac="aa:aa:aa:aa:aa:02",
        kind=DeviceKind.HOTSPOT,
        rssi_dbm=-60.0,
    )
    corrected, summary = apply_scene_corrections([anchor, newcomer], cal)
    assert summary.mode == "honest"
    assert summary.active_anchors == 1
    assert corrected[0].anchor_status == "anchor"
    assert corrected[1].distance_override_m is not None
    assert corrected[1].correction_note == "scene distance"
    assert corrected[1].bearing_override_deg is None
