//! Domain core for wifi-radar (models, store, scan traits).
//!
//! Passive observation only. No deauth, injection, association, or cracking.

#![deny(missing_docs)]

/// Crate version string (mirrors workspace package version).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
