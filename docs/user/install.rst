Installation
============

System Requirements
-------------------

- **Operating system**: Linux (kernel with nl80211 / cfg80211)
- **Python**: 3.11 or later
- **Architecture**: Any (x86_64, aarch64, etc.)

System Tools
------------

The following must be available on ``$PATH``:

.. list-table::
   :widths: 20 15 65
   :header-rows: 1

   * - Tool
     - Required?
     - Purpose
   * - ``iw``
     - Always
     - AP scanning, interface discovery, monitor VIF creation
   * - ``ip``
     - For monitor
     - Bring up the monitor virtual interface
   * - ``airodump-ng``
     - Optional
     - Passive client/probe capture (part of aircrack-ng suite)
   * - ``tshark``
     - Optional
     - Probe-request enrichment (part of Wireshark)

On Arch Linux:

.. code-block:: bash

   sudo pacman -S iw iproute2 aircrack-ng wireshark-cli

On Debian/Ubuntu:

.. code-block:: bash

   sudo apt install iw iproute2 aircrack-ng tshark

Install wifi-radar
------------------

**From PyPI** (once published):

.. code-block:: bash

   pip install wifi-radar

**From source** (development):

.. code-block:: bash

   git clone https://github.com/mkhellat/wifi-radar.git
   cd wifi-radar
   ./configure
   make

This follows the traditional GNU/FOSS workflow: ``./configure`` detects the
platform and generates ``Makefile`` from the committed ``Makefile.in`` template.

**Verify installation**:

.. code-block:: bash

   .venv/bin/wifi-radar --version

OUI Vendor Database
-------------------

For vendor name display, download the IEEE OUI database:

.. code-block:: bash

   .venv/bin/wifi-radar --fetch-oui

This downloads ``oui.csv`` from IEEE and caches it at
``~/.cache/wifi_radar/oui.txt``. Run periodically to update.
