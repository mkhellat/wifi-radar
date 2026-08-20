Physics of Band Rejection
=========================

"Ignore" is not a software choice. It is **cascaded physical rejection** at
every analog stage of the receiver.

Quantitative Rejection by Frequency
------------------------------------

.. list-table::
   :widths: 18 15 12 15 15 15 10
   :header-rows: 1

   * - Source
     - Frequency
     - Wavelength
     - Antenna coupling
     - SAW rejection
     - After mixer+LPF
     - **Total**
   * - Power lines
     - 50/60 Hz
     - ~5000 km
     - Negligible
     - N/A
     - N/A
     - **>200 dB**
   * - AM radio
     - 530–1700 kHz
     - ~200 m
     - Negligible
     - N/A
     - N/A
     - **>150 dB**
   * - FM radio
     - 88–108 MHz
     - ~3 m
     - Poor (~0.01 λ)
     - >50 dB
     - N/A
     - **>80 dB**
   * - TV (UHF)
     - 470–860 MHz
     - ~50 cm
     - Poor match
     - >40 dB
     - >30 dB
     - **>70 dB**
   * - LTE band 3
     - 1800 MHz
     - 17 cm
     - Moderate
     - >30 dB
     - offset ~600 MHz
     - **>60 dB**
   * - LTE band 7/41
     - 2500–2690 MHz
     - 12 cm
     - Good (near 2.4G)
     - ~10–20 dB (edge)
     - Close to baseband
     - **~20–30 dB**
   * - Bluetooth
     - 2402–2480 MHz
     - 12 cm
     - Excellent (same band)
     - **Passes**
     - **Passes**
     - **0 dB**
   * - Microwave oven
     - ~2450 MHz
     - 12 cm
     - Excellent
     - **Passes**
     - **Passes**
     - **0 dB**
   * - 5 GHz WiFi
     - 5150–5825 MHz
     - ~5.5 cm
     - Excellent (band 2)
     - **Passes**
     - **Passes**
     - **0 dB**

What Each Stage Rejects
-----------------------

**Antenna (physical size vs wavelength)**

A laptop WiFi antenna is typically 2–6 cm. It is an efficient radiator only
near its resonant frequency (2.4/5 GHz). For low-frequency signals:

- At 50 Hz, the antenna is ~10⁻⁸ wavelengths long — induced voltage from
  mains E/H fields is negligible compared to a proper ELF meter.
- At FM (100 MHz), the antenna is ~0.01 λ — severely mismatched, with very
  poor coupling efficiency.

**SAW/BAW band-pass filter**

The surface acoustic wave (SAW) filter has a passband of ~84 MHz (2.4 GHz
band) or ~700 MHz (5 GHz band). Everything outside is attenuated by 30–50+ dB.
This is the **primary hardware barrier** against out-of-band signals.

**Mixer + LPF (frequency translation)**

After mixing, only energy within ±BW/2 of the local oscillator appears in the
baseband window. A signal 600 MHz away from the LO would need to pass through
a low-pass filter with ~10–40 MHz cutoff — physically impossible.

Strong Out-of-Band Transmitters
-------------------------------

A very powerful nearby transmitter (e.g. a 50 W FM antenna at 10 m) can leak
through the SAW filter's finite stopband and cause **desensitisation**:

- The LNA compresses (gain drops)
- AGC raises the noise floor
- WiFi performance degrades

This is a **failure mode**, not a detection mode. You observe the *symptom*
(elevated noise, packet loss), not a measurement of that transmitter.

.. admonition:: Key insight

   Only energy within the 2.4 GHz or 5 GHz passband reaches the ADC.
   Everything else is rejected by physics before digitisation.
