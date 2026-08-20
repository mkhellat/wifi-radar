Hardware Under Test: QCA6174A
=============================

The reference hardware for this analysis is the Qualcomm Atheros QCA6174A
wireless SoC, as found in the development machine.

Identification
--------------

.. list-table::
   :widths: 30 70
   :header-rows: 0

   * - **Chip**
     - Qualcomm Atheros QCA6174 (PCI vendor ``0x168c``, device ``0x003e``)
   * - **Revision**
     - 32
   * - **Subsystem**
     - Dell ``0x1028:0x0310``
   * - **Linux driver**
     - ``ath10k_pci``
   * - **Process node**
     - TSMC 40 nm
   * - **Package**
     - 172-ball WLNSP (4.89 × 6.02 × 0.57 mm)

Radio Specifications
--------------------

.. list-table::
   :widths: 30 70
   :header-rows: 0

   * - **WLAN standard**
     - 802.11a/b/g/n/ac, 2×2 MIMO
   * - **Bluetooth**
     - 5.0 (LE), shared antenna via time-division
   * - **Antenna config**
     - 2 antennas (TX mask ``0x3``, RX mask ``0x3``)
   * - **Band 1 (2.4 GHz)**
     - 2412–2484 MHz (channels 1–14; ch 14 disabled in most regions)
   * - **Band 2 (5 GHz)**
     - 5180–5865 MHz (channels 36–173; ch 173 disabled)
   * - **Max channel bandwidth**
     - 80 MHz (VHT80)
   * - **Interface**
     - PCIe 2.1 with L1 sub-states
   * - **Power supply**
     - 3.3 V (single rail) + 1.8 V I/O
   * - **Firmware RAM**
     - 1216 KB (WiFi) + 192 KB (BT)
   * - **Firmware ROM**
     - 448 KB (WiFi) + 672 KB (BT)

Key Design Features
-------------------

- **Integrated RF front-end**: LNA, mixers, PLL synthesizer, and 2.4 GHz PA
  are on-die. No external balun required (single-ended RF port design).
- **External 5 GHz FEM**: Many module implementations (e.g. Murata TYPE1CQ)
  add an external front-end module for the 5 GHz band (PA + LNA + T/R switch)
  for better output power and noise figure.
- **Shared antenna**: WiFi and Bluetooth share the antenna via time-division
  multiplexing controlled by the firmware coexistence engine.

.. admonition:: Implications for EMF detection

   This chip is a **narrow-band, protocol-specific** radio. It was designed to
   decode IEEE 802.11 frames on 2.4/5 GHz ISM/UNII bands. It is not a
   wideband receiver, spectrum analyser, or EMF meter.
