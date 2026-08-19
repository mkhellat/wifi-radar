"""Background scan worker thread."""

from __future__ import annotations

import os
import shutil
import signal
import subprocess
import tempfile
import threading
import time
from pathlib import Path

from wifi_radar.iface import MonitorSession
from wifi_radar.merge import DeviceStore
from wifi_radar.models import WifiDevice
from wifi_radar.oui import load_oui_map, vendor_for
from wifi_radar.scan.airodump import parse_airodump_csv
from wifi_radar.scan.iw import run_iw_scan
from wifi_radar.scan.tshark import run_tshark_probes
from wifi_radar.util import is_locally_administered

SCAN_INTERVAL = 8.0
MONITOR_SAMPLE_SEC = 4.0


class ScanWorker:
    """Runs scans in a background thread, feeding a DeviceStore."""

    def __init__(
        self,
        iface: str,
        store: DeviceStore,
        use_monitor: bool = True,
    ) -> None:
        self.iface = iface
        self.store = store
        self.use_monitor = use_monitor and os.geteuid() == 0
        self._oui_map = load_oui_map()
        self._monitor = MonitorSession(iface)
        self._work_dir = Path(tempfile.mkdtemp(prefix="wifi_radar_"))
        self._stop = threading.Event()
        self._thread = threading.Thread(target=self._loop, daemon=True)
        self.status = "Starting..."
        self.mon_enabled = self.use_monitor

    def start(self) -> None:
        self._thread.start()

    def stop(self) -> None:
        self._stop.set()
        self._monitor.teardown()
        shutil.rmtree(self._work_dir, ignore_errors=True)

    def toggle_monitor(self) -> None:
        self.mon_enabled = not self.mon_enabled
        self.status = f"Monitor {'ON' if self.mon_enabled else 'OFF'}"

    def force_rescan(self) -> None:
        """Trigger an immediate scan cycle (best-effort)."""
        self._do_iw_scan()
        if self.mon_enabled:
            self._do_monitor_scan()

    def _loop(self) -> None:
        while not self._stop.is_set():
            self._do_iw_scan()
            if self.mon_enabled:
                self._do_monitor_scan()
            self._stop.wait(SCAN_INTERVAL)

    def _do_iw_scan(self) -> None:
        try:
            devices = run_iw_scan(self.iface)
            for d in devices:
                v = vendor_for(d.mac, self._oui_map)
                if v:
                    d.vendor = v
            self.store.merge(devices)
            self.status = f"Scan: {len(devices)} APs ({time.strftime('%H:%M:%S')})"
        except Exception as exc:  # noqa: BLE001
            self.status = f"Scan error: {exc}"

    def _do_monitor_scan(self) -> None:
        if not self._monitor.ensure():
            self.status = "Monitor interface failed"
            return
        try:
            aps, clients = self._run_airodump()
            adapters = self._run_tshark()
            for group in (aps, clients, adapters):
                for d in group:
                    v = vendor_for(d.mac, self._oui_map)
                    if v:
                        d.vendor = v
                    if is_locally_administered(d.mac):
                        d.vendor = (d.vendor + " (random MAC)").strip()
            self.store.merge(aps)
            self.store.merge(clients)
            self.store.merge(adapters)
            self.status = (
                f"Monitor: {len(aps)} APs, {len(clients)} clients, "
                f"{len(adapters)} adapters ({time.strftime('%H:%M:%S')})"
            )
        except Exception as exc:  # noqa: BLE001
            self.status = f"Monitor error: {exc}"

    def _run_airodump(self) -> tuple[list[WifiDevice], list[WifiDevice]]:
        prefix = self._work_dir / "dump"
        for old in self._work_dir.glob("dump-*.csv"):
            old.unlink(missing_ok=True)
        proc = subprocess.Popen(
            [
                "airodump-ng", self._monitor.mon_iface,
                "--band", "abg",
                "--write", str(prefix),
                "--output-format", "csv",
                "--write-interval", "1",
            ],
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        time.sleep(MONITOR_SAMPLE_SEC)
        proc.send_signal(signal.SIGINT)
        try:
            proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            proc.kill()
        csv_path = Path(f"{prefix}-01.csv")
        return parse_airodump_csv(csv_path)

    def _run_tshark(self) -> list[WifiDevice]:
        if not shutil.which("tshark"):
            return []
        return run_tshark_probes(self._monitor.mon_iface, seconds=2.0)
