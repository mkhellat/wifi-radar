Keyboard Controls
=================

The radar TUI responds to the following keys:

.. list-table::
   :widths: 20 80
   :header-rows: 1

   * - Key
     - Action
   * - ``q`` / ``Esc``
     - Quit the application
   * - ``r``
     - Force an immediate rescan (both managed and monitor)
   * - ``m``
     - Toggle monitor-mode sampling on/off
   * - ``c``
     - Start or stop bearing calibration
   * - ``←`` / ``→``
     - Rotate heading (the plot rotates with you)
   * - ``↑`` / ``↓``
     - Select device in the list (by signal strength order)
   * - ``Enter``
     - Show detailed info for the selected device in the status line

Display Symbols
---------------

.. list-table::
   :widths: 10 30 60
   :header-rows: 1

   * - Glyph
     - Kind
     - Description
   * - ◉
     - Hotspot / AP
     - Broadcasting access point
   * - ◎
     - Client
     - Station associated to a nearby AP
   * - ○
     - Adapter
     - Probe-only device (searching for networks, not associated)

Device Detail
-------------

When a device is selected (via ``↑``/``↓``) and ``Enter`` is pressed, the
status line shows:

- MAC address
- Device kind (hotspot/client/adapter)
- Estimated distance (metres)
- Bearing (degrees, with calibration confidence)
- RSSI (dBm)
- Vendor name (if OUI database is loaded)
