"""Optional import of the Rust PyO3 façade (`wifi_radar_native`).

The extension is built with maturin from ``crates/wifi-radar-py``. When it is
not installed, ``native`` is ``None`` and callers should keep using the pure
Python modules.
"""

from __future__ import annotations

from types import ModuleType
from typing import Any

native: ModuleType | None
try:
    import wifi_radar_native as _native

    native = _native
except ImportError:  # pragma: no cover - depends on maturin install
    native = None

__all__ = ["native", "available"]


def available() -> bool:
    """True when ``wifi_radar_native`` imported successfully."""
    return native is not None


def core_version() -> str | None:
    """Rust core version string, or ``None`` if the extension is missing."""
    if native is None:
        return None
    version = getattr(native, "version", None)
    if version is None:
        return None
    return str(version())


def describe() -> dict[str, Any]:
    """Small status dict for diagnostics / docs examples."""
    from wifi_radar.calib_bridge import native_calibration_ready

    return {
        "available": available(),
        "version": core_version(),
        "calibration_store": native_calibration_ready(),
    }
