Spectral Scan: Beyond 802.11 Frames
=====================================

The QCA6174A baseband includes a **hardware FFT engine** that dumps the power
spectral density of the received signal on the currently-tuned channel,
regardless of whether that energy is WiFi or not.

.. note::

   Spectral scan is **confirmed available** on the development hardware via
   ``/sys/kernel/debug/ieee80211/phy0/ath10k/spectral_scan_ctl``.

How It Works
------------

1. The interface must be associated to an AP or in monitor mode on a channel.
2. Userspace configures the scan via debugfs:

   .. code-block:: bash

      # Set number of FFT bins (64, 128, or 256)
      echo 256 > /sys/kernel/debug/ieee80211/phy0/ath10k/spectral_bins

      # Enable background mode (samples during idle time)
      echo background > /sys/kernel/debug/ieee80211/phy0/ath10k/spectral_scan_ctl
      echo trigger > /sys/kernel/debug/ieee80211/phy0/ath10k/spectral_scan_ctl

      # Read binary TLV samples
      cat /sys/kernel/debug/ieee80211/phy0/ath10k/spectral_scan0 > samples.bin

3. Firmware reports FFT samples as TLV (Type-Length-Value) binary records via
   a relayfs channel.

Data Format (Per Sample)
------------------------

Each sample is a ``struct fft_sample_ath10k`` (from ``spectral_common.h``):

.. code-block:: c

   struct fft_sample_ath10k {
       struct fft_sample_tlv tlv;   // type=3, big-endian length
       u8  chan_width_mhz;          // 20, 40, or 80
       u16 freq1;                   // centre frequency 1 (BE)
       u16 freq2;                   // centre frequency 2 (80+80)
       u16 noise;                   // noise floor (dBm, BE)
       u16 max_magnitude;           // peak bin magnitude
       u16 total_gain_db;           // total analog gain applied
       u16 base_pwr_db;             // base power reference
       u64 tsf;                     // timestamp (µs)
       s8  max_index;               // bin index of peak
       u8  rssi;                    // wideband RSSI
       u8  relpwr_db;              // peak relative to noise
       u8  avgpwr_db;              // average power across bins
       u8  max_exp;                 // exponent for magnitude scaling
       u8  data[];                  // FFT bin magnitudes
   } __packed;

Resolution
----------

.. list-table::
   :widths: 20 15 20 25
   :header-rows: 1

   * - Channel BW
     - Bins
     - Bin width
     - Observable band
   * - 20 MHz
     - 64
     - 312.5 kHz
     - 20 MHz around channel centre
   * - 20 MHz
     - 128
     - 156.25 kHz
     - 20 MHz (finer resolution)
   * - 20 MHz
     - 256
     - 78.125 kHz
     - 20 MHz (finest)
   * - 40 MHz
     - 128
     - 312.5 kHz
     - 40 MHz
   * - 80 MHz
     - 256
     - 312.5 kHz
     - 80 MHz

What Spectral Scan Can Detect
-----------------------------

Energy that **falls within the WiFi band** but is **not 802.11**:

- **Bluetooth (FHSS/GFSK)**: ~1 MHz wide spikes hopping across 2.4 GHz bins
- **Zigbee (802.15.4)**: ~2 MHz wide OQPSK at fixed channels
- **Microwave oven**: Broadband ~20 MHz smear centred near 2450 MHz
- **Analog video transmitters**: Carrier + sidebands in 2.4 or 5 GHz
- **Unknown interference**: Raw energy of unidentified origin

What Spectral Scan Cannot Do
-----------------------------

- Tune outside 2.4/5 GHz (no FM, no cellular, no ELF)
- Identify the *protocol* of non-WiFi energy (only power vs frequency)
- Provide calibrated E-field (V/m) or H-field (A/m) measurements
- Replace a spectrum analyser with continuous 0–6 GHz coverage
- Detect anything when tuned to a different channel

Visualisation Tools
-------------------

- `FFT_eval <https://github.com/simonwunderlich/FFT_eval>`_: Reference tool
  by the ath9k/ath10k spectral scan author
- `speccy <https://github.com/bcopeland/speccy>`_: Real-time Python visualiser
  for ath spectral data
