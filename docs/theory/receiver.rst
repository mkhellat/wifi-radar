Receiver Architecture: Direct Conversion
=========================================

The QCA6174A uses a **direct-conversion** (homodyne / zero-IF) receiver
architecture, which is standard in modern WiFi and cellular SoCs.

How Direct Conversion Works
---------------------------

The RF signal is mixed directly to baseband (0 Hz centre) in a single step,
without an intermediate frequency (IF) stage. This simplifies the design
(fewer filters, no image-rejection stage) at the cost of requiring careful
DC-offset and I/Q-imbalance calibration in the digital domain.

Signal Chain (One Receive Chain)
--------------------------------

.. code-block:: text

   Antenna → [Diplexer] → [SAW BPF] → [LNA] → [Splitter]
                                                    │
                                          ┌─────────┴─────────┐
                                          │                   │
                                     [Mixer I]           [Mixer Q]
                                       cos(ωt)           sin(ωt)
                                          │                   │
                                          ↓                   ↓
                                      [LPF I]            [LPF Q]
                                          │                   │
                                          ↓                   ↓
                                      [ADC I]            [ADC Q]
                                          │                   │
                                          └─────────┬─────────┘
                                                    ↓
                                         [Digital Baseband/MAC]
                                                    │
                                                    ↓ PCIe 2.1
                                              Host (Linux)

Component Roles
---------------

.. list-table::
   :widths: 15 35 50
   :header-rows: 1

   * - Stage
     - Function
     - Impact on "EMF detection"
   * - **Antenna**
     - Dual-band PIFA/PCB trace, ~2–6 cm. Resonant at 2.4 and 5 GHz.
     - At 50/60 Hz (λ ≈ 5000 km) this is an electrically invisible stub.
       At FM (λ ≈ 3 m) it is severely mismatched. Only GHz signals couple.
   * - **Diplexer**
     - Splits 2.4 GHz and 5 GHz paths from the shared antenna port.
     - Routes energy to the correct band's filter chain. Out-of-band energy
       is terminated or reflected.
   * - **SAW BPF**
     - Band-definition filter. Passes 2400–2484 MHz or 5150–5850 MHz.
       Rejects everything else by 30–50+ dB.
     - **Primary physics barrier.** FM, LTE, TV are in the stopband.
   * - **LNA**
     - Low-noise amplifier, ~2 dB noise figure, ~15–20 dB gain.
     - Amplifies only what passed the BPF.
   * - **Splitter**
     - Divides signal into I and Q paths.
     - Passive; no frequency selectivity added.
   * - **Mixers (I/Q)**
     - Multiply RF by LO (cos for I, sin for Q). LO = channel centre freq.
     - Only energy within ±BW/2 of f_LO appears at baseband.
   * - **LPF**
     - Baseband low-pass, cuts off at ~10/20/40 MHz (20/40/80 MHz BW).
     - Defines instantaneous observable bandwidth.
   * - **ADC**
     - Digitises I and Q basebands. 10–12 bit, ~160–320 MSPS.
     - Dynamic range sized for WiFi signals (−90 to −20 dBm).
   * - **Digital BB**
     - OFDM demod, rate detection, MAC framing, AGC, spectral FFT engine.
     - Decodes 802.11 frames. Can optionally dump raw FFT bins.

Frequency Synthesis (Local Oscillator)
--------------------------------------

The LO is a **fractional-N PLL** locked to an on-board crystal reference.
It is programmable across WiFi channels only:

- Band 1: 2412–2484 MHz in 5 MHz steps (channels 1–14)
- Band 2: 5180–5825 MHz in 5 MHz steps (channels 36–165)

.. warning::

   The PLL **cannot** be tuned to 100 MHz (FM), 900 MHz (LTE band 8), or
   50 Hz (mains). The VCO and divider chain are designed for the WiFi range
   only. Even if firmware allowed arbitrary tuning, the analog VCO would not
   lock outside its designed range.
