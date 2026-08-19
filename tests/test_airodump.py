"""Tests for airodump-ng CSV parser."""

from __future__ import annotations

from pathlib import Path

from wifi_radar.models import DeviceKind
from wifi_radar.scan.airodump import parse_airodump_csv

FIXTURES = Path(__file__).parent / "fixtures"


def test_parse_aps_and_clients() -> None:
    aps, clients = parse_airodump_csv(FIXTURES / "airodump.csv")

    assert len(aps) == 2
    assert len(clients) == 2

    home = next(a for a in aps if a.ssid == "HomeNetwork")
    assert home.mac == "aa:bb:cc:dd:ee:01"
    assert home.channel == 6
    assert home.rssi_dbm == -45.0
    assert home.kind == DeviceKind.HOTSPOT

    office = next(a for a in aps if a.ssid == "Office5G")
    assert office.channel == 36

    associated = next(c for c in clients if c.associated_bssid)
    assert associated.mac == "aa:bb:cc:11:22:33"
    assert associated.associated_bssid == "aa:bb:cc:dd:ee:01"
    assert associated.kind == DeviceKind.CLIENT

    probe_only = next(c for c in clients if not c.associated_bssid)
    assert probe_only.mac == "de:ad:00:00:00:01"
    assert probe_only.kind == DeviceKind.ADAPTER
    assert "FreeWifi" in probe_only.probe_ssids
    assert "CoffeeShop" in probe_only.probe_ssids


def test_missing_file() -> None:
    aps, clients = parse_airodump_csv(Path("/nonexistent/file.csv"))
    assert aps == []
    assert clients == []
