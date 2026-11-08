# wifi-radar-py

Thin PyO3 / Maturin façade exposing the Rust core to Python for scripting and
the existing `wifi-radar` console-script migration path.

Maturin packaging lands in a later milestone; this crate currently builds as
an ordinary Rust library until PyO3 is wired.
