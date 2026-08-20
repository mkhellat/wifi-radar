Calibration
===========

wifi-radar supports three related mechanisms:

1. **MAC-hash bearing** (default) — stable display layout, no physical meaning
2. **Rotation calibration** (``c``) — RSSI vs heading during a slow turn
3. **Manual pin + distance reference** (selected device) — persisted to disk

Full formulas and constants are in :doc:`../theory/localization`.

Default layout (no calibration)
-------------------------------

Without calibration, each device is placed at a **pseudo-random but stable
angle** derived from a hash of its MAC address. The detail line shows
``uncal`` for bearing confidence.

This keeps the polar view readable (no overlap) and deterministic across
restarts, but **does not reflect true direction**.

Rotation bearing calibration
----------------------------

Press ``c`` to start calibration (status bar shows ``CALIBRATING``).

Workflow
~~~~~~~~

1. **Slowly rotate** your laptop or external antenna through as much of a
   full circle as practical.
2. Set your current **heading** with ``h`` / ``l`` (or ``←`` / ``→``) as you
   turn — the sweep line always points “forward”.
3. The calibrator records one ``(heading, RSSI)`` pair **per fresh background
   scan** (not every UI frame), so rotation should be slow enough that scans
   catch new angles.
4. After **25 seconds**, or when you press ``c`` again, calibration stops.
5. For each device (except manually pinned MACs), if enough variation was
   seen, bearing is set to the heading where RSSI was **strongest**.

Quality gates
~~~~~~~~~~~~~

Rotation results are applied only when:

- At least **4** samples were collected for that MAC
- RSSI **spread** (max − min) is at least **5 dB**

Otherwise the device keeps its previous bearing (often still MAC-hash).
Omnidirectional sources and very close devices (e.g. a phone on the desk)
often fail the 5 dB gate — use manual pin instead.

**Bearing confidence** (detail line, as a percentage) is
``min(100%, spread / 20 dB × 100%)`` for rotation-calibrated devices.

Rotation calibration is **session-only**; it is not written to disk.

Manual bearing pin (persisted)
------------------------------

Select a device (``j`` / ``k``), then pin its direction **relative to your
current heading**:

.. list-table::
   :widths: 15 25 60
   :header-rows: 1

   * - Key
     - Offset
     - Meaning
   * - ``8``
     - 0°
     - Device is **ahead**
   * - ``6``
     - +90°
     - Device is to your **right**
   * - ``4``
     - −90°
     - Device is to your **left**
   * - ``2``
     - 180°
     - Device is **behind**

World bearing = ``(current_heading + offset) mod 360°``.

The detail line shows ``manual`` instead of a confidence percentage.
Manual bearings are saved and survive restarts; rotation calibration will
not overwrite them.

Distance reference (persisted)
------------------------------

With a device selected:

.. list-table::
   :widths: 15 85
   :header-rows: 1

   * - Key
     - Action
   * - ``D``
     - Calibrate so **current RSSI** maps to **0.25 m**
   * - ``d``
     - Calibrate so **current RSSI** maps to **1 m**

Use when you know the physical distance (e.g. AP on the desk beside you).
This adjusts the per-MAC ``RSSI at 1 m`` reference used by ``distance_m()``.

Calibration file
----------------

Both distance references and manual bearings are stored in:

``~/.cache/wifi_radar/calibration.json``

Fields:

- ``rssi_at_1m`` — MAC → reference dBm at 1 m
- ``manual_bearing`` — MAC → world bearing in degrees

Tips for good results
---------------------

- **Close or omnidirectional sources:** prefer **manual pin** and **D** / ``d``
  over rotation calibration.
- **Directional antenna** (USB dongle with external antenna) improves rotation
  calibration more than a flat laptop lid.
- **Rotate slowly** and keep the environment stable (fewer people/doors moving).
- **Multiple devices** are calibrated together during rotation mode.

See :doc:`limitations` for accuracy expectations.
