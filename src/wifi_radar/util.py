"""Shared utility functions."""

from __future__ import annotations

import re


def normalize_mac(mac: str) -> str:
    """Normalize a MAC address to lowercase colon-separated hex."""
    mac = mac.strip().lower().replace("-", ":")
    parts = re.split(r"[:.]", mac)
    if len(parts) == 6:
        return ":".join(p.zfill(2) for p in parts)
    return mac


def is_locally_administered(mac: str) -> bool:
    """Check if MAC has the locally-administered bit set (randomized MAC)."""
    try:
        first = int(mac.split(":")[0], 16)
        return bool(first & 0x02)
    except (ValueError, IndexError):
        return False
