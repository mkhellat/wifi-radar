#!/usr/bin/env python3
"""
Interactive WiFi surroundings radar.

Discovers access points (hotspots), client adapters, and probe-only devices,
estimates distance from RSSI, and plots them on a terminal polar radar.

Requires: iw, ip, nmcli, airodump-ng, tshark (optional enrichment), sudo for monitor mode.
"""

from __future__ import annotations

import argparse
import curses
import csv
import hashlib
import math
import os
import re
import shutil
import signal
import subprocess
import sys
import tempfile
import time
from dataclasses import dataclass, field
from enum import Enum
from pathlib import Path
# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------

DEFAULT_IFACE = "wlan0"
MON_SUFFIX = "mon"
SCAN_AP_INTERVAL = 8.0
MONITOR_SAMPLE_SEC = 4.0
PATH_LOSS_EXPONENT = 2.7  # indoor-ish
TX_POWER_DBM = 20.0  # assumed AP/client TX for rough range
FREQ_MHZ_2G4 = 2437.0

OUI_CACHE = Path.home() / ".cache" / "wifi_radar" / "oui.txt"
OUI_URL = "https://standards-oui.ieee.org/oui/oui.csv"


class DeviceKind(Enum):
    HOTSPOT = "hotspot"       # broadcasting AP / listening AP (beacon)
    CLIENT = "client"         # associated or active client adapter
    ADAPTER = "adapter"       # probe-only / unassociated WiFi adapter


@dataclass
class WifiDevice:
    mac: str
    kind: DeviceKind
    ssid: str = ""
    vendor: str = ""
    channel: int = 0
    rssi_dbm: float = -90.0
    last_seen: float = field(default_factory=time.time)
    probe_ssids: list[str] = field(default_factory=list)
    associated_bssid: str = ""
  # calibration: bearing in degrees (0 = north / top of radar); None = uncalibrated
    bearing_deg: float | None = None
    bearing_confidence: float = 0.0  # 0..1

    @property
    def label(self) -> str:
        if self.ssid:
            return self.ssid[:18]
        if self.probe_ssids:
            return self.probe_ssids[0][:18]
        return self.mac[-8:]

    def distance_m(self, tx_power: float = TX_POWER_DBM, n: float = PATH_LOSS_EXPONENT) -> float:
        """Log-distance path loss estimate (very approximate indoors)."""
        if self.rssi_dbm >= tx_power:
            return 0.5
        exponent = (tx_power - self.rssi_dbm) / (10.0 * n)
        return min(120.0, max(0.5, 10.0 ** exponent))

    def display_bearing(self) -> float:
        if self.bearing_deg is not None:
            return self.bearing_deg % 360.0
        # Stable pseudo-bearing until user calibrates by rotating
        h = int(hashlib.md5(self.mac.encode()).hexdigest()[:8], 16)
        return float(h % 360)


# ---------------------------------------------------------------------------
# Shell helpers
# ---------------------------------------------------------------------------


def run(cmd: list[str], timeout: float = 30.0, check: bool = False) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        cmd,
        capture_output=True,
        text=True,
        timeout=timeout,
        check=check,
    )


def which(name: str) -> bool:
    return shutil.which(name) is not None


def require_tools() -> list[str]:
    missing = [t for t in ("iw", "ip", "nmcli", "airodump-ng") if not which(t)]
    return missing


def normalize_mac(mac: str) -> str:
    mac = mac.strip().lower().replace("-", ":")
    parts = re.split(r"[:.]", mac)
    if len(parts) == 6:
        return ":".join(p.zfill(2) for p in parts)
    return mac


def is_locally_administered(mac: str) -> bool:
    try:
        first = int(mac.split(":")[0], 16)
        return bool(first & 0x02)
    except (ValueError, IndexError):
        return False


# ---------------------------------------------------------------------------
# OUI vendor lookup
# ---------------------------------------------------------------------------


def load_oui_map() -> dict[str, str]:
    oui: dict[str, str] = {}
    if OUI_CACHE.is_file():
        text = OUI_CACHE.read_text(encoding="utf-8", errors="replace")
    else:
        text = ""
    for line in text.splitlines():
        if not line or line.startswith("Registry"):
            continue
        parts = list(csv.reader([line]))[0]
        if len(parts) < 3:
            continue
        assignment = parts[1].replace("-", "").upper()
        org = parts[2].strip('"')
        if len(assignment) >= 6:
            oui[assignment[:6]] = org
    return oui


def vendor_for(mac: str, oui_map: dict[str, str]) -> str:
    key = mac.replace(":", "").upper()[:6]
    return oui_map.get(key, "Unknown")


# ---------------------------------------------------------------------------
# Scanners
# ---------------------------------------------------------------------------


def scan_access_points(iface: str) -> list[WifiDevice]:
    """Managed-mode scan via NetworkManager."""
    run(["nmcli", "dev", "wifi", "rescan", "ifname", iface], timeout=15.0)
    time.sleep(1.5)
    proc = run(
        [
            "nmcli", "-t",
            "-f", "SSID,BSSID,CHAN,SIGNAL,SECURITY,IN-USE",
            "dev", "wifi", "list", "ifname", iface,
        ],
        timeout=20.0,
    )
    devices: list[WifiDevice] = []
    for line in proc.stdout.splitlines():
        parts = line.split(":")
        if len(parts) < 5:
            continue
        ssid = parts[0] if parts[0] != "--" else ""
        bssid = normalize_mac(parts[1].replace("\\", ""))
        try:
            channel = int(parts[2])
        except ValueError:
            channel = 0
        try:
            # nmcli SIGNAL is 0..100, map crudely to dBm
            quality = int(parts[3])
            rssi = -100.0 + quality * 0.55
        except ValueError:
            rssi = -75.0
        devices.append(
            WifiDevice(
                mac=bssid,
                kind=DeviceKind.HOTSPOT,
                ssid=ssid,
                channel=channel,
                rssi_dbm=rssi,
                last_seen=time.time(),
            )
        )
    return devices


def parse_airodump_csv(path: Path) -> tuple[list[WifiDevice], list[WifiDevice]]:
    """Return (APs, clients) from airodump-ng CSV."""
    aps: list[WifiDevice] = []
    clients: list[WifiDevice] = []
    if not path.is_file():
        return aps, clients
    text = path.read_text(encoding="utf-8", errors="replace")
    sections = text.split("\n\n")
    if not sections:
        return aps, clients

    ap_lines = sections[0].strip().splitlines()
    if len(ap_lines) < 2:
        return aps, clients

    for row in csv.reader(ap_lines[1:]):
        if len(row) < 14 or not row[0].strip():
            continue
        try:
            mac = normalize_mac(row[0])
            channel = int(row[3]) if row[3].strip() else 0
            power = int(row[8]) if row[8].strip() else -90
            ssid = row[13].strip()
        except (ValueError, IndexError):
            continue
        aps.append(
            WifiDevice(
                mac=mac,
                kind=DeviceKind.HOTSPOT,
                ssid=ssid,
                channel=channel,
                rssi_dbm=float(power),
            )
        )

    if len(sections) < 2:
        return aps, clients

    st_lines = sections[1].strip().splitlines()
    if len(st_lines) < 2:
        return aps, clients

    for row in csv.reader(st_lines[1:]):
        if len(row) < 6 or not row[0].strip():
            continue
        mac = normalize_mac(row[0])
        bssid = normalize_mac(row[5]) if len(row) > 5 and row[5].strip() else ""
        try:
            power = int(row[3]) if row[3].strip() else -90
        except ValueError:
            power = -90
        clients.append(
            WifiDevice(
                mac=mac,
                kind=DeviceKind.CLIENT,
                rssi_dbm=float(power),
                associated_bssid=bssid,
            )
        )
    return aps, clients


def monitor_sample(mon_iface: str, seconds: float, work_dir: Path) -> tuple[list[WifiDevice], list[WifiDevice]]:
    """Passive sample using airodump-ng."""
    prefix = work_dir / "dump"
    for old in work_dir.glob("dump-*.csv"):
        old.unlink(missing_ok=True)

    proc = subprocess.Popen(
        [
            "airodump-ng", mon_iface,
            "--band", "abg",
            "--write", str(prefix),
            "--output-format", "csv",
            "--write-interval", "1",
        ],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    time.sleep(seconds)
    proc.send_signal(signal.SIGINT)
    try:
        proc.wait(timeout=5)
    except subprocess.TimeoutExpired:
        proc.kill()

    csv_path = Path(f"{prefix}-01.csv")
    return parse_airodump_csv(csv_path)


def enrich_with_tshark_probes(mon_iface: str, seconds: float = 2.0) -> dict[str, WifiDevice]:
    """Optional: probe requests -> adapter devices with SSID hints."""
    if not which("tshark"):
        return {}
    proc = run(
        [
            "tshark", "-i", mon_iface, "-a", f"duration:{int(seconds)}",
            "-Y", "wlan.fc.type_subtype == 4",
            "-T", "fields",
            "-e", "wlan.sa", "-e", "wlan.ssid", "-e", "radiotap.dbm_antsignal",
        ],
        timeout=seconds + 5,
    )
    adapters: dict[str, WifiDevice] = {}
    for line in proc.stdout.splitlines():
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
        dev.rssi_dbm = max(dev.rssi_dbm, rssi)
        dev.last_seen = time.time()
        if ssid and ssid not in dev.probe_ssids:
            dev.probe_ssids.append(ssid)
    return adapters


# ---------------------------------------------------------------------------
# Monitor interface lifecycle
# ---------------------------------------------------------------------------


class MonitorSession:
    def __init__(self, phy_iface: str) -> None:
        self.phy_iface = phy_iface
        self.mon_iface = f"{phy_iface}{MON_SUFFIX}"

    def ensure(self) -> bool:
        if self._iface_exists(self.mon_iface):
            run(["ip", "link", "set", self.mon_iface, "up"])
            return True
        proc = run(["iw", "dev", self.phy_iface, "interface", "add", self.mon_iface, "type", "monitor"])
        if proc.returncode != 0:
            return False
        run(["ip", "link", "set", self.mon_iface, "up"])
        return True

    def teardown(self) -> None:
        if self._iface_exists(self.mon_iface):
            run(["iw", "dev", self.mon_iface, "del"])

    @staticmethod
    def _iface_exists(name: str) -> bool:
        return Path(f"/sys/class/net/{name}").exists()


# ---------------------------------------------------------------------------
# Device merge + bearing calibration
# ---------------------------------------------------------------------------


def merge_devices(target: dict[str, WifiDevice], incoming: list[WifiDevice]) -> None:
    now = time.time()
    for d in incoming:
        d.mac = normalize_mac(d.mac)
        existing = target.get(d.mac)
        if existing is None:
            d.last_seen = now
            target[d.mac] = d
            continue
        existing.last_seen = now
        existing.rssi_dbm = max(existing.rssi_dbm, d.rssi_dbm)
        if d.ssid:
            existing.ssid = d.ssid
        if d.channel:
            existing.channel = d.channel
        if d.kind == DeviceKind.HOTSPOT:
            existing.kind = DeviceKind.HOTSPOT
        elif d.kind == DeviceKind.CLIENT and existing.kind != DeviceKind.HOTSPOT:
            existing.kind = DeviceKind.CLIENT
        for s in d.probe_ssids:
            if s not in existing.probe_ssids:
                existing.probe_ssids.append(s)
        if d.associated_bssid:
            existing.associated_bssid = d.associated_bssid
        if d.bearing_deg is not None:
            existing.bearing_deg = d.bearing_deg
            existing.bearing_confidence = max(existing.bearing_confidence, d.bearing_confidence)


class BearingCalibrator:
    """Track RSSI while user rotates laptop to estimate bearing toward each MAC."""

    def __init__(self) -> None:
        self.active = False
        self.started_at = 0.0
        self.samples: dict[str, list[tuple[float, float]]] = {}  # mac -> [(bearing, rssi)]

    def start(self) -> None:
        self.active = True
        self.started_at = time.time()
        self.samples.clear()

    def stop(self) -> None:
        self.active = False

    def feed(self, heading_deg: float, devices: dict[str, WifiDevice]) -> None:
        if not self.active:
            return
        for mac, dev in devices.items():
            self.samples.setdefault(mac, []).append((heading_deg % 360.0, dev.rssi_dbm))

    def apply(self, devices: dict[str, WifiDevice]) -> None:
        for mac, points in self.samples.items():
            if len(points) < 8:
                continue
            best_bearing, best_rssi = max(points, key=lambda p: p[1])
            dev = devices.get(mac)
            if dev is None:
                continue
            dev.bearing_deg = best_bearing
            spread = max(p[1] for p in points) - min(p[1] for p in points)
            dev.bearing_confidence = min(1.0, spread / 25.0)


# ---------------------------------------------------------------------------
# Radar UI (curses)
# ---------------------------------------------------------------------------

KIND_GLYPH = {
    DeviceKind.HOTSPOT: "◉",
    DeviceKind.CLIENT: "◎",
    DeviceKind.ADAPTER: "○",
}

KIND_COLOR = {
    DeviceKind.HOTSPOT: 1,   # red
    DeviceKind.CLIENT: 2,    # green
    DeviceKind.ADAPTER: 3,   # yellow
}


def rssi_to_radius(rssi: float, max_radius: float, min_rssi: float = -95, max_rssi: float = -30) -> float:
    clamped = max(min_rssi, min(max_rssi, rssi))
    t = (clamped - min_rssi) / (max_rssi - min_rssi)
    return max_radius * (1.0 - t)  # stronger signal -> closer to center


def draw_radar(
    stdscr,
    devices: list[WifiDevice],
    heading: float,
    calibrating: bool,
    status: str,
    selected: int | None,
) -> None:
    stdscr.erase()
    max_y, max_x = stdscr.getmaxyx()
    cx, cy = max_x // 2, (max_y - 6) // 2 + 1
    max_r = min(cx - 4, cy - 2, 18)

    # rings
    for ring, label in ((max_r, "far"), (max_r * 2 // 3, "med"), (max_r // 3, "near")):
        for deg in range(0, 360, 8):
            rad = math.radians(deg - 90)
            x = int(cx + ring * math.cos(rad))
            y = int(cy + ring * math.sin(rad) * 0.55)
            if 0 < y < max_y - 1 and 0 < x < max_x - 1:
                try:
                    stdscr.addch(y, x, ".", curses.A_DIM)
                except curses.error:
                    pass

    # compass / sweep line (center → heading)
    sweep = math.radians(heading - 90)
    steps = max_r
    try:
        for step in range(1, steps + 1):
            t = step / steps
            x = int(cx + max_r * t * math.cos(sweep))
            y = int(cy + max_r * t * math.sin(sweep) * 0.55)
            if 0 < y < max_y - 1 and 0 < x < max_x - 1:
                stdscr.addch(y, x, "|" if step < steps else "^", curses.A_DIM)
        stdscr.addstr(0, 2, f"Heading {heading:5.1f}  {'CALIBRATING' if calibrating else 'sweep'}", curses.A_BOLD)
    except curses.error:
        pass

    sorted_devs = sorted(devices, key=lambda d: -d.rssi_dbm)
    for idx, dev in enumerate(sorted_devs[:24]):
        bearing = dev.display_bearing()
        rad = math.radians(bearing - 90)
        dist = rssi_to_radius(dev.rssi_dbm, max_r)
        x = int(cx + dist * math.cos(rad))
        y = int(cy + dist * math.sin(rad) * 0.55)
        if not (0 < y < max_y - 6 and 0 < x < max_x - 1):
            continue
        glyph = KIND_GLYPH.get(dev.kind, "?")
        color = KIND_COLOR.get(dev.kind, 0)
        attr = curses.color_pair(color)
        if selected is not None and sorted_devs[selected].mac == dev.mac:
            attr |= curses.A_REVERSE
        try:
            stdscr.addch(y, x, glyph, attr)
        except curses.error:
            pass

    # legend
    leg_y = max_y - 5
    try:
        stdscr.addstr(leg_y, 2, "◉ hotspot/AP   ◎ client   ○ probe-only adapter", curses.A_DIM)
        stdscr.addstr(leg_y + 1, 2, status[: max_x - 4], curses.A_DIM)
        stdscr.addstr(
            leg_y + 2, 2,
            "[q]uit [r]escan [m]onitor [c]alibrate [←→]heading [↑↓]select [Enter]detail",
            curses.A_DIM,
        )
    except curses.error:
        pass

    if selected is not None and 0 <= selected < len(sorted_devs):
        d = sorted_devs[selected]
        conf = f"{d.bearing_confidence * 100:.0f}%" if d.bearing_deg is not None else "uncalibrated"
        detail = (
            f"{d.label}  {d.mac}  {d.kind.value}  ch{d.channel or '?'}  "
            f"{d.rssi_dbm:.0f} dBm  ~{d.distance_m():.1f}m  bearing {d.display_bearing():.0f}° ({conf})"
        )
        if d.vendor:
            detail += f"  {d.vendor}"
        if d.probe_ssids:
            detail += f"  probes:{','.join(d.probe_ssids[:3])}"
        try:
            stdscr.addstr(leg_y + 3, 2, detail[: max_x - 4], curses.A_BOLD)
        except curses.error:
            pass

    stdscr.refresh()


def run_ui(
    phy_iface: str,
    use_monitor: bool,
    oui_map: dict[str, str],
) -> int:
    devices: dict[str, WifiDevice] = {}
    monitor = MonitorSession(phy_iface)
    work_dir = Path(tempfile.mkdtemp(prefix="wifi_radar_"))
    calibrator = BearingCalibrator()
    heading = 0.0
    selected: int | None = None
    last_ap_scan = 0.0
    last_mon_scan = 0.0
    status = "Starting…"
    mon_enabled = use_monitor

    def do_ap_scan() -> None:
        nonlocal status, last_ap_scan
        try:
            aps = scan_access_points(phy_iface)
            for d in aps:
                d.vendor = vendor_for(d.mac, oui_map)
            merge_devices(devices, aps)
            status = f"AP scan: {len(aps)} hotspots ({time.strftime('%H:%M:%S')})"
            last_ap_scan = time.time()
        except Exception as exc:  # noqa: BLE001
            status = f"AP scan failed: {exc}"

    def do_monitor_scan() -> None:
        nonlocal status, last_mon_scan
        if not mon_enabled:
            return
        if os.geteuid() != 0:
            status = "Monitor scan needs sudo"
            return
        if not monitor.ensure():
            status = "Could not create monitor interface"
            return
        try:
            aps, clients = monitor_sample(monitor.mon_iface, MONITOR_SAMPLE_SEC, work_dir)
            adapters = enrich_with_tshark_probes(monitor.mon_iface, seconds=2.0)
            for group in (aps, clients, list(adapters.values())):
                for d in group:
                    d.vendor = vendor_for(d.mac, oui_map)
                    if d.kind == DeviceKind.ADAPTER and is_locally_administered(d.mac):
                        d.vendor += " (randomized MAC?)"
            merge_devices(devices, aps)
            merge_devices(devices, clients)
            merge_devices(devices, list(adapters.values()))
            status = (
                f"Monitor: {len(aps)} APs, {len(clients)} clients, "
                f"{len(adapters)} adapters ({time.strftime('%H:%M:%S')})"
            )
            last_mon_scan = time.time()
        except Exception as exc:  # noqa: BLE001
            status = f"Monitor scan failed: {exc}"

    def curses_main(stdscr) -> None:
        nonlocal heading, selected, status, mon_enabled, last_ap_scan, last_mon_scan
        curses.curs_set(0)
        stdscr.nodelay(True)
        stdscr.timeout(200)
        if curses.has_colors():
            curses.start_color()
            curses.use_default_colors()
            curses.init_pair(1, curses.COLOR_RED, -1)
            curses.init_pair(2, curses.COLOR_GREEN, -1)
            curses.init_pair(3, curses.COLOR_YELLOW, -1)

        do_ap_scan()
        if mon_enabled and os.geteuid() == 0:
            do_monitor_scan()

        while True:
            now = time.time()
            if now - last_ap_scan >= SCAN_AP_INTERVAL:
                do_ap_scan()
            if mon_enabled and os.geteuid() == 0 and now - last_mon_scan >= SCAN_AP_INTERVAL + 4:
                do_monitor_scan()

            if calibrator.active:
                calibrator.feed(heading, devices)
                if now - calibrator.started_at > 25:
                    calibrator.stop()
                    calibrator.apply(devices)
                    status = "Calibration complete (peak RSSI → bearing)"

            dev_list = sorted(devices.values(), key=lambda d: -d.rssi_dbm)
            draw_radar(stdscr, dev_list, heading, calibrator.active, status, selected)

            try:
                key = stdscr.getch()
            except KeyboardInterrupt:
                break
            if key == -1:
                continue
            if key in (ord("q"), ord("Q"), 27):
                break
            if key in (ord("r"), ord("R")):
                do_ap_scan()
                if mon_enabled:
                    do_monitor_scan()
            if key in (ord("m"), ord("M")):
                mon_enabled = not mon_enabled
                status = f"Monitor mode {'ON' if mon_enabled else 'OFF'}"
            if key in (ord("c"), ord("C")):
                if calibrator.active:
                    calibrator.stop()
                    calibrator.apply(devices)
                    status = "Calibration stopped"
                else:
                    calibrator.start()
                    status = "Calibrating: rotate slowly, use ← → to set heading"
            if key == curses.KEY_LEFT:
                heading = (heading - 5) % 360
            if key == curses.KEY_RIGHT:
                heading = (heading + 5) % 360
            if key == curses.KEY_UP:
                if dev_list:
                    selected = 0 if selected is None else max(0, selected - 1)
            if key == curses.KEY_DOWN:
                if dev_list:
                    selected = 0 if selected is None else min(len(dev_list) - 1, selected + 1)
            if key in (10, 13) and selected is not None and dev_list:
                d = dev_list[selected]
                status = (
                    f"{d.mac} {d.kind.value} ~{d.distance_m():.1f}m "
                    f"@{d.display_bearing():.0f}° RSSI {d.rssi_dbm:.0f}"
                )

    try:
        return curses.wrapper(curses_main)
    finally:
        monitor.teardown()
        shutil.rmtree(work_dir, ignore_errors=True)


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Interactive WiFi radar: hotspots, clients, adapters, distance & bearing.",
    )
    parser.add_argument("-i", "--interface", default=DEFAULT_IFACE, help="WiFi interface (default: wlan0)")
    parser.add_argument(
        "--no-monitor",
        action="store_true",
        help="Managed AP scan only (no airodump-ng / probe capture)",
    )
    parser.add_argument(
        "--fetch-oui",
        action="store_true",
        help="Download IEEE OUI CSV to ~/.cache/wifi_radar/oui.txt",
    )
    args = parser.parse_args()

    missing = require_tools()
    if missing:
        print("Missing required tools:", ", ".join(missing), file=sys.stderr)
        return 1

    if args.fetch_oui:
        OUI_CACHE.parent.mkdir(parents=True, exist_ok=True)
        import urllib.request

        print(f"Fetching {OUI_URL} …")
        with urllib.request.urlopen(OUI_URL, timeout=60) as resp:  # noqa: S310
            OUI_CACHE.write_bytes(resp.read())
        print(f"Saved to {OUI_CACHE}")

    oui_map = load_oui_map()
    if os.geteuid() != 0 and not args.no_monitor:
        print(
            "Tip: run with sudo for monitor-mode client/adapter discovery:\n"
            f"  sudo {sys.argv[0]} -i {args.interface}",
            file=sys.stderr,
        )

    return run_ui(args.interface, use_monitor=not args.no_monitor, oui_map=oui_map)


if __name__ == "__main__":
    raise SystemExit(main())
