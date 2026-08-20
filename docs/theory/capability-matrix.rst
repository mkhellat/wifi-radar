Capability Matrix: What Can Be Detected
========================================

Summary of what a QCA6174A WiFi adapter can and cannot detect.

Detection Matrix
----------------

.. list-table::
   :widths: 20 18 14 20 28
   :header-rows: 1

   * - Source
     - Frequency
     - Detected?
     - How?
     - Limitation
   * - WiFi APs
     - 2.4/5 GHz
     - **Yes**
     - Beacon decode, scan
     - Full identification (MAC, SSID, vendor)
   * - WiFi clients
     - 2.4/5 GHz
     - **Yes**
     - Monitor mode, probe requests
     - Full identification
   * - Bluetooth
     - 2402–2480 MHz
     - **Partial**
     - Spectral scan (energy spikes)
     - No protocol decode, no MAC
   * - Zigbee / Thread
     - 2405–2480 MHz
     - **Partial**
     - Spectral scan
     - Energy only
   * - Microwave oven
     - ~2450 MHz
     - **Partial**
     - Spectral scan (broadband)
     - Energy only
   * - Baby monitors
     - 2.4 GHz (analog)
     - **Partial**
     - Spectral scan
     - Energy only
   * - 5 GHz video TX
     - 5.8 GHz
     - **Partial**
     - Spectral scan (if tuned)
     - Energy only
   * - Cordless phones
     - 1880–1930 MHz (DECT)
     - **No**
     - Outside filter passband
     - —
   * - LTE/5G NR (most)
     - 700–2100 MHz
     - **No**
     - Outside filter passband
     - —
   * - FM radio
     - 88–108 MHz
     - **No**
     - Antenna + SAW rejection
     - —
   * - TV (DVB-T/ATSC)
     - 470–860 MHz
     - **No**
     - Antenna + SAW rejection
     - —
   * - Amateur radio
     - 144/430 MHz
     - **No**
     - Antenna + SAW rejection
     - —
   * - Power lines (EMF)
     - 50/60 Hz
     - **No**
     - λ vs antenna size
     - Fundamentally impossible
   * - Appliance fields
     - DC–kHz
     - **No**
     - Not RF; different physics
     - —

The Honest Statement
--------------------

.. pull-quote::

   A QCA6174A WiFi adapter can detect **any RF energy that falls within its
   2.4 GHz or 5 GHz passband** — including non-WiFi transmitters — as
   **unclassified power in spectral FFT bins**. It cannot detect, measure, or
   identify **anything outside these bands**. It is not an EMF meter.
