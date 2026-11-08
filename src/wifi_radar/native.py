"""Optional import of the Rust PyO3 façade (`wifi_radar_native`).

The extension is built with maturin from ``crates/wifi-radar-py``. When it is
not installed, ``native`` is ``None`` and callers should keep using the pure
Python modules.
"""

from __future__ import annotations

from typing import Any

try:
    import wifi_radar_native as native
except ImportError:  # pragma: no cover - depends on maturin install
    native = None  # type: ignore[assignment]

__all__ = ["native", "available"]


def available() -> bool:
    """True when ``wifi_radar_native`` imported successfully."""
    return native is not None


def core_version() -> str | None:
    """Rust core version string, or ``None`` if the extension is missing."""
    if native is None:
        return None
    return str(native.version())


def describe() -> dict[str, Any]:
    """Small status dict for diagnostics / docs examples."""
    return {
        "available": available(),
        "version": core_version(),
    }
