//! Scan backend trait (nl80211 is the long-term target; iw is interim).

use crate::models::WifiDevice;

/// Error from a scan backend.
#[derive(Debug)]
pub enum ScanError {
    /// Backend failed with a message (CLI failure, nl80211 error, …).
    Message(String),
    /// I/O failure.
    Io(std::io::Error),
}

impl std::fmt::Display for ScanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ScanError::Message(m) => write!(f, "{m}"),
            ScanError::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for ScanError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ScanError::Io(e) => Some(e),
            ScanError::Message(_) => None,
        }
    }
}

impl From<std::io::Error> for ScanError {
    fn from(value: std::io::Error) -> Self {
        ScanError::Io(value)
    }
}

/// Passive observation scan source.
///
/// Implementations must not inject, deauth, associate, or crack. The intended
/// production backend is native nl80211 (`wifi-radar-linux`); CLI `iw` is an
/// interim shim for parity while that lands.
pub trait ScanBackend {
    /// Human-readable backend name (`iw-cli`, `nl80211`, …).
    fn name(&self) -> &'static str;

    /// Run one scan pass and return observed devices.
    fn scan(&mut self) -> Result<Vec<WifiDevice>, ScanError>;
}
