Localization: Distance, Bearing, and Display Geometry
=======================================================

wifi-radar maps each observed device onto a **polar display**: estimated
**range** from RSSI and **bearing** from calibration or a MAC-hash
placeholder. This page documents the exact models, constants, smoothing,
and display mapping used in the implementation (``models.py``, ``merge.py``,
``calibration.py``, ``ui/radar.py``, ``ui/app.py``).

.. note::

   These are **heuristic** estimates for situational awareness, not
   survey-grade positioning. Indoor multipath, unknown TX power, and
   omnidirectional antennas dominate the error budget.

Distance from RSSI
------------------

Log-distance path loss
~~~~~~~~~~~~~~~~~~~~~~

Received power in dBm is modelled with a reference level at 1 m and a
path-loss exponent *n*:

.. math::

   \mathrm{RSSI}(d) = \mathrm{RSSI}_{1\mathrm{m}} - 10\,n\,\log_{10}(d)

Solving for distance *d* (metres):

.. math::

   d = 10^{\,\bigl(\mathrm{RSSI}_{1\mathrm{m}} - \mathrm{RSSI}\bigr) / (10\,n)}

Implementation (``WifiDevice.distance_m``):

.. math::

   d = \mathrm{clamp}\bigl(0.1,\; 120,\; 10^{(\mathrm{ref} - \mathrm{RSSI}) / (10 n)}\bigr)

Default constants
~~~~~~~~~~~~~~~~~

.. list-table::
   :header-rows: 1
   :widths: 30 15 55

   * - Symbol / field
     - Value
     - Meaning
   * - ``RSSI_AT_1M_AP``
     - −30 dBm
     - Reference at 1 m for access points (hotspots)
   * - ``RSSI_AT_1M_CLIENT``
     - −35 dBm
     - Reference at 1 m for clients / adapters
   * - ``PATH_LOSS_EXPONENT_INDOOR`` (*n*)
     - 3.0
     - Indoor log-distance exponent
   * - 5 GHz adjustment
     - *n* ← *n* + 0.3
     - Applied when ``freq_mhz`` > 4000 (faster wall attenuation)
   * - Range clamp
     - [0.1, 120] m
     - Display and numerical stability bounds

The default ``RSSI_{1m}`` values assume typical consumer hardware at
moderate TX power; they are **not** measured per device unless the user
calibrates (see below).

Per-MAC distance calibration
~~~~~~~~~~~~~~~~~~~~~~~~~~~~

When the user presses ``D`` (0.25 m) or ``d`` (1 m) with a device selected,
the tool stores a per-MAC override ``rssi_at_1m`` so that the **current
smoothed RSSI** maps to the chosen distance. From the path-loss equation at
the calibration distance :math:`d_\mathrm{cal}`:

.. math::

   \mathrm{RSSI}_{1\mathrm{m}} =
   \mathrm{RSSI} + 10\,n\,\log_{10}(d_\mathrm{cal})

(``CalibrationStore.set_distance_reference`` uses :math:`d_\mathrm{cal} \geq 0.1` m.)

Overrides are persisted in ``~/.cache/wifi_radar/calibration.json`` under
``rssi_at_1m``. The selected calibration distance is also stored as
``distance_anchor_m`` for scene-correction modes. On each
``DeviceStore.snapshot()``, a matching override is copied to
``WifiDevice.rssi_at_1m_override`` and used as ``ref`` in the distance formula.

RSSI smoothing (EMA)
--------------------

Raw scan RSSI values fluctuate with fading and traffic. The device store
merges repeated sightings with an exponential moving average (``merge.py``):

.. math::

   \mathrm{RSSI}_\mathrm{new} =
   \alpha\,\mathrm{RSSI}_\mathrm{scan} +
   (1 - \alpha)\,\mathrm{RSSI}_\mathrm{stored}

with :math:`\alpha = 0.25` (``RSSI_EMA_ALPHA``). Lower :math:`\alpha`
smooths more aggressively; 0.25 was chosen to reduce close-range jitter
without lagging too far behind real movement.

Devices not seen for ``DEFAULT_TTL_SEC`` (30 s) are expired from the store.

Polar display mapping
---------------------

Estimated metres → radius
~~~~~~~~~~~~~~~~~~~~~~~~~

The radar plot uses the **same** ``distance_m()`` as the detail line
(``ui/radar.py``). Radial position is linear in distance:

.. math::

   r = r_\mathrm{max} \cdot \min\!\left(1,\; \max\!\left(0,\; d / d_\mathrm{scale}\right)\right)

Centre = observer (0 m); the outer ring = ``scale_max_m``.

Auto-scale ring presets
~~~~~~~~~~~~~~~~~~~~~~~

``SCALE_PRESETS`` choose ``scale_max_m`` and ring labels from the farthest
device on screen:

.. list-table::
   :header-rows: 1
   :widths: 20 80

   * - ``scale_max_m``
     - Ring fractions (label examples)
   * - 10 m
     - 1 m, 5 m, 10 m
   * - 30 m
     - 3 m, 10 m, 30 m
   * - 100 m
     - 5 m, 20 m, 50 m, 100 m
   * - 300 m
     - 10 m, 50 m, 150 m, 300 m

The initial preset is chosen so the farthest device fits within
``threshold × 1.1``.

Hysteresis
~~~~~~~~~~

To avoid ring presets flipping every scan, preset changes require:

- **Zoom out:** farthest device > :math:`1.25 \times` current ``scale_max_m``
  and a larger preset is indicated
- **Zoom in:** farthest device < :math:`0.45 \times` current ``scale_max_m``
  and a smaller preset is indicated

Otherwise the previous preset is kept. Ring labels therefore describe
**display geometry**, not independent ranging measurements.

Bearing estimation
------------------

Three methods apply in priority order on snapshot:

1. **Manual pin** (persisted, highest priority for direction)
2. **Rotation calibration** (session RSSI vs heading)
3. **MAC-hash placeholder** (default)

MAC-hash placeholder
~~~~~~~~~~~~~~~~~~~~

When no bearing is calibrated:

.. math::

   \theta = \mathrm{MD5}(\mathrm{MAC})_{0:8} \bmod 360

(First eight hex digits of the MD5 digest, interpreted as an integer.)

This angle is **stable and deterministic** but has **no physical meaning**.
It prevents all devices stacking at one bearing.

Rotation calibration
~~~~~~~~~~~~~~~~~~~~

Press ``c`` to start ``BearingCalibrator``:

1. Slowly rotate the laptop; set heading with ``h`` / ``l`` (or arrows).
2. One sample ``(heading, RSSI)`` is recorded **per fresh background scan**
   (``scan_generation`` gate — not every UI frame).
3. After 25 s or a second ``c``, calibration stops and results are applied.

For each MAC with manual bearing unset:

- Require ≥ 4 samples
- Require RSSI spread :math:`\max - \min \geq 5` dB (else skip — too flat
  for omnidirectional / very close sources)
- Bearing = heading at **maximum RSSI** sample
- Confidence = :math:`\min(1,\; \mathrm{spread} / 20)`

Rotation calibration is **not** persisted; only manual pin and distance
references are saved to disk.

Manual bearing pin
~~~~~~~~~~~~~~~~~~

With a device selected, keypad-style keys pin bearing **relative to current
heading** (world bearing = ``(heading + offset) % 360``):

.. list-table::
   :header-rows: 1
   :widths: 15 25 60

   * - Key
     - Offset
     - Typical use
   * - ``8``
     - 0°
     - Ahead
   * - ``6``
     - +90°
     - Right
   * - ``4``
     - −90°
     - Left
   * - ``2``
     - 180°
     - Behind

Stored in ``calibration.json`` as ``manual_bearing``. On snapshot, manual
bearings set ``bearing_deg``, ``bearing_manual = True``, and
``bearing_confidence = 1.0``. Rotation calibration skips MACs with manual
bearings.

Displayed bearing
~~~~~~~~~~~~~~~~~

``display_bearing()`` returns, in order:

1. scene-correction override bearing (if any)
2. calibrated/manual ``bearing_deg``
3. MAC-hash placeholder angle

The detail line shows confidence as a percentage for rotation-calibrated
devices, ``manual`` for pinned devices, and ``uncal`` otherwise.

Scene correction modes
----------------------

After per-MAC calibration is applied, wifi-radar can optionally derive a
scene-wide correction from all currently visible calibrated devices.

RF-honest mode
~~~~~~~~~~~~~~

Saved calibration remains authoritative only for that MAC. For new devices,
the tool may apply a **global distance scale** derived from the median of
visible anchor residuals:

.. math::

   s_i = d_{\\mathrm{target},i} / d_{\\mathrm{raw},i}

.. math::

   s = \\mathrm{median}(s_i)

.. math::

   d_{\\mathrm{scene}} = s \\cdot d_{\\mathrm{raw}}

This mode does **not** globally invent physical bearing for unrelated devices.

Anchored-scene mode
~~~~~~~~~~~~~~~~~~~

This mode uses the same distance scaling, and also computes a heuristic global
bearing offset from anchors with saved manual bearing:

.. math::

   \\Delta\\theta_i = \\theta_{\\mathrm{manual},i} - \\theta_{\\mathrm{placeholder},i}

.. math::

   \\Delta\\theta = \\mathrm{median}(\\Delta\\theta_i)

For uncalibrated devices:

.. math::

   \\theta_{\\mathrm{scene}} =
   \\theta_{\\mathrm{placeholder}} + \\Delta\\theta

This is intentionally a **display heuristic**, not angle-of-arrival physics.

Anchor consistency
~~~~~~~~~~~~~~~~~~

An anchor is marked stale and excluded from scene solving when its currently
estimated calibrated distance drifts too far from its saved anchor distance.
The current implementation rejects anchors whose calibrated/current distance is
outside roughly ``0.55×`` to ``1.8×`` of the saved anchor distance.

Error budget and assumptions
----------------------------

Distance
~~~~~~~~

.. list-table::
   :header-rows: 1
   :widths: 35 65

   * - Factor
     - Effect
   * - Unknown TX power / antenna gain
     - Scales entire range estimate
   * - Multipath fading
     - RSSI swings of 10–30 dB indoors are common
   * - Fixed *n* = 3.0 (+0.3 on 5 GHz)
     - Real environments vary (~2–4+)
   * - Single-point RSSI calibration
     - Helps **one** device at **one** pose; does not fix environment
   * - EMA lag
     - Smoothed RSSI trails sudden movement

Expect **order-of-magnitude** accuracy indoors (roughly within a factor of
2–3 at best). Sub-metre truth (e.g. a phone 20 cm away) is **not** reliably
inferable from RSSI alone.

Bearing
~~~~~~~

.. list-table::
   :header-rows: 1
   :widths: 35 65

   * - Factor
     - Effect
   * - Omnidirectional antenna
     - RSSI barely changes with rotation; 5 dB gate rejects many close sources
   * - Body shadowing only
     - Weak directional cue vs true angle-of-arrival
   * - Multipath
     - False RSSI peaks at arbitrary headings
   * - MAC-hash default
     - Stable layout only; not geographic north

For trustworthy direction at close range, **manual pin** after physically
identifying the source is the intended workflow.

Consistency rule
~~~~~~~~~~~~~~~~

Both the polar plot and the detail line call ``distance_m()`` on the same
snapshot copy (with calibration overrides applied). If distance readings
and ring positions disagree, that indicates a software bug — not two
different models.

See also
--------

- :doc:`../user/calibration` — user workflow for pin and rotation calib
- :doc:`../user/limitations` — practical caveats
- :doc:`../rf-theory-and-capability` — RF hardware and receiver limits
