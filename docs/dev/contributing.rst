Contributing
============

Development setup
-----------------

.. code-block:: bash

   git clone https://github.com/mkhellat/wifi-radar.git
   cd wifi-radar
   ./configure --with-docs
   make

This project uses the classic ``./configure && make`` workflow. The committed
``Makefile.in`` template is converted into a machine-specific ``Makefile`` by
``./configure``.

For Rust workspace work you also need a recent Rust toolchain
(``rustc`` / ``cargo``, edition 2021, MSRV 1.75+). Optional: ``maturin`` in
the project venv for the PyO3 façade.

Running checks
--------------

.. code-block:: bash

   make check          # ruff + mypy + pytest
   cargo test --workspace
   make docs           # sphinx-build -W

   # or individual Python checks
   make lint
   make typecheck
   make test

Python checks must pass before submitting Python changes. Rust changes need
``cargo test --workspace``. CI runs the Python matrix plus a
``rust-workspace`` job (``cargo test --workspace``).

Code style
----------

**Python**

- **Line length**: 100 characters
- **Formatting**: follow ruff defaults (``make format`` if needed)
- **Imports**: sorted by ruff (isort-compatible)
- **Type annotations**: all public functions must have complete type hints
  (enforced by mypy strict mode)
- **Docstrings**: required for modules and public functions (Google style)

**Rust**

- Prefer small, documented public APIs (``#![deny(missing_docs)]`` on library
  crates)
- Match existing module layout under ``crates/``
- Keep ScanBackend passive-only — no injection helpers

Commit messages
---------------

- Use imperative mood ("Add feature", not "Added feature")
- Prefer an area tag when helpful (``tui:``, ``py:``, ``docs:``, ``core:``)
- First line: concise summary (≈50–72 chars)
- Body: explain *why*, then *what*, then how to verify
- One reviewable design step per commit (prefer 1–3 related files)
- Do not use "one code mega-commit + one docs mega-commit" as the only split

Scope rules
-----------

- **Passive observation only.** No deauth, injection, association, or cracking.
- **Linux / nl80211 only.** No Windows, macOS, or cross-platform abstractions.
- **Shipping Python package:** stdlib-only at runtime for the curses app.
  The optional ``wifi_radar_native`` extension is a separate Maturin artefact.
- **Do not add attack features.** This is a monitoring tool, not a pentest suite.
- **Never commit** local agent notes: ``CHECKPOINT.md``, ``CLAUDE.md``,
  ``.cursor/``, ``plans/``.

Mirrors and landing path
------------------------

The project is mirrored on two remotes with **different landing rules**:

- **GitHub** (origin): https://github.com/mkhellat/wifi-radar —
  reviewable work lands on ``main`` via **pull requests** (a coherent
  collection of atomic commits). Do not push feature work straight to
  ``origin/main``.
- **Codeberg**: https://codeberg.org/mkhellat/wifi-radar —
  keep **atomic one-SHA pushes** so the Codeberg activity log stays
  step-by-step readable.

Typical flow:

1. Branch from ``main`` (``topic/short-name``).
2. One reviewable design step per commit (prefer 1–3 related files).
3. After each commit, push that single SHA to the **same branch on
   Codeberg** before starting the next commit.
4. When the collection is ready, open a GitHub PR into ``main``.
5. After the PR merges, sync ``codeberg/main`` (still one SHA at a time
   when replaying several commits).

On networks that need it, prefix remote git/ssh with ``proxychains4 -q``.

Pull request body (GNU / Savannah style)
----------------------------------------

Write PR descriptions for human reviewers, in the spirit of GNU Savannah
patch mail and ChangeLog notes — not a paste of commit subjects alone.

Include:

- **Purpose** — what problem this collection solves (one short paragraph).
- **Motivation** — why the change is needed now / what was wrong before.
- **Changes** — file-oriented notes (``* path:`` imperative summary),
  grouped if the PR spans several commits.
- **How to verify** — commands reviewers can run (``make check``,
  ``cargo test --workspace``, ``sphinx-build -W``, demo TUI, etc.).

Keep the PR scoped to one coherent theme (e.g. “nl80211 scan MVP”, not
“nl80211 + polar TUI + multi-node”).

Prefer merge or rebase onto ``main`` so atomic commits stay visible; do
not squash a stack that was authored as separate reviewable steps.
