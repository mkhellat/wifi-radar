"""Curses application loop with vi-style input model."""

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


# vi-style commands executed from command mode
COMMANDS: dict[str, str] = {
    "q": "quit",
    "quit": "quit",
    "r": "rescan",
    "rescan": "rescan",
    "m": "monitor",
    "monitor": "monitor",
    "c": "calib",
    "calib": "calib",
    "calibrate": "calib",
    "h": "help",
    "help": "help",
}


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

        command_mode = False
        command_buf = ""

        while True:
            devices = store.snapshot()

            if calibrator.active:
                calibrator.feed(heading, devices, time.time())
                if time.time() - calibrator.started_at > 25:
                    calibrator.stop()
                    calibrator.apply(store)
                    worker.status = "Calibration complete"

            # Build status string
            display_status = f":{command_buf}\u2588" if command_mode else worker.status

            draw_radar(
                stdscr, devices, heading, calibrator.active,
                display_status, selected_mac,
            )

            try:
                key = stdscr.getch()
            except KeyboardInterrupt:
                break
            if key == -1:
                continue

            # ── Command mode (vi-style :command) ──────────────────────
            if command_mode:
                if key == 27:  # Esc — cancel command
                    command_mode = False
                    command_buf = ""
                elif key in (10, 13, curses.KEY_ENTER):  # Enter — execute
                    command_mode = False
                    cmd = command_buf.strip().lower()
                    command_buf = ""
                    action = COMMANDS.get(cmd, "")
                    if action == "quit":
                        return
                    elif action == "rescan":
                        worker.force_rescan()
                    elif action == "monitor":
                        worker.toggle_monitor()
                    elif action == "calib":
                        if calibrator.active:
                            calibrator.stop()
                            calibrator.apply(store)
                            worker.status = "Calibration stopped"
                        else:
                            calibrator.start()
                            worker.status = "CALIBRATING: rotate slowly, h/l to turn"
                    elif action == "help":
                        worker.status = (
                            "Normal: h/l=hdg j/k=sel q=quit | "
                            "Cmd(:): q r m c help"
                        )
                    else:
                        worker.status = f"Unknown command: {cmd}"
                elif key in (curses.KEY_BACKSPACE, 127, 8):
                    command_buf = command_buf[:-1]
                    if not command_buf:
                        command_mode = False
                elif 32 <= key < 127:
                    command_buf += chr(key)
                continue

            # ── Normal mode (vi-style single keys) ────────────────────

            # Quit
            if key in (ord("q"), ord("Q")):
                break

            # Enter command mode
            if key == ord(":"):
                command_mode = True
                command_buf = ""
                continue

            # Heading: h = left, l = right (vi motion)
            if key in (ord("h"), curses.KEY_LEFT):
                heading = (heading - 5) % 360
            elif key in (ord("l"), curses.KEY_RIGHT):
                heading = (heading + 5) % 360
            elif key in (ord("H"),):
                heading = (heading - 15) % 360
            elif key in (ord("L"),):
                heading = (heading + 15) % 360

            # Selection: j = down, k = up (vi motion)
            elif key in (ord("k"), curses.KEY_UP):
                if devices:
                    if selected_mac is None:
                        selected_mac = devices[0].mac
                    else:
                        idx = next(
                            (i for i, d in enumerate(devices)
                             if d.mac == selected_mac), 0
                        )
                        idx = max(0, idx - 1)
                        selected_mac = devices[idx].mac
            elif key in (ord("j"), curses.KEY_DOWN):
                if devices:
                    if selected_mac is None:
                        selected_mac = devices[-1].mac
                    else:
                        idx = next(
                            (i for i, d in enumerate(devices)
                             if d.mac == selected_mac), 0
                        )
                        idx = min(len(devices) - 1, idx + 1)
                        selected_mac = devices[idx].mac

            # Deselect
            elif key == 27:  # Esc
                selected_mac = None

            # Quick actions
            elif key in (ord("r"), ord("R")):
                worker.force_rescan()
            elif key in (ord("m"), ord("M")):
                worker.toggle_monitor()
            elif key in (ord("c"), ord("C")):
                if calibrator.active:
                    calibrator.stop()
                    calibrator.apply(store)
                    worker.status = "Calibration stopped"
                else:
                    calibrator.start()
                    worker.status = "CALIBRATING: rotate slowly, h/l to turn"

            # First/last device
            elif key in (ord("g"),):
                if devices:
                    selected_mac = devices[0].mac
            elif key in (ord("G"),):
                if devices:
                    selected_mac = devices[-1].mac

            # Toggle info
            elif key in (10, 13, curses.KEY_ENTER):
                if selected_mac:
                    selected_mac = None

            # Show help
            elif key == ord("?"):
                worker.status = (
                    "h/l=hdg H/L=fast j/k=sel g/G=top/bot "
                    "q=quit r=rescan m=mon c=cal :=cmd ?=help"
                )

    try:
        curses.wrapper(curses_main)
    finally:
        worker.stop()
    return 0
