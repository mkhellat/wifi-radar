"""Parse `iw dev <iface> scan` output for AP discovery."""

from __future__ import annotations

import re
import subprocess
from typing import TYPE_CHECKING

from wifi_radar.models import DeviceKind, WifiDevice

if TYPE_CHECKING:
    from pathlib import Path

_BSS_RE = re.compile(r"^BSS ([0-9a-f:]{17})\(on .+\)")
_FREQ_RE = re.compile(r"^\s+freq:\s+(\d+)")
_SIGNAL_RE = re.compile(r"^\s+signal:\s+([-\d.]+)\s+dBm")
_SSID_RE = re.compile(r"^\s+SSID:\s*(.*)")
_DS_CHAN_RE = re.compile(r"^\s+DS Parameter set:\s*channel\s+(\d+)")
_HT_PRIMARY_RE = re.compile(r"^\s+\*\s+primary channel:\s+(\d+)")


def _freq_to_channel(freq: int) -> int:
    if 2412 <= freq <= 2484:
        if freq == 2484:
            return 14
        return (freq - 2407) // 5
    if 5180 <= freq <= 5825:
        return (freq - 5000) // 5
    if 5955 <= freq <= 7115:
        return (freq - 5950) // 5
    return 0


def parse_iw_scan(text: str) -> list[WifiDevice]:
    """Parse the text output of `iw dev <iface> scan` into WifiDevice list."""
    devices: list[WifiDevice] = []
    current_mac: str | None = None
    freq = 0
    signal = -90.0
    ssid = ""
    channel = 0

    def _flush() -> None:
        nonlocal current_mac, freq, signal, ssid, channel
        if current_mac is None:
            return
        if channel == 0 and freq > 0:
            channel = _freq_to_channel(freq)
        devices.append(
            WifiDevice(
                mac=current_mac,
                kind=DeviceKind.HOTSPOT,
                ssid=ssid,
                channel=channel,
                freq_mhz=freq,
                rssi_dbm=signal,
            )
        )
        current_mac = None
        freq = 0
        signal = -90.0
        ssid = ""
        channel = 0

    for line in text.splitlines():
        m = _BSS_RE.match(line)
        if m:
            _flush()
            current_mac = m.group(1).lower()
            continue

        if current_mac is None:
            continue

        m = _FREQ_RE.match(line)
        if m:
            freq = int(m.group(1))
            continue

        m = _SIGNAL_RE.match(line)
        if m:
            signal = float(m.group(1))
            continue

        m = _SSID_RE.match(line)
        if m:
            ssid = m.group(1)
            continue

        m = _DS_CHAN_RE.match(line)
        if m:
            channel = int(m.group(1))
            continue

        m = _HT_PRIMARY_RE.match(line)
        if m and channel == 0:
            channel = int(m.group(1))
            continue

    _flush()
    return devices


def run_iw_scan(iface: str, timeout: float = 30.0) -> list[WifiDevice]:
    """Execute `iw dev <iface> scan` and parse results."""
    proc = subprocess.run(
        ["iw", "dev", iface, "scan"],
        capture_output=True,
        text=True,
        timeout=timeout,
    )
    if proc.returncode != 0:
        proc2 = subprocess.run(
            ["iw", "dev", iface, "scan", "dump"],
            capture_output=True,
            text=True,
            timeout=timeout,
        )
        return parse_iw_scan(proc2.stdout)
    return parse_iw_scan(proc.stdout)
