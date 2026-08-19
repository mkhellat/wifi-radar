Testing
=======

Test Suite
----------

Tests live in ``tests/`` and use pytest. No live WiFi hardware is required —
all parser tests use golden fixtures.

.. code-block:: bash

   pytest -v

Test Structure
--------------

.. code-block:: text

   tests/
   ├── __init__.py
   ├── fixtures/
   │   ├── iw_scan.txt        Golden output from `iw dev wlan0 scan`
   │   └── airodump.csv       Golden airodump-ng CSV (APs + stations)
   ├── test_iw_parse.py       iw scan parser tests
   ├── test_airodump.py       airodump CSV parser tests
   ├── test_merge.py          DeviceStore merge/TTL/EMA tests
   ├── test_distance.py       Path-loss distance model tests
   └── test_cli.py            CLI --version/--help subprocess tests

Writing Tests
-------------

**Parser tests**: Add a fixture file in ``tests/fixtures/`` representing real
tool output. Write tests that parse it and assert on extracted fields.

**Merge tests**: Use ``_make_dev()`` helper to create ``WifiDevice`` instances
with controlled parameters. Test EMA behaviour, TTL expiry, kind priority.

**CLI tests**: Run ``python -m wifi_radar`` as a subprocess with
``PYTHONPATH=src`` and assert on exit code / stdout.

**No live-radio tests**: The test suite must pass without ``sudo``, without
a wireless interface, and in CI (GitHub Actions Ubuntu runners have no WiFi).

Adding Golden Fixtures
----------------------

When adding a new scan backend or changing parser behaviour:

1. Capture real output from the tool (sanitise MACs if from a real environment)
2. Save to ``tests/fixtures/``
3. Write tests that cover normal cases, edge cases, and empty input
4. Verify with ``pytest -v``

Coverage
--------

No formal coverage threshold is enforced yet, but aim for:

- 100% of parser logic (every branch in the state machine)
- Core merge/TTL behaviour
- CLI argument handling
