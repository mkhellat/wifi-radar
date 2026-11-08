Architecture
============

Hybrid layout
-------------

A **Rust Cargo workspace** owns the long-term domain core and primary TUI.
The **Python** package remains the shipping interactive app (curses polar
radar) until Ratatui reaches feature parity.

.. code-block:: text

   crates/
     wifi-radar-core/     Models, DeviceStore, ScanBackend trait
     wifi-radar-linux/    Interim IwCliBackend + Nl80211Backend stub
     wifi-radar-tui/      Ratatui list MVP (--demo / -i)
     wifi-radar-py/       PyO3 module wifi_radar_native (maturin)

   src/wifi_radar/        Shipping Python app (curses + iw/airodump/tshark)

**ScanBackend** exists in both stacks so nl80211 can replace the interim
``iw`` CLI shim without rewriting merge or UI callers.

**DeviceStore** semantics (EMA α=0.25, TTL 30 s, strongest-first snapshot)
are shared in intent: Rust ``wifi-radar-core`` for Ratatui/PyO3; Python
``merge.py`` for the curses app until the façade is wired end-to-end.

Python package layout
---------------------

.. code-block:: text

   src/wifi_radar/
   ├── __init__.py          Package version
   ├── __main__.py          python -m wifi_radar entry
   ├── cli.py               Argument parsing, tool checks, entry point
   ├── models.py            WifiDevice dataclass, DeviceKind enum
   ├── merge.py             DeviceStore (EMA RSSI, TTL, merge logic)
   ├── correction.py        Scene correction and anchor diagnostics
   ├── worker.py            Background scan thread
   ├── iface.py             Interface discovery, MonitorSession
   ├── oui.py               OUI vendor lookup and fetch
   ├── util.py              MAC normalisation, locally-administered check
   ├── scan/
   │   ├── __init__.py
   │   ├── base.py          ScanBackend protocol
   │   ├── iw.py            iw scan parser (state machine)
   │   ├── airodump.py      airodump-ng CSV parser
   │   └── tshark.py        tshark probe-request parser
   └── ui/
       ├── __init__.py
       ├── radar.py          Polar drawing (curses)
       └── app.py            Main loop, key handling, calibration

Data flow (shipping Python app)
-------------------------------

.. code-block:: text

   ┌─────────────────────────────────────────────────┐
   │              Worker Thread                       │
   │                                                  │
   │  iw scan ──┐                                    │
   │             ├──▶ DeviceStore (EMA + TTL)         │
   │  airodump ─┘         │                          │
   │  tshark ──────────────┘                          │
   └──────────────────────────┬──────────────────────┘
                              │ .snapshot()
                              ▼
   ┌──────────────────────────────────────────────────┐
   │              UI Thread (curses)                   │
   │                                                   │
   │  apply_scene_corrections()                        │
   │  draw_radar() ← corrected devices + selection     │
   │  key handling → heading, selection, commands      │
   └──────────────────────────────────────────────────┘

Rust MVP data flow
------------------

.. code-block:: text

   IwCliBackend / demo ──▶ DeviceStore (core) ──▶ snapshot()
                                                    │
                    ┌───────────────────────────────┼───────────────┐
                    ▼                               ▼               ▼
             wifi-radar-tui                  wifi_radar_native
             (Ratatui list)                  (PyO3 dict rows)

Key design decisions
--------------------

**UI never calls subprocesses (Python shipping app).**
  All scanning runs in the worker thread. The UI reads a snapshot of the
  device store every ~200 ms via ``store.snapshot()``.

**Ratatui MVP scans on the UI thread for now.**
  The list MVP calls ``ScanBackend::scan`` on an interval inside the event
  loop. A dedicated worker (parity with Python) lands with polar UI work.

**Selection by MAC, not list index.**
  When RSSI changes cause re-sorting, the selected device stays selected
  (Python app). The Ratatui MVP still uses a list index; MAC binding is
  planned with polar parity.

**EMA RSSI (α=0.25) replaces max-forever.**
  Devices move on the radar as signal strength changes. Stale devices expire
  after a configurable TTL (default 30 seconds).

**Scene correction is layered after snapshot (Python).**
  Per-MAC calibration is applied in ``DeviceStore.snapshot()`` first, then
  ``correction.py`` derives optional scene-wide distance/bearing adjustments
  from visible anchors based on the active calibration mode.

**Heading rotates the plot.**
  Device bearings are rendered as ``bearing - heading``, so "forward" (the
  sweep line) always points up regardless of heading rotation.

**ScanBackend protocol / trait.**
  Python ``scan/base.py`` and Rust ``wifi_radar_core::ScanBackend`` allow
  future backends (native nl80211, spectral scan) without changing merge or
  UI layers.

Threading model (Python)
------------------------

- **Main thread**: runs the curses event loop (``ui/app.py``)
- **Worker thread**: daemon thread running ``worker.py._loop()``
- **Shared state**: ``DeviceStore`` is written by the worker and read by the UI.
  The current implementation is safe because Python's GIL serialises dict
  operations, and the UI only calls ``.snapshot()`` which copies the data.

For a future native backend (Tier B/C), this may evolve to use a
``queue.Queue`` (Python) or a channel (Rust) for cleaner decoupling.
