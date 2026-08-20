wifi-radar documentation
========================

**wifi-radar** is an interactive terminal radar that maps nearby WiFi access
points, associated clients, and probe-only adapters on a polar display.
It estimates distance from RSSI and direction from calibration.

.. note::

   Passive observation only. No deauth, injection, association, or cracking.

.. toctree::
   :maxdepth: 2
   :caption: User Guide

   user/install
   user/usage
   user/keys
   user/calibration
   user/limitations

.. toctree::
   :maxdepth: 2
   :caption: RF Capability

   theory/overview
   theory/hardware
   theory/receiver
   theory/band-rejection
   theory/firmware
   theory/spectral-scan
   theory/csi-and-iq
   theory/capability-matrix
   theory/tiers

.. toctree::
   :maxdepth: 2
   :caption: Localization & Ranging

   theory/localization

.. toctree::
   :maxdepth: 2
   :caption: Developer Guide

   dev/architecture
   dev/contributing
   dev/testing

.. toctree::
   :maxdepth: 2
   :caption: API Reference

   api/models
   api/merge
   api/scan
   api/ui
   api/cli

Indices and tables
==================

* :ref:`genindex`
* :ref:`modindex`
