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

# Approximate distance labels for ring boundaries
RING_DEFS = (
    (0.33, "3m"),
    (0.66, "10m"),
    (1.0, "30m"),
)

ASPECT = 0.48  # terminal character aspect ratio (height/width)


def rssi_to_radius(rssi: float, max_radius: float) -> float:
    """Map RSSI to distance from center (stronger = closer)."""
    min_rssi, max_rssi = -95.0, -25.0
    clamped = max(min_rssi, min(max_rssi, rssi))
    t = (clamped - min_rssi) / (max_rssi - min_rssi)
    return max_radius * (1.0 - t)


def _draw_ring(
    stdscr: curses.window,
    cx: int, cy: int,
    radius: int,
    max_y: int, max_x: int,
    char: str = "\u2219",  # ∙ (bullet operator — small, clean)
    attr: int = 0,
) -> None:
    """Draw a smooth circle using high angular resolution."""
    # Use 2-degree steps for smooth appearance
    for deg in range(0, 360, 2):
        rad = math.radians(deg - 90)
        x = int(cx + radius * math.cos(rad))
        y = int(cy + radius * math.sin(rad) * ASPECT)
        if 0 < y < max_y - 1 and 0 < x < max_x - 1:
            try:
                stdscr.addstr(y, x, char, attr)
            except curses.error:
                pass


def _draw_crosshair(
    stdscr: curses.window,
    cx: int, cy: int,
    max_r: int,
    max_y: int, max_x: int,
) -> None:
    """Draw faint + crosshair through center."""
    for i in range(-max_r, max_r + 1):
        # Horizontal
        x = cx + i
        if 0 < cy < max_y - 1 and 0 < x < max_x - 1:
            try:
                stdscr.addstr(cy, x, "\u2500", curses.A_DIM)  # ─
            except curses.error:
                pass
        # Vertical
        y = cy + int(i * ASPECT)
        if 0 < y < max_y - 1 and 0 < cx < max_x - 1:
            try:
                stdscr.addstr(y, cx, "\u2502", curses.A_DIM)  # │
            except curses.error:
                pass
    # Center marker
    if 0 < cy < max_y - 1 and 0 < cx < max_x - 1:
        try:
            stdscr.addstr(cy, cx, "\u253c", curses.A_DIM)  # ┼
        except curses.error:
            pass


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
    if max_y < 12 or max_x < 50:
        try:
            stdscr.addstr(0, 0, "Terminal too small (need 50x12+)", curses.A_BOLD)
        except curses.error:
            pass
        stdscr.refresh()
        return

    panel_h = 6  # reserved for bottom panel
    cx = max_x // 2
    cy = (max_y - panel_h) // 2 + 1
    max_r = max(6, min(cx - 6, int((cy - 2) / ASPECT), 28))

    # Draw crosshair (faint reference grid)
    _draw_crosshair(stdscr, cx, cy, max_r, max_y, max_x)

    # Draw range rings
    for ring_frac, label in RING_DEFS:
        ring = int(max_r * ring_frac)
        _draw_ring(stdscr, cx, cy, ring, max_y, max_x, "\u2219", curses.A_DIM)
        # Place label at 45-degree angle (top-right) to avoid overlap
        lx = cx + int(ring * math.cos(math.radians(-45)))
        ly = cy + int(ring * math.sin(math.radians(-45)) * ASPECT)
        if 0 < ly < max_y - panel_h and lx + len(label) + 1 < max_x - 1:
            try:
                stdscr.addstr(ly, lx + 1, label, curses.A_DIM)
            except curses.error:
                pass

    # Compass bearing markers (relative to heading)
    compass = ((0, "N"), (90, "E"), (180, "S"), (270, "W"))
    for deg, lbl in compass:
        rel = (deg - heading) % 360.0
        rad = math.radians(rel - 90)
        x = int(cx + (max_r + 3) * math.cos(rad))
        y = int(cy + (max_r + 3) * math.sin(rad) * ASPECT)
        if 0 < y < max_y - panel_h and 0 < x < max_x - 2:
            try:
                stdscr.addstr(y, x, lbl, curses.A_BOLD)
            except curses.error:
                pass

    # Sweep line (heading direction — always "up" visually)
    sweep_rad = math.radians(-90)  # straight up
    for step in range(1, max_r + 1):
        t = step / max_r
        x = int(cx + max_r * t * math.cos(sweep_rad))
        y = int(cy + max_r * t * math.sin(sweep_rad) * ASPECT)
        if 0 < y < max_y - panel_h and 0 < x < max_x - 1:
            try:
                ch = "\u25b2" if step == max_r else "\u2502"  # ▲ or │
                stdscr.addstr(y, x, ch, curses.A_BOLD | curses.color_pair(2))
            except curses.error:
                pass

    # Header
    try:
        hdr = f" wifi-radar  hdg {heading:5.1f}\u00b0"
        if calibrating:
            hdr += "  \u25cf CALIBRATING"
        stdscr.addstr(0, 1, hdr, curses.A_BOLD)
    except curses.error:
        pass

    # Plot devices
    for dev in devices[:40]:
        bearing = (dev.display_bearing() - heading) % 360.0
        rad = math.radians(bearing - 90)
        dist = rssi_to_radius(dev.rssi_dbm, max_r)
        x = int(cx + dist * math.cos(rad))
        y = int(cy + dist * math.sin(rad) * ASPECT)
        if not (0 < y < max_y - panel_h and 1 < x < max_x - 2):
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

    # ── Bottom panel ──────────────────────────────────────────────────────
    panel_y = max_y - panel_h

    # Separator line
    try:
        stdscr.addstr(panel_y, 0, "\u2500" * (max_x - 1), curses.A_DIM)
    except curses.error:
        pass

    # Legend row
    try:
        legend = (
            " \u25c9 AP  \u25ce client  \u25cb probe    "
            "rings: ~3m ~10m ~30m    N/E/S/W = compass"
        )
        stdscr.addstr(panel_y + 1, 1, legend[: max_x - 3], curses.A_DIM)
    except curses.error:
        pass

    # Status row
    try:
        stdscr.addstr(panel_y + 2, 1, status[: max_x - 3], curses.A_DIM)
    except curses.error:
        pass

    # Key hints
    try:
        keys = "[q]uit [r]escan [m]on [c]alib [\u2190\u2192]hdg [\u2191\u2193]sel [Ret]desel"
        stdscr.addstr(panel_y + 3, 1, keys[: max_x - 3], curses.A_DIM)
    except curses.error:
        pass

    # Detail line (always visible when a device is selected)
    if selected_mac:
        sel = next((d for d in devices if d.mac == selected_mac), None)
        if sel:
            conf = (
                f"{sel.bearing_confidence * 100:.0f}%"
                if sel.bearing_deg is not None
                else "uncal"
            )
            detail = (
                f" {sel.mac}  {sel.kind.value}  "
                f"ch{sel.channel or '?'}  "
                f"{sel.rssi_dbm:.0f}dBm  ~{sel.distance_m():.1f}m  "
                f"brg {sel.display_bearing():.0f}\u00b0({conf})"
            )
            if sel.vendor:
                detail += f"  [{sel.vendor}]"
            if sel.label:
                detail = f" {sel.label} |{detail}"
            try:
                stdscr.addstr(
                    panel_y + 4, 1, detail[: max_x - 3],
                    curses.A_BOLD | curses.color_pair(2),
                )
            except curses.error:
                pass
    else:
        try:
            cnt = f" {len(devices)} device(s)"
            stdscr.addstr(panel_y + 4, 1, cnt, curses.A_DIM)
        except curses.error:
            pass

    stdscr.refresh()
