Firmware and Driver: The Digital Wall
======================================

Even after the analog front-end, a second barrier exists: the QCA6174A's
firmware controls what data the host computer can read.

Firmware Architecture
---------------------

The QCA6174A runs its own embedded CPU with dedicated firmware that implements:

- **WiFi MAC engine**: 802.11 state machines, rate control, power management
- **PHY/Baseband DSP**: OFDM demodulation, channel estimation, AGC
- **Spectral FFT engine**: Hardware FFT for spectral scanning
- **Bluetooth core**: BLE/classic, time-division coexistence with WiFi

The firmware decides what to DMA to the host. The host does not have direct
access to the ADC output.

What the Host (Linux) Can Access
--------------------------------

.. list-table::
   :widths: 25 40 35
   :header-rows: 1

   * - Interface
     - Data
     - Protocol knowledge required
   * - ``nl80211`` scan results
     - BSS list (SSID, BSSID, signal, channel)
     - 802.11 beacons only
   * - Monitor mode frames
     - Raw 802.11 MPDUs + radiotap header (RSSI, noise, rate)
     - 802.11 preamble must be decoded by firmware
   * - ``ath10k`` spectral scan
     - Per-bin FFT power (64/128/256 bins) over current channel BW
     - **None** — raw power spectral density
   * - Noise floor calibration
     - Calibrated noise floor per chain (dBm)
     - None
   * - CCA busy time
     - % of time channel is occupied
     - Energy-detection threshold only

What Monitor Mode Does NOT Provide
-----------------------------------

Monitor mode gives you **802.11 frames that the firmware successfully decoded**.
It does not provide:

- Raw ADC / IQ samples (would require ~640 MB/s for 80 MHz × 2 × 12-bit)
- Tuning outside WiFi channels
- Demodulation of non-802.11 protocols (BLE, Zigbee, Z-Wave)
- Identification of non-WiFi transmitters

Non-WiFi in-band energy (Bluetooth, Zigbee, microwave) appears only as:

- Elevated noise floor / CCA busy percentage
- Spectral scan FFT bins (if enabled)
- Corrupted or dropped WiFi frames (indirect)

.. warning::

   The firmware **never** provides raw IQ samples, arbitrary LO tuning, or
   non-802.11 protocol decoding on stock QCA6174A hardware.
