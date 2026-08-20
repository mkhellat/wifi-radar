Known Limitations
=================

This page summarises practical limits. For equations, constants, and display
mapping, see :doc:`../theory/localization`.

Distance accuracy
-----------------

Distance uses a **log-distance path-loss** model with default reference RSSI
at 1 m (−30 dBm APs, −35 dBm clients), indoor exponent *n* = 3.0 (+0.3 on
5 GHz), EMA-smoothed RSSI (α = 0.25), and optional per-MAC calibration
(``D`` / ``d`` keys).

Typical indoor accuracy is **order-of-magnitude** (often within a factor of
2–3 when calibrated at a known range). It is not GPS-grade positioning.

Factors that degrade accuracy:

- Walls, furniture, and bodies absorb and reflect signals
- Multipath causes rapid RSSI swings (10–30 dB is common)
- Actual TX power and antenna gain differ per device (defaults assume “typical”)
- A single calibration point does not characterise the whole environment
- **Very close** sources break the far-field assumption; sub-metre truth from
  RSSI alone is unreliable

The radar **ring labels** follow an auto-scaled preset (10 / 30 / 100 / 300 m
max) with hysteresis so rings do not flicker. Rings describe **display
geometry**; the numeric distance in the detail line always uses the same
``distance_m()`` formula as dot placement.

Bearing accuracy
----------------

**Without calibration:** bearing is a MAC-hash placeholder — stable on screen,
no geographic meaning.

**Rotation calibration (``c``):** requires ≥ 5 dB RSSI variation across
headings; omnidirectional antennas and close sources often fail this gate.
Indoor multipath creates false peaks. A single radio cannot measure true
angle-of-arrival without a phased array.

**Manual pin (``4`` / ``6`` / ``8`` / ``2``):** the reliable way to mark
direction when you know where a device physically is. Persisted in
``~/.cache/wifi_radar/calibration.json``.

MAC randomisation
-----------------

Modern phones and laptops randomise their MAC address when scanning for
networks. This means:

- The same physical device may appear as multiple entries
- Vendor lookup returns "locally administered" / "random MAC"
- Device tracking across sessions is not possible for randomised MACs

Channel coverage
----------------

- ``iw scan`` performs a managed scan that hops through supported channels.
  This is not instantaneous; quiet devices may be missed.
- Monitor mode (``airodump-ng``) with ``--band abg`` covers both 2.4 and 5 GHz
  but brief samples may miss infrequent transmitters.
- Your own machine's traffic dominates when associated on the same channel.

Interface conflicts
-------------------

- Creating a monitor VIF while NetworkManager manages the same PHY can cause
  conflicts on some drivers (especially Intel).
- Some drivers do not support simultaneous managed + monitor VIFs.
- If you experience issues, stop NetworkManager on the interface before running,
  or use ``--no-monitor``.

5 GHz and DFS channels
----------------------

- DFS (Dynamic Frequency Selection) channels (52–144) require radar detection.
  The kernel may not allow scanning on these channels without a CAC period.
- Channels marked "no IR" (no initiate radiation) cannot be used for active
  probing but are visible in passive scans.

Terminal size
-------------

The radar display requires at least 40 columns and 10 rows. Smaller terminals
show a "Terminal too small" message.
