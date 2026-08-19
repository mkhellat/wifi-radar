"""Parse airodump-ng CSV output for APs and clients."""

from __future__ import annotations

import csv
from pathlib import Path

from wifi_radar.models import DeviceKind, WifiDevice
from wifi_radar.util import normalize_mac


def parse_airodump_csv(path: Path) -> tuple[list[WifiDevice], list[WifiDevice]]:
    """Return (APs, clients) from an airodump-ng CSV file."""
    aps: list[WifiDevice] = []
    clients: list[WifiDevice] = []
    if not path.is_file():
        return aps, clients

    text = path.read_text(encoding="utf-8", errors="replace")
    sections = text.split("\r\n\r\n")
    if len(sections) < 2:
        sections = text.split("\n\n")
    if not sections:
        return aps, clients

    # --- APs section ---
    ap_lines = sections[0].strip().splitlines()
    if len(ap_lines) >= 2:
        for row in csv.reader(ap_lines[1:]):
            if len(row) < 14 or not row[0].strip():
                continue
            try:
                mac = normalize_mac(row[0])
                channel = int(row[3].strip()) if row[3].strip() else 0
                power = int(row[8].strip()) if row[8].strip() else -90
                ssid = row[13].strip()
            except (ValueError, IndexError):
                continue
            if power == -1:
                power = -90
            aps.append(
                WifiDevice(
                    mac=mac,
                    kind=DeviceKind.HOTSPOT,
                    ssid=ssid,
                    channel=channel,
                    rssi_dbm=float(power),
                )
            )

    # --- Clients section ---
    if len(sections) >= 2:
        st_lines = sections[1].strip().splitlines()
        if len(st_lines) >= 2:
            for row in csv.reader(st_lines[1:]):
                if len(row) < 6 or not row[0].strip():
                    continue
                mac = normalize_mac(row[0])
                bssid_raw = row[5].strip() if len(row) > 5 else ""
                if "(not associated)" in bssid_raw or not bssid_raw:
                    bssid = ""
                else:
                    bssid = normalize_mac(bssid_raw)
                try:
                    power = int(row[3].strip()) if row[3].strip() else -90
                except ValueError:
                    power = -90
                if power == -1:
                    power = -90
                probes: list[str] = []
                if len(row) > 6:
                    probes = [s.strip() for s in row[6:] if s.strip()]
                kind = DeviceKind.CLIENT if bssid else DeviceKind.ADAPTER
                clients.append(
                    WifiDevice(
                        mac=mac,
                        kind=kind,
                        rssi_dbm=float(power),
                        associated_bssid=bssid,
                        probe_ssids=probes,
                    )
                )

    return aps, clients
