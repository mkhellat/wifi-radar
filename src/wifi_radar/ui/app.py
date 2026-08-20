"""Curses application loop with vi-style input model."""

from __future__ import annotations

import curses
import time
from typing import cast

from wifi_radar.calibration import CalibrationMode
from wifi_radar.correction import apply_scene_corrections
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
        self._last_scan_generation = -1

    def start(self) -> None:
        self.active = True
        self.started_at = time.time()
        self._samples.clear()
        self._last_scan_generation = -1

    def stop(self) -> None:
        self.active = False

    def feed(
        self,
        heading: float,
        devices: list[WifiDevice],
        scan_generation: int,
    ) -> None:
        """Record one sample per fresh iw scan (not every UI frame)."""
        if not self.active:
            return
        if scan_generation == self._last_scan_generation:
            return
        self._last_scan_generation = scan_generation
        for dev in devices:
            self._samples.setdefault(dev.mac, []).append(
                (heading % 360.0, dev.rssi_dbm)
            )

    def apply(self, store: DeviceStore) -> None:
        """Set bearing for devices with enough samples."""
        for mac, points in self._samples.items():
            if store.calibration.manual_bearing(mac) is not None:
                continue
            if len(points) < 4:
                continue
            spread = max(p[1] for p in points) - min(p[1] for p in points)
            if spread < 5.0:
                continue  # omnidirectional / too close — bearing not observable
            best_bearing, _ = max(points, key=lambda p: p[1])
            dev = store.devices.get(mac)
            if dev is None:
                continue
            dev.bearing_deg = best_bearing
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
    "x": "clear",
    "clear": "clear",
}


def _pin_bearing(
    store: DeviceStore,
    worker: ScanWorker,
    mac: str,
    heading: float,
    offset_deg: float,
) -> None:
    """Pin world bearing for a device relative to current heading."""
    bearing = (heading + offset_deg) % 360.0
    store.calibration.set_manual_bearing(mac, bearing)
    dev = store.devices.get(mac)
    label = dev.label if dev else mac
    if dev:
        dev.bearing_deg = bearing
        dev.bearing_manual = True
        dev.bearing_confidence = 1.0
    worker.status = f"Pinned {label} at {bearing:.0f}° (manual)"


def _calibrate_distance(
    store: DeviceStore,
    worker: ScanWorker,
    mac: str,
    distance_m: float,
) -> None:
    dev = store.devices.get(mac)
    if dev is None:
        return
    ref = store.calibration.set_distance_reference(mac, dev.rssi_dbm, distance_m)
    worker.status = (
        f"Calibrated {dev.label} at ~{distance_m:.2f}m "
        f"(RSSI {dev.rssi_dbm:.0f} dBm, ref {ref:.0f})"
    )


def _clear_calibration(
    store: DeviceStore,
    worker: ScanWorker,
    mac: str,
) -> None:
    """Release saved distance/bearing calibration for one MAC."""
    dev = store.devices.get(mac)
    label = dev.label if dev else mac
    store.calibration.clear(mac)
    if dev is not None:
        dev.bearing_deg = None
        dev.bearing_manual = False
        dev.bearing_confidence = 0.0
    worker.status = f"Cleared calibration for {label}"


def _build_help_lines(
    selected_mac: str | None,
    mode: CalibrationMode,
) -> list[str]:
    """Build help text with current context."""
    selected_state = "selected device: yes" if selected_mac else "selected device: no"
    mode_label = "anchor" if mode == "anchor" else "honest"
    selected_actions = [
        "  8 / 4 / 6 / 2  Pin ahead / left / right / behind",
        "  D / d          Calibrate distance at ~0.25m / ~1m",
        "  x              Release saved calibration",
    ]
    if not selected_mac:
        selected_actions = [
            "  Select a device with j / k first",
            "  Then use pin / distance / clear actions",
        ]
    return [
        "wifi-radar help",
        f"mode: {mode_label}    {selected_state}",
        "",
        "NAVIGATION",
        "  h / Left       Rotate heading left (5 deg)",
        "  l / Right      Rotate heading right (5 deg)",
        "  H / L          Rotate heading fast (15 deg)",
        "  j / Down       Select next device (weaker)",
        "  k / Up         Select previous device (stronger)",
        "  g / G          Jump to first / last device",
        "  Esc            Deselect or cancel command mode",
        "",
        "GLOBAL ACTIONS",
        "  q              Quit",
        "  r              Force immediate rescan",
        "  m              Toggle monitor mode",
        "  c              Start/stop bearing calibration",
        "  ?              Show this help screen",
        "",
        "SELECTED DEVICE",
        *selected_actions,
        "",
        "COMMAND MODE",
        "  :q   :quit     Quit",
        "  :r   :rescan   Force rescan",
        "  :m   :monitor  Toggle monitor",
        "  :c   :calib    Toggle calibration",
        "  :x   :clear    Release selected calibration",
        "  :mode honest   Conservative scene correction",
        "  :mode anchor   Heuristic anchor propagation",
        "  :h   :help     Show help overlay",
        "",
        "DISPLAY",
        "  ◉ / ◎ / ○      AP / associated client / probe-only device",
        "  rings          Auto-scaled distance reference rings",
        "  N/E/S/W        Compass markers relative to heading",
        "  ▲ sweep        Your forward direction",
        "  <anchor>       Device contributes to scene correction",
        "  <stale-cal>    Saved calibration rejected as stale",
        "",
        "CALIBRATION NOTES",
        "  Rotation calibration needs visible RSSI variation.",
        "  Manual pin and distance cal persist across runs.",
        "  Use x or :clear when a calibrated device has moved.",
        "",
        "Press any key to return to radar",
    ]


def _show_help(
    stdscr: curses.window,
    selected_mac: str | None,
    mode: CalibrationMode,
) -> None:
    """Display full-screen help overlay with dynamic box."""
    help_lines = _build_help_lines(selected_mac, mode)
    stdscr.erase()
    max_y, max_x = stdscr.getmaxyx()

    # Compute box dimensions
    content_w = max(len(line) for line in help_lines)
    box_w = min(content_w + 4, max_x - 2)  # 2 border + 2 padding
    box_h = min(len(help_lines) + 2, max_y - 1)  # 2 border
    inner_w = box_w - 4
    x0 = max(0, (max_x - box_w) // 2)
    y0 = max(0, (max_y - box_h) // 2)

    # Draw box
    top = "\u250c" + "\u2500" * (box_w - 2) + "\u2510"
    bot = "\u2514" + "\u2500" * (box_w - 2) + "\u2518"
    mid = "\u251c" + "\u2500" * (box_w - 2) + "\u2524"
    try:
        stdscr.addstr(y0, x0, top[: max_x - x0 - 1])
    except curses.error:
        pass

    for i in range(box_h - 2):
        y = y0 + 1 + i
        if y >= max_y - 1:
            break
        if i < len(help_lines):
            text = help_lines[i]
            # Title line centered, section headers bold
            if i == 0:
                pad_l = (inner_w - len(text)) // 2
                content = " " * pad_l + text
            else:
                content = " " + text
            content = content[:inner_w]
            content = content + " " * (inner_w - len(content))
            row = "\u2502 " + content + " \u2502"
        else:
            row = "\u2502" + " " * (box_w - 2) + "\u2502"
        attr = curses.A_BOLD if i == 0 else 0
        # Draw separator before FOOTER
        if i == len(help_lines) - 2 and i > 0:
            try:
                stdscr.addstr(y, x0, mid[: max_x - x0 - 1], curses.A_DIM)
            except curses.error:
                pass
            continue
        try:
            stdscr.addstr(y, x0, row[: max_x - x0 - 1], attr)
        except curses.error:
            pass

    try:
        stdscr.addstr(y0 + box_h - 1, x0, bot[: max_x - x0 - 1])
    except curses.error:
        pass

    stdscr.refresh()
    stdscr.timeout(-1)
    try:
        stdscr.get_wch()
    except curses.error:
        pass
    stdscr.timeout(200)


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
            devices, correction = apply_scene_corrections(devices, store.calibration)

            if calibrator.active:
                calibrator.feed(heading, devices, worker.scan_generation)
                if time.time() - calibrator.started_at > 25:
                    calibrator.stop()
                    calibrator.apply(store)
                    worker.status = "Calibration complete"

            # Build status string
            display_status = (
                f":{command_buf}\u2588"
                if command_mode
                else f"{worker.status}  [{correction.short_label()}]"
            )

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
                        _show_help(stdscr, selected_mac, store.calibration.mode())
                    elif action == "clear":
                        if selected_mac:
                            _clear_calibration(store, worker, selected_mac)
                        else:
                            worker.status = "Select a device first"
                    elif cmd.startswith("mode "):
                        mode = cmd.split(None, 1)[1].strip().lower()
                        if mode in {"honest", "anchor"}:
                            store.calibration.set_mode(cast(CalibrationMode, mode))
                            worker.status = f"Calibration mode: {mode}"
                        else:
                            worker.status = f"Unknown mode: {mode}"
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

            # Manual bearing / distance (requires selection)
            elif selected_mac and key_int == ord("8"):
                _pin_bearing(store, worker, selected_mac, heading, 0.0)
            elif selected_mac and key_int == ord("4"):
                _pin_bearing(store, worker, selected_mac, heading, -90.0)
            elif selected_mac and key_int == ord("6"):
                _pin_bearing(store, worker, selected_mac, heading, 90.0)
            elif selected_mac and key_int == ord("2"):
                _pin_bearing(store, worker, selected_mac, heading, 180.0)
            elif selected_mac and key_int == ord("D"):
                _calibrate_distance(store, worker, selected_mac, 0.25)
            elif selected_mac and key_int == ord("d"):
                _calibrate_distance(store, worker, selected_mac, 1.0)
            elif selected_mac and key_int in (ord("x"), ord("X")):
                _clear_calibration(store, worker, selected_mac)

            # Toggle info
            elif key_int in (10, 13, curses.KEY_ENTER):
                if selected_mac:
                    selected_mac = None

            # Show help
            elif key_int == ord("?"):
                _show_help(stdscr, selected_mac, store.calibration.mode())

    try:
        curses.wrapper(curses_main)
    finally:
        worker.stop()
    return 0
