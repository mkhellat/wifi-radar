"""Command-line interface for wifi-radar."""

from __future__ import annotations

import argparse
import os
import shutil
import sys

from wifi_radar import __version__


def main() -> int:
    parser = argparse.ArgumentParser(
        prog="wifi-radar",
        description="Interactive WiFi radar: APs, clients, adapters on a polar TUI.",
    )
    parser.add_argument("--version", action="version", version=f"%(prog)s {__version__}")
    parser.add_argument(
        "-i", "--interface",
        default=None,
        help="WiFi interface (default: auto-detect via `iw dev`)",
    )
    parser.add_argument(
        "--no-monitor",
        action="store_true",
        help="Managed AP scan only (no airodump-ng / probe capture)",
    )
    parser.add_argument(
        "--fetch-oui",
        action="store_true",
        help="Download IEEE OUI CSV to ~/.cache/wifi_radar/oui.txt and exit",
    )
    args = parser.parse_args()

    # --fetch-oui: download and exit
    if args.fetch_oui:
        from wifi_radar.oui import fetch_oui

        path = fetch_oui()
        print(f"Saved to {path}")
        return 0

    # Tool checks (only what we actually need)
    if not shutil.which("iw"):
        print("Error: `iw` not found. Install iw (iproute2).", file=sys.stderr)
        return 1

    use_monitor = not args.no_monitor
    if use_monitor:
        missing = []
        if not shutil.which("airodump-ng"):
            missing.append("airodump-ng")
        if missing:
            print(
                f"Warning: {', '.join(missing)} not found. "
                "Monitor features disabled. Use --no-monitor to suppress.",
                file=sys.stderr,
            )
            use_monitor = False

    # Interface discovery
    iface = args.interface
    if iface is None:
        from wifi_radar.iface import default_iface

        iface = default_iface()
        if iface is None:
            print("Error: no wireless interface found. Specify with -i.", file=sys.stderr)
            return 1

    if os.geteuid() != 0 and use_monitor:
        print(
            f"Tip: run with sudo for monitor-mode scanning:\n"
            f"  sudo wifi-radar -i {iface}",
            file=sys.stderr,
        )

    from wifi_radar.ui.app import run_app

    return run_app(iface, use_monitor=use_monitor)
