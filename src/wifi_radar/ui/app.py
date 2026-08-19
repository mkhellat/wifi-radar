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


HELP_TEXT = """\
┌─────────────────────────────────────────────────────────────┐
│                    wifi-radar help                           │
├─────────────────────────────────────────────────────────────┤
│  NAVIGATION                                                 │
│    h / ←        Rotate heading left (5°)                    │
│    l / →        Rotate heading right (5°)                   │
│    H            Rotate heading left fast (15°)              │
│    L            Rotate heading right fast (15°)             │
│    j / ↓        Select next device (weaker signal)          │
│    k / ↑        Select previous device (stronger signal)    │
│    g            Jump to first device (strongest)            │
│    G            Jump to last device (weakest)               │
│    Esc          Deselect current device                     │
│                                                             │
│  ACTIONS                                                    │
│    q            Quit                                        │
│    r            Force immediate rescan                      │
│    m            Toggle monitor mode on/off                  │
│    c            Start/stop bearing calibration              │
│    ?            Show this help screen                       │
│                                                             │
│  COMMAND MODE (vi-style)                                    │
│    :            Enter command mode                          │
│    :q :quit     Quit                                        │
│    :r :rescan   Force rescan                                │
│    :m :monitor  Toggle monitor                              │
│    :c :calib    Toggle calibration                          │
│    :h :help     Show help                                   │
│    Esc          Cancel command                              │
│                                                             │
│  DISPLAY                                                    │
│    ◉ (red)      Access point / hotspot                      │
│    ◎ (green)    Associated client station                   │
│    ○ (yellow)   Probe-only adapter (searching)              │
│    ───          Crosshair reference grid                    │
│    ∙ rings      Distance: inner ~3m, mid ~10m, outer ~30m   │
│    N/E/S/W      Compass bearings (rotate with heading)      │
│    ▲            Sweep line (your forward direction)          │
│                                                             │
│  CALIBRATION                                                │
│    Press 'c' to start. Slowly rotate your laptop.           │
│    Use h/l to set heading as you turn. After ~25s or        │
│    press 'c' again, calibration ends and devices are        │
│    placed at their peak-signal bearing.                     │
│                                                             │
├─────────────────────────────────────────────────────────────┤
│           Press any key to return to radar                  │
└─────────────────────────────────────────────────────────────┘"""


def _show_help(stdscr: curses.window) -> None:
    """Display full-screen help overlay."""
    stdscr.erase()
    max_y, max_x = stdscr.getmaxyx()
    lines = HELP_TEXT.split("\n")
    start_y = max(0, (max_y - len(lines)) // 2)
    for i, line in enumerate(lines):
        y = start_y + i
        if y >= max_y - 1:
            break
        x = max(0, (max_x - len(line)) // 2)
        try:
            stdscr.addstr(y, x, line[: max_x - 1], curses.A_BOLD)
        except curses.error:
            pass
    stdscr.refresh()
    stdscr.timeout(-1)  # block until keypress
    try:
        stdscr.get_wch()
    except curses.error:
        pass
    stdscr.timeout(200)  # restore normal timeout


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
        curses.noecho()
        curses.cbreak()
        stdscr.keypad(True)
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
                key = stdscr.get_wch()
            except curses.error:
                continue
            except KeyboardInterrupt:
                break
            # get_wch returns str for characters, int for special keys
            key_int = ord(key) if isinstance(key, str) else key

            # ── Command mode (vi-style :command) ──────────────────────
            if command_mode:
                if key_int == 27:  # Esc — cancel command
                    command_mode = False
                    command_buf = ""
                elif key_int in (10, 13, curses.KEY_ENTER):  # Enter — execute
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
                        _show_help(stdscr)
                    else:
                        worker.status = f"Unknown command: {cmd}"
                elif key_int in (curses.KEY_BACKSPACE, 127, 8):
                    command_buf = command_buf[:-1]
                    if not command_buf:
                        command_mode = False
                elif isinstance(key, str) and len(key) == 1 and key.isprintable():
                    command_buf += key
                continue

            # ── Normal mode (vi-style single keys) ────────────────────

            # Quit
            if key_int in (ord("q"), ord("Q")):
                break

            # Enter command mode
            elif key_int == ord(":"):
                command_mode = True
                command_buf = ""

            # Heading: h = left, l = right (vi motion)
            elif key_int in (ord("h"), curses.KEY_LEFT):
                heading = (heading - 5) % 360
            elif key_int in (ord("l"), curses.KEY_RIGHT):
                heading = (heading + 5) % 360
            elif key_int == ord("H"):
                heading = (heading - 15) % 360
            elif key_int == ord("L"):
                heading = (heading + 15) % 360

            # Selection: j = down, k = up (vi motion)
            elif key_int in (ord("k"), curses.KEY_UP):
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
            elif key_int in (ord("j"), curses.KEY_DOWN):
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
            elif key_int == 27:  # Esc
                selected_mac = None

            # Quick actions
            elif key_int in (ord("r"), ord("R")):
                worker.force_rescan()
            elif key_int in (ord("m"), ord("M")):
                worker.toggle_monitor()
            elif key_int in (ord("c"), ord("C")):
                if calibrator.active:
                    calibrator.stop()
                    calibrator.apply(store)
                    worker.status = "Calibration stopped"
                else:
                    calibrator.start()
                    worker.status = "CALIBRATING: rotate slowly, h/l to turn"

            # First/last device
            elif key_int == ord("g"):
                if devices:
                    selected_mac = devices[0].mac
            elif key_int == ord("G"):
                if devices:
                    selected_mac = devices[-1].mac

            # Toggle info
            elif key_int in (10, 13, curses.KEY_ENTER):
                if selected_mac:
                    selected_mac = None

            # Show help
            elif key_int == ord("?"):
                _show_help(stdscr)

    try:
        curses.wrapper(curses_main)
    finally:
        worker.stop()
    return 0
