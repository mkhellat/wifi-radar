Known Limitations
=================

Distance Accuracy
-----------------

The distance estimate uses a log-distance path-loss model with a reference
RSSI at 1 metre and an indoor path-loss exponent of 3.0. This gives
**order-of-magnitude** accuracy (within a factor of 2–3) in typical indoor
environments. It is not GPS-grade positioning.

Factors that degrade accuracy:

- Walls, furniture, and human bodies absorb and reflect signals
- Multipath causes constructive/destructive interference
- Different APs have different actual TX power (assumed 20 dBm)
- Different devices have different antenna gains

Bearing Accuracy
----------------

Without calibration, bearing is a stable pseudo-angle with no physical meaning.

With calibration:

- Omnidirectional antennas provide minimal directional information
- Indoor multipath creates false peaks
- A single radio cannot measure true angle-of-arrival without a phased array

MAC Randomisation
-----------------

Modern phones and laptops randomise their MAC address when scanning for
networks. This means:

- The same physical device may appear as multiple entries
- Vendor lookup returns "locally administered" / "random MAC"
- Device tracking across sessions is not possible for randomised MACs

Channel Coverage
----------------

- ``iw scan`` performs a managed scan that hops through all supported channels.
  This is not instantaneous; quiet devices may be missed.
- Monitor mode (``airodump-ng``) with ``--band abg`` covers both 2.4 and 5 GHz
  but brief samples may miss infrequent transmitters.
- Your own machine's traffic dominates when associated on the same channel.

Interface Conflicts
-------------------

- Creating a monitor VIF while NetworkManager manages the same PHY can cause
  conflicts on some drivers (especially Intel).
- Some drivers do not support simultaneous managed + monitor VIFs.
- If you experience issues, stop NetworkManager on the interface before running,
  or use ``--no-monitor``.

5 GHz and DFS Channels
-----------------------

- DFS (Dynamic Frequency Selection) channels (52–144) require radar detection.
  The kernel may not allow scanning on these channels without a CAC period.
- Channels marked "no IR" (no initiate radiation) cannot be used for active
  probing but are visible in passive scans.

Terminal Size
-------------

The radar display requires at least 40 columns and 10 rows. Smaller terminals
show a "Terminal too small" message.
