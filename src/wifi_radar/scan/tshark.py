"""Parse tshark probe-request output for adapter discovery."""

from __future__ import annotations

import subprocess
import time

from wifi_radar.models import DeviceKind, WifiDevice
from wifi_radar.util import normalize_mac


def parse_tshark_probes(text: str) -> list[WifiDevice]:
    """Parse tab-separated tshark fields output into adapter devices."""
    adapters: dict[str, WifiDevice] = {}
    for line in text.splitlines():
        parts = line.split("\t")
        if not parts or not parts[0]:
            continue
        mac = normalize_mac(parts[0])
        ssid = parts[1] if len(parts) > 1 else ""
        try:
            rssi = float(parts[2]) if len(parts) > 2 and parts[2] else -85.0
        except ValueError:
            rssi = -85.0

        dev = adapters.get(mac)
        if dev is None:
            dev = WifiDevice(mac=mac, kind=DeviceKind.ADAPTER, rssi_dbm=rssi)
            adapters[mac] = dev

        if rssi > dev.rssi_dbm:
            dev.rssi_dbm = rssi
        dev.last_seen = time.time()
        if ssid and ssid not in dev.probe_ssids:
            dev.probe_ssids.append(ssid)

    return list(adapters.values())


def run_tshark_probes(mon_iface: str, seconds: float = 2.0) -> list[WifiDevice]:
    """Capture probe requests via tshark on a monitor interface."""
    proc = subprocess.run(
        [
            "tshark",
            "-i", mon_iface,
            "-a", f"duration:{int(seconds)}",
            "-Y", "wlan.fc.type_subtype == 4",
            "-T", "fields",
            "-e", "wlan.sa",
            "-e", "wlan.ssid",
            "-e", "radiotap.dbm_antsignal",
        ],
        stdin=subprocess.DEVNULL,
        capture_output=True,
        text=True,
        timeout=seconds + 10,
    )
    return parse_tshark_probes(proc.stdout)
