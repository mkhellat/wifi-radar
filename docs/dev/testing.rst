Testing
=======

Python test suite
-----------------

Tests live in ``tests/`` and use pytest. No live WiFi hardware is required —
all parser tests use golden fixtures.

.. code-block:: bash

   make test
   # or
   pytest -v

Test structure
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

Rust workspace tests
--------------------

The Cargo workspace under ``crates/`` has its own unit tests (core store,
iw parse, PyO3 façade helpers). Run from the repo root:

.. code-block:: bash

   cargo test --workspace

No live radio is required. ``wifi-radar-tui --demo`` is a manual smoke
path (needs a TTY), not part of ``cargo test``.

PyO3 / maturin smoke
--------------------

Optional, after ``pip install maturin`` in the project venv:

.. code-block:: bash

   cd crates/wifi-radar-py
   maturin develop --features extension-module
   python -c "import wifi_radar_native as w; print(w.version())"

Writing tests
-------------

**Parser tests**: Add a fixture file in ``tests/fixtures/`` representing real
tool output. Write tests that parse it and assert on extracted fields.

**Merge tests**: Use ``_make_dev()`` helper to create ``WifiDevice`` instances
with controlled parameters. Test EMA behaviour, TTL expiry, kind priority.

**CLI tests**: Run ``python -m wifi_radar`` as a subprocess with
``PYTHONPATH=src`` and assert on exit code / stdout.

**Rust tests**: Prefer ``#[cfg(test)]`` next to the behaviour under test in
``wifi-radar-core`` / ``wifi-radar-linux`` / ``wifi-radar-py``.

**No live-radio tests**: Suites must pass without ``sudo``, without a
wireless interface, and in CI (GitHub Actions Ubuntu runners have no WiFi).

Adding golden fixtures
----------------------

When adding a new scan backend or changing parser behaviour:

1. Capture real output from the tool (sanitise MACs if from a real environment)
2. Save to ``tests/fixtures/``
3. Write tests that cover normal cases, edge cases, and empty input
4. Verify with ``pytest -v`` (and ``cargo test`` if the Rust iw parser changes)

Coverage
--------

No formal coverage threshold is enforced yet, but aim for:

- 100% of parser logic (every branch in the state machine)
- Core merge/TTL behaviour (Python and Rust)
- CLI argument handling
