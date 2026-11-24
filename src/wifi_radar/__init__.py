# SPDX-FileCopyrightText: 2026 wifi-radar contributors
# SPDX-License-Identifier: GPL-3.0-or-later

"""Interactive WiFi surroundings radar (Linux / nl80211)."""

from __future__ import annotations

from wifi_radar.native import available as native_available
from wifi_radar.native import native

__version__ = "0.1.0"
__all__ = ["__version__", "native", "native_available"]
