Keyboard Controls
=================

wifi-radar uses vi-style single-key bindings. Arrow keys are supported as
aliases. Press ``?`` at any time for a full-screen help overlay.

Navigation
----------

.. list-table::
   :widths: 20 80
   :header-rows: 1

   * - Key
     - Action
   * - ``h`` / ``←``
     - Rotate heading left (5°)
   * - ``l`` / ``→``
     - Rotate heading right (5°)
   * - ``H``
     - Rotate heading left fast (15°)
   * - ``L``
     - Rotate heading right fast (15°)
   * - ``j`` / ``↓``
     - Select next device (weaker signal)
   * - ``k`` / ``↑``
     - Select previous device (stronger signal)
   * - ``g``
     - Jump to first device (strongest signal)
   * - ``G``
     - Jump to last device (weakest signal)
   * - ``Esc``
     - Deselect current device

Actions
-------

.. list-table::
   :widths: 20 80
   :header-rows: 1

   * - Key
     - Action
   * - ``q``
     - Quit
   * - ``r``
     - Force immediate rescan (signals the background worker)
   * - ``m``
     - Toggle monitor-mode sampling on/off
   * - ``c``
     - Start/stop bearing calibration
   * - ``?``
     - Show full-screen help overlay

Command Mode
------------

Press ``:`` to enter command mode (vi-style). A prompt appears in the
status line. Type a command and press Enter. Press Esc to cancel.

.. list-table::
   :widths: 25 75
   :header-rows: 1

   * - Command
     - Action
   * - ``:q`` / ``:quit``
     - Quit
   * - ``:r`` / ``:rescan``
     - Force rescan
   * - ``:m`` / ``:monitor``
     - Toggle monitor mode
   * - ``:c`` / ``:calib``
     - Start/stop calibration
   * - ``:h`` / ``:help``
     - Show help overlay

Display Symbols
---------------

.. list-table::
   :widths: 15 25 60
   :header-rows: 1

   * - Glyph
     - Colour
     - Description
   * - ◉
     - Red
     - Broadcasting access point (hotspot)
   * - ◎
     - Green
     - Station associated to a nearby AP (client)
   * - ○
     - Yellow
     - Probe-only device (searching for networks, not associated)

Radar Elements
--------------

- **Concentric rings** — distance scale based on RSSI mapping:
  inner ring ≈ 3m, middle ≈ 10m, outer ≈ 30m
- **Crosshair** (─ │ ┼) — reference grid through center
- **N/E/S/W markers** — compass bearings, rotating with heading
- **Sweep line** (▲) — your forward direction (always points up)
- **Device count** — shown in the status panel when nothing is selected
- **Detail line** — MAC, kind, channel, RSSI, distance, bearing, vendor
  (visible whenever a device is selected)
