Honest Capability Tiers
========================

The project follows a staged approach, with each tier requiring progressively
different hardware.

Tier Definitions
----------------

.. list-table::
   :widths: 8 20 25 22 25
   :header-rows: 1

   * - Tier
     - Name
     - Hardware
     - Observable
     - Identifies source?
   * - **A**
     - WiFi Radar
     - QCA6174 (stock driver)
     - WiFi APs, clients, probes
     - Yes (MAC, SSID, vendor)
   * - **B**
     - ISM Activity Monitor
     - QCA6174 (spectral scan)
     - All 2.4/5 GHz energy
     - No (power vs freq only)
   * - **C**
     - Wideband RF Scanner
     - RTL-SDR / HackRF
     - 24 MHz – 6 GHz continuous
     - With software decoders
   * - **D**
     - EMF / ELF Meter
     - Dedicated field sensor
     - 50/60 Hz E and H fields
     - N/A (field strength)

Tier Progression for wifi-radar
-------------------------------

**v1 (Tier A)** — Current implementation. Detect and plot 802.11 devices using
``iw scan`` and optional monitor mode.

**v1.x (Tier B)** — Add spectral scan view. Channel-hop + per-bin FFT power
plot. Flag non-WiFi energy as "interference" without claiming source identity.
Achievable on the **same QCA6174A** with no hardware changes.

**v2+ (Tier C)** — Optional SDR backend (RTL-SDR, HackRF). Different hardware,
different dependencies. WiFi adapter becomes optional sidecar.

**Never (Tier D)** — ELF/VLF EMF metering requires a fundamentally different
transducer (E-field plates, H-field loop). Not addressable by any WiFi or
SDR hardware.

Hardware Boundaries
-------------------

.. warning::

   Using WiFi hardware "as-is" for Tier C or D is **not credible**:

   - The SAW filters physically block everything outside 2.4/5 GHz
   - The firmware provides no raw ADC access
   - The PLL cannot tune below ~2.4 GHz or above ~5.8 GHz
   - CSI/IQ research is chip-specific, patched-firmware, and still WiFi-band

   A product claiming the same WiFi dongle is a general EMF/broadcast/cellular
   detector would be making false claims.

Legal and Ethics Note
---------------------

Passive WiFi monitoring where you are authorised to observe is well-understood.
Marketing a tool as "detect all EM transmitters" oversells capability and can
blur into signals-intelligence territory in some jurisdictions. Any scope change
must match **what the hardware can actually prove**.
