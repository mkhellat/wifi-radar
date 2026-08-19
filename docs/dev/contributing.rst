Contributing
============

Development Setup
-----------------

.. code-block:: bash

   git clone https://github.com/mkhellat/wifi-radar.git
   cd wifi-radar
   python3 -m venv .venv
   source .venv/bin/activate
   pip install -e ".[dev]"

Running Checks
--------------

.. code-block:: bash

   # Lint
   ruff check src/ tests/

   # Type check (strict mode)
   mypy

   # Tests
   pytest -v

All three must pass before submitting changes. CI runs these on Python
3.11, 3.12, and 3.13.

Code Style
----------

- **Line length**: 100 characters
- **Formatting**: follow ruff defaults (``ruff format`` if needed)
- **Imports**: sorted by ruff (isort-compatible)
- **Type annotations**: all public functions must have complete type hints
  (enforced by mypy strict mode)
- **Docstrings**: required for modules and public functions (Google style)

Commit Messages
---------------

- Use imperative mood ("Add feature", not "Added feature")
- First line: concise summary (≤72 chars)
- Body: explain *why*, not *what* (the diff shows what)
- Reference issue numbers where applicable

Scope Rules
-----------

- **Passive observation only.** No deauth, injection, association, or cracking.
- **Linux / nl80211 only.** No Windows, macOS, or cross-platform abstractions.
- **Stdlib only at runtime.** No pip dependencies for the installed package.
- **Do not add attack features.** This is a monitoring tool, not a pentest suite.

Mirrors
-------

The project is mirrored on two remotes:

- **GitHub** (origin): https://github.com/mkhellat/wifi-radar
- **Codeberg**: https://codeberg.org/mkhellat/wifi-radar

Push to both after merging to main.
