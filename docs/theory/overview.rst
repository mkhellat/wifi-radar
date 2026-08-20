RF Capability Overview
======================

**Subject:** Can a stock WiFi adapter detect general EMF, EM signals, and
arbitrary RF transmitters?

**Hardware under test:** Qualcomm Atheros QCA6174A (rev 32), 2×2 MIMO
802.11a/b/g/n/ac + Bluetooth 5, PCIe 2.1, driven by ``ath10k_pci`` on Linux.

**Date:** 2026-08-19

Executive Summary
-----------------

A WiFi adapter is a **narrow-band, protocol-specific receiver**. It is not a
general-purpose EMF meter, spectrum analyzer, or arbitrary-transmitter
detector. Its analog front-end physically rejects signals outside the 2.4 GHz
and 5 GHz ISM/UNII bands. Its firmware only decodes 802.11 frames.

The QCA6174A also supports **spectral scan** — a hardware FFT feature that
reports per-bin power across the currently tuned channel bandwidth (up to
80 MHz). That allows detection of **in-band non-WiFi energy** (Bluetooth,
Zigbee, microwave leakage, video transmitters) as unclassified interference,
without identifying the source protocol.

**Bottom line:** the adapter is a **WiFi-band activity sensor**, not an
"all EM" detector. Extending it to detect FM, cellular, power-line fields, or
sub-GHz transmitters is **physically impossible** without different hardware.

Reading Order
-------------

1. :doc:`hardware` — chip identification and radio limits
2. :doc:`receiver` — direct-conversion architecture and signal chain
3. :doc:`band-rejection` — why out-of-band energy never reaches the ADC
4. :doc:`firmware` — what Linux can and cannot read
5. :doc:`spectral-scan` — the Tier B window beyond 802.11 frames
6. :doc:`csi-and-iq` — research frontiers that still stay in-band
7. :doc:`capability-matrix` — detection matrix
8. :doc:`tiers` — honest A–D capability tiers for this project

Localization geometry (distance / bearing / display math) lives in
:doc:`localization`.

References
----------

1. Qualcomm. *QCA6174A Product Brief* (87-YB799-1-C). 2019.
2. Linux Wireless. *ath10k Spectral Scan*.
   https://wireless.docs.kernel.org/en/latest/en/users/drivers/ath10k/spectral.html
3. Kernel source. ``drivers/net/wireless/ath/spectral_common.h`` — FFT sample TLV format.
4. Murata. *TYPE1CQ Module Datasheet* (QCA6174A reference design with diplexer + SAW + FEM).
5. LiteOn. *QCA6174A-3 Wireless Module User Manual*.
6. RF Essentials. *Direct Conversion (Zero-IF) Receiver*.
7. S. Wunderlich. *FFT_eval* — Atheros spectral scan visualization.
   https://github.com/simonwunderlich/FFT_eval
8. B. Copeland. *speccy* — Real-time spectrum visualizer for ath9k/ath10k.
   https://github.com/bcopeland/speccy
9. Wireless Pi. *Direct Conversion (Zero-IF) Receiver*.
10. Microwave Journal. *On the Direct Conversion Receiver — A Tutorial*.
