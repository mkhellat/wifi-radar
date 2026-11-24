# SPDX-FileCopyrightText: 2026 wifi-radar contributors
# SPDX-License-Identifier: GPL-3.0-or-later

"""Scan backend protocol."""

from __future__ import annotations

from typing import Protocol

from wifi_radar.models import WifiDevice


class ScanBackend(Protocol):
    """Interface that all scan backends implement."""

    def scan(self) -> list[WifiDevice]:
        """Run a scan and return discovered devices."""
        ...
