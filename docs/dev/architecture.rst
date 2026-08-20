Architecture
============

Package Layout
--------------

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

Data Flow
---------

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

Key Design Decisions
--------------------

**UI never calls subprocesses.**
  All scanning runs in the worker thread. The UI reads a snapshot of the
  device store every ~200 ms via ``store.snapshot()``.

**Selection by MAC, not list index.**
  When RSSI changes cause re-sorting, the selected device stays selected.

**EMA RSSI (α=0.25) replaces max-forever.**
  Devices move on the radar as signal strength changes. Stale devices expire
  after a configurable TTL (default 30 seconds).

**Scene correction is layered after snapshot.**
  Per-MAC calibration is applied in ``DeviceStore.snapshot()`` first, then
  ``correction.py`` derives optional scene-wide distance/bearing adjustments
  from visible anchors based on the active calibration mode.

**Heading rotates the plot.**
  Device bearings are rendered as ``bearing - heading``, so "forward" (the
  sweep line) always points up regardless of heading rotation.

**ScanBackend protocol.**
  The ``scan/base.py`` protocol allows future backends (native nl80211,
  spectral scan) to be added without changing the merge or UI layers.

Threading Model
---------------

- **Main thread**: runs the curses event loop (``ui/app.py``)
- **Worker thread**: daemon thread running ``worker.py._loop()``
- **Shared state**: ``DeviceStore`` is written by the worker and read by the UI.
  The current implementation is safe because Python's GIL serialises dict
  operations, and the UI only calls ``.snapshot()`` which copies the data.

For a future native backend (Tier B/C), this may evolve to use a
``queue.Queue`` for cleaner decoupling.
