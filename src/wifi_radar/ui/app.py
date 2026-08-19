"""Curses application loop."""

from __future__ import annotations

import curses
import time

from wifi_radar.merge import DeviceStore
from wifi_radar.models import WifiDevice
from wifi_radar.ui.radar import draw_radar
from wifi_radar.worker import ScanWorker


class BearingCalibrator:
    """Track RSSI while user rotates to estimate bearing per MAC."""

    def __init__(self) -> None:
        self.active = False
        self.started_at = 0.0
        self._samples: dict[str, list[tuple[float, float]]] = {}

    def start(self) -> None:
        self.active = True
        self.started_at = time.time()
        self._samples.clear()

    def stop(self) -> None:
        self.active = False

    def feed(self, heading: float, devices: list[WifiDevice], last_scan_time: float) -> None:
        """Record samples only when fresh scan data arrived."""
        if not self.active:
            return
        for dev in devices:
            self._samples.setdefault(dev.mac, []).append(
                (heading % 360.0, dev.rssi_dbm)
            )

    def apply(self, store: DeviceStore) -> None:
        """Set bearing for devices with enough samples."""
        for mac, points in self._samples.items():
            if len(points) < 6:
                continue
            best_bearing, _ = max(points, key=lambda p: p[1])
            dev = store.devices.get(mac)
            if dev is None:
                continue
            dev.bearing_deg = best_bearing
            spread = max(p[1] for p in points) - min(p[1] for p in points)
            dev.bearing_confidence = min(1.0, spread / 20.0)


def run_app(iface: str, use_monitor: bool) -> int:
    """Main curses application. Returns exit code."""
    store = DeviceStore()
    worker = ScanWorker(iface, store, use_monitor=use_monitor)
    calibrator = BearingCalibrator()
    heading = 0.0
    selected_mac: str | None = None

    def curses_main(stdscr: curses.window) -> None:
        nonlocal heading, selected_mac

        curses.curs_set(0)
        stdscr.keypad(True)
        stdscr.nodelay(True)
        stdscr.timeout(200)
        if curses.has_colors():
            curses.start_color()
            curses.use_default_colors()
            curses.init_pair(1, curses.COLOR_RED, -1)
            curses.init_pair(2, curses.COLOR_GREEN, -1)
            curses.init_pair(3, curses.COLOR_YELLOW, -1)

        worker.start()

        while True:
            devices = store.snapshot()

            if calibrator.active:
                calibrator.feed(heading, devices, time.time())
                if time.time() - calibrator.started_at > 25:
                    calibrator.stop()
                    calibrator.apply(store)
                    worker.status = "Calibration complete"

            draw_radar(
                stdscr, devices, heading, calibrator.active,
                worker.status, selected_mac,
            )

            try:
                key = stdscr.getch()
            except KeyboardInterrupt:
                break
            if key == -1:
                continue
            if key in (ord("q"), ord("Q"), 27):
                break
            if key in (ord("r"), ord("R")):
                worker.force_rescan()
            if key in (ord("m"), ord("M")):
                worker.toggle_monitor()
            if key in (ord("c"), ord("C")):
                if calibrator.active:
                    calibrator.stop()
                    calibrator.apply(store)
                    worker.status = "Calibration stopped"
                else:
                    calibrator.start()
                    worker.status = "Calibrating: rotate slowly, use \u2190\u2192"
            if key == curses.KEY_LEFT:
                heading = (heading - 5) % 360
            if key == curses.KEY_RIGHT:
                heading = (heading + 5) % 360
            if key == curses.KEY_UP:
                if devices:
                    if selected_mac is None:
                        selected_mac = devices[0].mac
                    else:
                        idx = next(
                            (i for i, d in enumerate(devices) if d.mac == selected_mac), 0
                        )
                        idx = max(0, idx - 1)
                        selected_mac = devices[idx].mac
            if key == curses.KEY_DOWN:
                if devices:
                    if selected_mac is None:
                        selected_mac = devices[0].mac
                    else:
                        idx = next(
                            (i for i, d in enumerate(devices) if d.mac == selected_mac), 0
                        )
                        idx = min(len(devices) - 1, idx + 1)
                        selected_mac = devices[idx].mac
            if key in (10, 13) and selected_mac:
                # Enter deselects (toggle)
                selected_mac = None

    try:
        curses.wrapper(curses_main)
    finally:
        worker.stop()
    return 0
