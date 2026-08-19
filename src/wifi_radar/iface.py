"""Wireless interface discovery and monitor mode lifecycle."""

from __future__ import annotations

import re
import subprocess
from pathlib import Path


def list_wireless_ifaces() -> list[str]:
    """Return wireless interface names via `iw dev`."""
    proc = subprocess.run(
        ["iw", "dev"],
        capture_output=True,
        text=True,
        timeout=5,
    )
    return re.findall(r"Interface\s+(\S+)", proc.stdout)


def default_iface() -> str | None:
    """Return the first available wireless interface, or None."""
    ifaces = list_wireless_ifaces()
    return ifaces[0] if ifaces else None


def iface_exists(name: str) -> bool:
    return Path(f"/sys/class/net/{name}").exists()


class MonitorSession:
    """Create and destroy a monitor-mode virtual interface."""

    MON_SUFFIX = "mon"

    def __init__(self, phy_iface: str) -> None:
        self.phy_iface = phy_iface
        self.mon_iface = f"{phy_iface}{self.MON_SUFFIX}"

    def ensure(self) -> bool:
        """Create monitor VIF if needed. Returns True on success."""
        if iface_exists(self.mon_iface):
            subprocess.run(
                ["ip", "link", "set", self.mon_iface, "up"],
                capture_output=True, timeout=5,
            )
            return True
        proc = subprocess.run(
            ["iw", "dev", self.phy_iface, "interface", "add",
             self.mon_iface, "type", "monitor"],
            capture_output=True, timeout=5,
        )
        if proc.returncode != 0:
            return False
        subprocess.run(
            ["ip", "link", "set", self.mon_iface, "up"],
            capture_output=True, timeout=5,
        )
        return True

    def teardown(self) -> None:
        """Remove the monitor VIF if it exists."""
        if iface_exists(self.mon_iface):
            subprocess.run(
                ["iw", "dev", self.mon_iface, "del"],
                capture_output=True, timeout=5,
            )
