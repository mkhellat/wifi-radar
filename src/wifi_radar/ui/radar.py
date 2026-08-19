"""Polar radar drawing routines (curses)."""

from __future__ import annotations

import curses
import math

from wifi_radar.models import DeviceKind, WifiDevice

KIND_GLYPH = {
    DeviceKind.HOTSPOT: "\u25c9",  # ◉
    DeviceKind.CLIENT: "\u25ce",   # ◎
    DeviceKind.ADAPTER: "\u25cb",  # ○
}

KIND_COLOR = {
    DeviceKind.HOTSPOT: 1,
    DeviceKind.CLIENT: 2,
    DeviceKind.ADAPTER: 3,
}


def rssi_to_radius(rssi: float, max_radius: float) -> float:
    """Map RSSI to distance from center (stronger = closer)."""
    min_rssi, max_rssi = -95.0, -25.0
    clamped = max(min_rssi, min(max_rssi, rssi))
    t = (clamped - min_rssi) / (max_rssi - min_rssi)
    return max_radius * (1.0 - t)


def draw_radar(
    stdscr: curses.window,
    devices: list[WifiDevice],
    heading: float,
    calibrating: bool,
    status: str,
    selected_mac: str | None,
) -> None:
    """Draw the polar radar onto the curses window."""
    stdscr.erase()
    max_y, max_x = stdscr.getmaxyx()
    if max_y < 10 or max_x < 40:
        try:
            stdscr.addstr(0, 0, "Terminal too small", curses.A_BOLD)
        except curses.error:
            pass
        stdscr.refresh()
        return

    cx, cy = max_x // 2, (max_y - 6) // 2 + 1
    max_r = max(4, min(cx - 4, cy - 2, 20))

    # Distance scale: rings map RSSI to approximate metres
    # Inner ring ≈ 3m (-35 dBm), middle ≈ 10m (-60 dBm), outer ≈ 30m+ (-85 dBm)
    ring_labels = ((0.33, "~3m"), (0.66, "~10m"), (1.0, "~30m"))
    for ring_frac, label in ring_labels:
        ring = int(max_r * ring_frac)
        for deg in range(0, 360, 6):
            rad = math.radians(deg - 90)
            x = int(cx + ring * math.cos(rad))
            y = int(cy + ring * math.sin(rad) * 0.5)
            if 0 < y < max_y - 1 and 0 < x < max_x - 1:
                try:
                    stdscr.addstr(y, x, ".", curses.A_DIM)
                except curses.error:
                    pass
        # Label each ring on the right side
        lx = cx + ring + 1
        ly = cy
        if 0 < ly < max_y - 1 and lx + len(label) < max_x - 1:
            try:
                stdscr.addstr(ly, lx, label, curses.A_DIM)
            except curses.error:
                pass

    # Compass bearing markers (relative to heading)
    compass = ((0, "N"), (90, "E"), (180, "S"), (270, "W"))
    for deg, lbl in compass:
        rel = (deg - heading) % 360.0
        rad = math.radians(rel - 90)
        x = int(cx + (max_r + 2) * math.cos(rad))
        y = int(cy + (max_r + 2) * math.sin(rad) * 0.5)
        if 0 < y < max_y - 1 and 0 < x < max_x - 1:
            try:
                stdscr.addstr(y, x, lbl, curses.A_BOLD)
            except curses.error:
                pass

    # Sweep line (heading direction)
    sweep = math.radians(heading - 90)
    for step in range(1, max_r + 1):
        t = step / max_r
        x = int(cx + max_r * t * math.cos(sweep))
        y = int(cy + max_r * t * math.sin(sweep) * 0.5)
        if 0 < y < max_y - 1 and 0 < x < max_x - 1:
            try:
                ch = "^" if step == max_r else "|"
                stdscr.addstr(y, x, ch, curses.A_DIM)
            except curses.error:
                pass

    # Header
    try:
        hdr = f" Heading {heading:5.1f}\u00b0  {'CALIBRATING' if calibrating else ''}"
        stdscr.addstr(0, 2, hdr.strip(), curses.A_BOLD)
    except curses.error:
        pass

    # Plot devices (bearing rotated by heading so "forward" is up)
    for dev in devices[:30]:
        bearing = (dev.display_bearing() - heading) % 360.0
        rad = math.radians(bearing - 90)
        dist = rssi_to_radius(dev.rssi_dbm, max_r)
        x = int(cx + dist * math.cos(rad))
        y = int(cy + dist * math.sin(rad) * 0.5)
        if not (0 < y < max_y - 6 and 0 < x < max_x - 2):
            continue
        glyph = KIND_GLYPH.get(dev.kind, "?")
        color = KIND_COLOR.get(dev.kind, 0)
        attr = curses.color_pair(color)
        if selected_mac and dev.mac == selected_mac:
            attr |= curses.A_REVERSE
        try:
            stdscr.addstr(y, x, glyph, attr)
        except curses.error:
            pass

    # Legend and status panel
    leg_y = max_y - 6
    try:
        stdscr.addstr(leg_y, 2, "\u25c9 AP   \u25ce client   \u25cb probe-only", curses.A_DIM)
        stdscr.addstr(
            leg_y, 38,
            "Rings: ~3m / ~10m / ~30m   Bearing: N/E/S/W",
            curses.A_DIM,
        )
    except curses.error:
        pass
    try:
        stdscr.addstr(leg_y + 1, 2, status[: max_x - 4], curses.A_DIM)
    except curses.error:
        pass
    try:
        stdscr.addstr(
            leg_y + 2, 2,
            "[q]uit [r]escan [m]onitor [c]alib [\u2190\u2192]head [\u2191\u2193]sel [Ent]desel",
            curses.A_DIM,
        )
    except curses.error:
        pass

    # Device detail (always visible when selected)
    if selected_mac:
        sel = next((d for d in devices if d.mac == selected_mac), None)
        if sel:
            conf = (
                f"{sel.bearing_confidence * 100:.0f}%"
                if sel.bearing_deg is not None
                else "uncal"
            )
            detail = (
                f"\u2192 {sel.label}  {sel.mac}  {sel.kind.value}  ch{sel.channel or '?'}  "
                f"{sel.rssi_dbm:.0f}dBm  ~{sel.distance_m():.1f}m  "
                f"bearing {sel.display_bearing():.0f}\u00b0({conf})"
            )
            if sel.vendor:
                detail += f"  [{sel.vendor}]"
            try:
                stdscr.addstr(leg_y + 3, 2, detail[: max_x - 4], curses.A_BOLD)
            except curses.error:
                pass
    # Device count
    try:
        count_str = f"{len(devices)} device(s) visible"
        stdscr.addstr(leg_y + 4, 2, count_str, curses.A_DIM)
    except curses.error:
        pass

    stdscr.refresh()
