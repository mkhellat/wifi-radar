"""Tests for distance estimation."""

from __future__ import annotations

from wifi_radar.models import DeviceKind, WifiDevice


def test_strong_signal_close() -> None:
    dev = WifiDevice(mac="aa:bb:cc:dd:ee:ff", kind=DeviceKind.HOTSPOT, rssi_dbm=-30.0)
    assert dev.distance_m() < 3.0


def test_weak_signal_far() -> None:
    dev = WifiDevice(mac="aa:bb:cc:dd:ee:ff", kind=DeviceKind.HOTSPOT, rssi_dbm=-85.0)
    assert dev.distance_m() > 20.0


def test_5ghz_closer_estimate_than_2g4_same_rssi() -> None:
    """At 5 GHz path loss is steeper, so same RSSI implies shorter distance."""
    dev_2g = WifiDevice(
        mac="aa:bb:cc:dd:ee:ff", kind=DeviceKind.HOTSPOT, rssi_dbm=-60.0, freq_mhz=2437
    )
    dev_5g = WifiDevice(
        mac="aa:bb:cc:dd:ee:ff", kind=DeviceKind.HOTSPOT, rssi_dbm=-60.0, freq_mhz=5180
    )
    assert dev_5g.distance_m() < dev_2g.distance_m()


def test_client_assumed_lower_tx() -> None:
    ap = WifiDevice(mac="aa:bb:cc:dd:ee:01", kind=DeviceKind.HOTSPOT, rssi_dbm=-60.0)
    client = WifiDevice(mac="aa:bb:cc:dd:ee:02", kind=DeviceKind.CLIENT, rssi_dbm=-60.0)
    # Client has lower assumed TX → shorter estimated distance
    assert client.distance_m() < ap.distance_m()


def test_distance_clamped() -> None:
    dev = WifiDevice(mac="aa:bb:cc:dd:ee:ff", kind=DeviceKind.HOTSPOT, rssi_dbm=-100.0)
    assert dev.distance_m() <= 120.0
    dev2 = WifiDevice(mac="aa:bb:cc:dd:ee:ff", kind=DeviceKind.HOTSPOT, rssi_dbm=0.0)
    assert dev2.distance_m() >= 0.5
