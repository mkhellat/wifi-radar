"""Tests for CLI argument handling."""

from __future__ import annotations

import os
import subprocess
import sys
from pathlib import Path

_PROJECT = Path(__file__).parent.parent
_SRC = str(_PROJECT / "src")


def test_version_flag() -> None:
    env = os.environ.copy()
    env["PYTHONPATH"] = _SRC
    proc = subprocess.run(
        [sys.executable, "-m", "wifi_radar", "--version"],
        capture_output=True,
        text=True,
        env=env,
    )
    assert proc.returncode == 0
    assert "0.1.0" in proc.stdout


def test_help_flag() -> None:
    env = os.environ.copy()
    env["PYTHONPATH"] = _SRC
    proc = subprocess.run(
        [sys.executable, "-m", "wifi_radar", "--help"],
        capture_output=True,
        text=True,
        env=env,
    )
    assert proc.returncode == 0
    assert "wifi-radar" in proc.stdout
