Bearing Calibration
===================

By default, devices are placed at a **pseudo-random but stable angle** derived
from a hash of their MAC address. This provides a consistent layout but does
not reflect true physical direction.

To determine actual bearing, wifi-radar supports **rotation calibration**.

How It Works
------------

1. Press ``c`` to start calibration mode (status bar shows "CALIBRATING").
2. **Slowly rotate your laptop** (or the antenna) through a full circle.
3. Use ``←`` / ``→`` to set your current heading as you rotate.
4. The tool records RSSI for each device at each heading angle.
5. After ~25 seconds (or press ``c`` again to stop early), calibration ends.
6. Each device is placed at the heading where its signal was strongest.

The **bearing confidence** (shown as a percentage in the detail line) reflects
how much RSSI variation was observed during rotation:

- High confidence: strong directional pattern (>20 dB spread)
- Low confidence: omnidirectional or too few samples

Requirements for Good Results
-----------------------------

- **Directional antenna** gives the best results (USB dongle with external
  antenna, or a laptop with asymmetric antenna placement).
- **Rotate slowly**: the tool samples every ~200 ms but scans update every
  ~8 seconds. Fast rotation means stale RSSI at many angles.
- **Stable environment**: moving people, doors opening, etc. add noise.
- **Multiple devices**: calibration applies to all visible devices
  simultaneously.

Limitations
-----------

- A single omnidirectional antenna provides little directional information.
  The RSSI variation comes mainly from the laptop body shadowing the antenna.
- Indoor multipath severely distorts bearing estimates. Results are best in
  open areas or near windows.
- Calibration applies per-session. It is not saved to disk.

Without Calibration
-------------------

When uncalibrated, the detail line shows "uncal" instead of a confidence
percentage. The pseudo-angle from MAC hash ensures:

- Devices do not overlap on the display
- The same device always appears at the same angle between scans
- The layout is deterministic (restart shows the same positions)
