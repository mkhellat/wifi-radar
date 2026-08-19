"""OUI vendor lookup from IEEE CSV cache."""

from __future__ import annotations

import csv
from pathlib import Path

OUI_CACHE = Path.home() / ".cache" / "wifi_radar" / "oui.txt"
OUI_URL = "https://standards-oui.ieee.org/oui/oui.csv"


def load_oui_map(path: Path = OUI_CACHE) -> dict[str, str]:
    """Load OUI→vendor mapping from the cached IEEE CSV."""
    oui: dict[str, str] = {}
    if not path.is_file():
        return oui
    text = path.read_text(encoding="utf-8", errors="replace")
    for line in text.splitlines():
        if not line or line.startswith("Registry"):
            continue
        try:
            parts = next(csv.reader([line]))
        except StopIteration:
            continue
        if len(parts) < 3:
            continue
        assignment = parts[1].replace("-", "").upper()
        org = parts[2].strip('"')
        if len(assignment) >= 6:
            oui[assignment[:6]] = org
    return oui


def vendor_for(mac: str, oui_map: dict[str, str]) -> str:
    """Look up vendor name for a MAC address."""
    key = mac.replace(":", "").upper()[:6]
    return oui_map.get(key, "")


def fetch_oui(path: Path = OUI_CACHE) -> Path:
    """Download the IEEE OUI CSV to cache. Returns the path."""
    import urllib.request

    path.parent.mkdir(parents=True, exist_ok=True)
    with urllib.request.urlopen(OUI_URL, timeout=60) as resp:  # noqa: S310
        path.write_bytes(resp.read())
    return path
