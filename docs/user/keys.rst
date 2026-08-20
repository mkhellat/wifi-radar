Keyboard Controls
=================

wifi-radar uses vi-style single-key bindings. Arrow keys are supported as
aliases. Press ``?`` at any time for a full-screen help overlay. The overlay
now groups actions by navigation, global actions, selected-device actions,
command mode, and display status.

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
   * - ``x``
     - Release saved calibration for the selected device
   * - ``?``
     - Show full-screen help overlay

Selected Device Actions
-----------------------

These actions only make sense after selecting a device with ``j`` / ``k``.

.. list-table::
   :widths: 20 80
   :header-rows: 1

   * - Key
     - Action
   * - ``8`` / ``4`` / ``6`` / ``2``
     - Pin device ahead / left / right / behind relative to current heading
   * - ``D`` / ``d``
     - Calibrate distance at ~0.25 m / ~1 m
   * - ``x``
     - Release saved calibration for the selected device

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
   * - ``:x`` / ``:clear``
     - Release selected-device saved calibration
   * - ``:mode honest``
     - Conservative scene correction (distance only when anchors agree)
   * - ``:mode anchor``
     - Heuristic anchor propagation for both distance and bearing
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
- **Anchor tags** — ``<anchor>`` or ``<stale-cal>`` when a selected device is
  acting as a scene anchor or has been rejected as stale
- **Help header** — current calibration mode and whether a device is selected
