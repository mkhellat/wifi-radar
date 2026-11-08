"""Optional wifi_radar_native import helpers."""

from __future__ import annotations

from wifi_radar import native, native_available
from wifi_radar.native import available, core_version, describe


def test_describe_keys() -> None:
    info = describe()
    assert set(info) >= {"available", "version"}
    assert info["available"] is available()
    assert available() is native_available()


def test_native_reexport_consistent() -> None:
    if available():
        assert native is not None
        assert core_version() == native.version()
    else:
        assert native is None
        assert core_version() is None
