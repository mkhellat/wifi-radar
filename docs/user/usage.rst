Usage
=====

Basic Usage
-----------

.. code-block:: bash

   # Full scan with auto-detected interface (requires root for monitor mode)
   sudo wifi-radar

   # Specify interface explicitly
   sudo wifi-radar -i wlan0

   # AP scan only — no monitor mode, no root needed
   wifi-radar --no-monitor

   # Download OUI vendor database and exit
   wifi-radar --fetch-oui

Command-Line Options
--------------------

.. code-block:: text

   usage: wifi-radar [-h] [--version] [-i INTERFACE] [--no-monitor] [--fetch-oui]

   Interactive WiFi radar: APs, clients, adapters on a polar TUI.

   options:
     -h, --help            show this help message and exit
     --version             show program's version number and exit
     -i INTERFACE, --interface INTERFACE
                           WiFi interface (default: auto-detect via `iw dev`)
     --no-monitor          Managed AP scan only (no airodump-ng / probe capture)
     --fetch-oui           Download IEEE OUI CSV to ~/.cache/wifi_radar/oui.txt and exit

Interface Auto-Detection
------------------------

If ``-i`` is not specified, wifi-radar queries ``iw dev`` and uses the first
wireless interface found. If no wireless interface exists, it exits with an
error.

Monitor Mode
------------

When run as root (or via ``sudo``), wifi-radar creates a monitor-mode virtual
interface (``<iface>mon``) for passive observation of:

- Client stations associated to nearby APs
- Probe requests from devices searching for networks
- Additional AP beacons not visible via managed scan

If ``airodump-ng`` is not installed, monitor features are automatically
disabled with a warning. Use ``--no-monitor`` to suppress the warning.

.. note::

   Monitor mode and managed mode can conflict on some drivers. If you
   experience issues, use ``--no-monitor`` or stop NetworkManager on the
   interface before running.
