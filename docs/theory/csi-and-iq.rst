CSI and Raw IQ: Research Frontiers
===================================

Beyond spectral scan, research has pushed WiFi chips toward more exotic data
extraction. This section documents what exists and what applies to QCA6174A.

Channel State Information (CSI)
-------------------------------

CSI reports the per-subcarrier **complex channel response** (amplitude + phase)
for successfully decoded WiFi OFDM symbols. It captures how the wireless
channel distorted a known WiFi waveform — multipath, fading, Doppler.

.. list-table::
   :widths: 25 75
   :header-rows: 1

   * - Aspect
     - Detail
   * - What it measures
     - H(f) = channel transfer function at WiFi subcarrier frequencies
   * - Requires
     - A WiFi packet to be transmitted and received (either from an AP or
       injected)
   * - Chipset support
     - Intel 5300 (iwlwifi CSI tool), Atheros (ath9k patched firmware),
       some Broadcom (Nexmon)
   * - QCA6174 / ath10k
     - **Not natively supported** for CSI extraction. Would require firmware
       reverse-engineering and patching.
   * - Applications
     - Indoor localisation, gesture recognition, breathing detection,
       presence sensing
   * - Limitations
     - Still WiFi-band only; needs WiFi packets as pilot; not general EMF

Raw IQ Extraction
-----------------

Some research projects extract raw ADC samples (I + Q before OFDM demod):

- Requires **patched firmware** (e.g. Nexmon on Broadcom BCM43xx)
- Or specific debug modes not exposed in production firmware
- QCA6174 / ath10k: **not available** without reverse-engineering

Even if raw IQ were available:

- Data rate: ~160 MSPS × 12 bit × 2 channels = **~480 MB/s**
- You still only observe the bandwidth and frequency the LO is tuned to
- You gain nothing outside the WiFi band

.. admonition:: For QCA6174A specifically

   Neither CSI extraction nor raw IQ dump is available through the stock
   ``ath10k`` driver or firmware. The only "beyond 802.11 frames" capability
   that works without modification is **spectral scan** (hardware FFT bins).
