Installation
============

System Requirements
-------------------

- **Operating system**: Linux (kernel with nl80211 / cfg80211)
- **Python**: 3.11 or later
- **Architecture**: Any (x86_64, aarch64, etc.)
- **Optional (Rust MVP)**: ``rustc`` / ``cargo`` (MSRV 1.75+) for the
  Ratatui binary; ``maturin`` for the ``wifi_radar_native`` extension

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

**Not on PyPI yet.** Install from source (development or local use):

.. code-block:: bash

   git clone https://github.com/mkhellat/wifi-radar.git
   cd wifi-radar
   ./configure
   make

Or editable pip from the clone: ``pip install -e ".[dev]"``.

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

Optional Rust front-ends
------------------------

The shipping interactive app is still ``wifi-radar`` (Python curses).
During the hybrid migration you can also try:

**Ratatui list MVP** (no root needed in demo mode). Live mode prefers
nl80211 GET_SCAN dump and falls back to ``iw`` if genetlink fails:

.. code-block:: bash

   cargo run -p wifi-radar-tui -- --demo
   cargo run -p wifi-radar-tui -- -i wlan0

**PyO3 scripting module** (from ``crates/wifi-radar-py``):

.. code-block:: bash

   pip install maturin
   maturin develop --features extension-module
   python -c "import wifi_radar_native as w; print(w.version())"
   python -c "from wifi_radar import native_available; print(native_available())"

See the crate READMEs under ``crates/wifi-radar-tui/`` and
``crates/wifi-radar-py/`` for keys and API details.
