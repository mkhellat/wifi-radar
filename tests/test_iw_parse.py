"""Tests for iw scan parser."""

from __future__ import annotations

from pathlib import Path

from wifi_radar.models import DeviceKind
from wifi_radar.scan.iw import parse_iw_scan

FIXTURES = Path(__file__).parent / "fixtures"


def test_parse_basic_aps() -> None:
    text = (FIXTURES / "iw_scan.txt").read_text()
    devices = parse_iw_scan(text)

    assert len(devices) == 3

    home = next(d for d in devices if d.ssid == "HomeNetwork")
    assert home.mac == "aa:bb:cc:dd:ee:01"
    assert home.channel == 6
    assert home.freq_mhz == 2437
    assert home.rssi_dbm == -45.0
    assert home.kind == DeviceKind.HOTSPOT

    office = next(d for d in devices if d.ssid == "Office5G")
    assert office.mac == "11:22:33:44:55:66"
    assert office.channel == 36
    assert office.freq_mhz == 5180
    assert office.rssi_dbm == -72.0

    hidden = next(d for d in devices if d.mac == "de:ad:be:ef:ca:fe")
    assert hidden.ssid == ""
    assert hidden.channel == 0 or hidden.freq_mhz == 2412


def test_empty_input() -> None:
    assert parse_iw_scan("") == []


def test_freq_to_channel_fallback() -> None:
    text = """BSS ff:ff:ff:ff:ff:ff(on wlan0)
\tfreq: 5745
\tsignal: -60.00 dBm
\tSSID: Test5G
"""
    devices = parse_iw_scan(text)
    assert len(devices) == 1
    assert devices[0].channel == 149
