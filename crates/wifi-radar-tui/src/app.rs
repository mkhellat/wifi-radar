//! Application state for the Ratatui MVP.

use std::time::{Duration, Instant};

use wifi_radar_core::{DeviceStore, ScanBackend, ScanError, WifiDevice};
use wifi_radar_linux::{IwCliBackend, Nl80211Backend};

/// How the TUI obtains scan samples.
pub enum ScanSource {
    /// Prefer genetlink GET_SCAN; fall back to `iw` on error.
    Live {
        nl: Nl80211Backend,
        iw: IwCliBackend,
    },
    /// Synthetic devices (no root / no radio needed).
    Demo,
}

/// Mutable TUI state.
pub struct App {
    pub store: DeviceStore,
    pub devices: Vec<WifiDevice>,
    pub selected: usize,
    pub status: String,
    pub iface: String,
    pub source: ScanSource,
    /// Last successful live backend name (`nl80211` / `iw-cli`), if any.
    pub last_backend: Option<&'static str>,
    pub should_quit: bool,
    last_scan: Instant,
    scan_interval: Duration,
}

impl App {
    /// Create an app bound to `iface` (or demo mode).
    pub fn new(iface: String, demo: bool) -> Self {
        let source = if demo {
            ScanSource::Demo
        } else {
            ScanSource::Live {
                nl: Nl80211Backend::new(iface.clone()),
                iw: IwCliBackend::new(iface.clone()),
            }
        };
        Self {
            store: DeviceStore::new(),
            devices: Vec::new(),
            selected: 0,
            status: if demo {
                "DEMO mode — synthetic devices (no radio scan)".into()
            } else {
                format!("Scanning {iface} via nl80211 (iw-cli fallback) …")
            },
            iface,
            source,
            last_backend: None,
            should_quit: false,
            last_scan: Instant::now() - Duration::from_secs(60),
            scan_interval: Duration::from_secs(8),
        }
    }

    /// Header label for the active / preferred backend.
    pub fn backend_label(&self) -> &'static str {
        match &self.source {
            ScanSource::Demo => "demo",
            ScanSource::Live { .. } => self.last_backend.unwrap_or("nl80211"),
        }
    }

    /// Run a scan if the interval elapsed or `force` is set.
    pub fn maybe_scan(&mut self, force: bool) {
        if !force && self.last_scan.elapsed() < self.scan_interval {
            return;
        }
        self.last_scan = Instant::now();

        let outcome = match &mut self.source {
            ScanSource::Demo => Ok((demo_devices(), "demo", None)),
            ScanSource::Live { nl, iw } => scan_live(nl, iw),
        };

        match outcome {
            Ok((batch, backend_name, note)) => {
                let n = batch.len();
                self.store.merge(batch);
                self.refresh_snapshot();
                if backend_name != "demo" {
                    self.last_backend = Some(backend_name);
                }
                self.status = if backend_name == "demo" {
                    format!(
                        "DEMO  {} device(s)  (j/k select, r rescan, q quit)",
                        self.devices.len()
                    )
                } else if let Some(note) = note {
                    format!(
                        "{} via {backend_name} ({note})  last {n} BSS  store {}  (j/k r q)",
                        self.iface,
                        self.devices.len()
                    )
                } else {
                    format!(
                        "{} via {backend_name}  last scan {n} BSS  store {}  (j/k r q)",
                        self.iface,
                        self.devices.len()
                    )
                };
            }
            Err(e) => {
                self.status = format!("scan error on {}: {e}", self.iface);
            }
        }
    }

    fn refresh_snapshot(&mut self) {
        self.devices = self.store.snapshot();
        if self.devices.is_empty() {
            self.selected = 0;
        } else if self.selected >= self.devices.len() {
            self.selected = self.devices.len() - 1;
        }
    }

    pub fn select_next(&mut self) {
        if !self.devices.is_empty() {
            self.selected = (self.selected + 1) % self.devices.len();
        }
    }

    pub fn select_prev(&mut self) {
        if !self.devices.is_empty() {
            self.selected = if self.selected == 0 {
                self.devices.len() - 1
            } else {
                self.selected - 1
            };
        }
    }
}

/// Try nl80211 first; on error fall back to iw-cli for this pass.
fn scan_live(
    nl: &mut Nl80211Backend,
    iw: &mut IwCliBackend,
) -> Result<(Vec<WifiDevice>, &'static str, Option<&'static str>), ScanError> {
    match nl.scan() {
        Ok(batch) => Ok((batch, nl.name(), None)),
        Err(nl_err) => match iw.scan() {
            Ok(batch) => Ok((batch, iw.name(), Some("fell back from nl80211"))),
            Err(iw_err) => Err(ScanError::Message(format!(
                "nl80211: {nl_err}; iw-cli: {iw_err}"
            ))),
        },
    }
}

fn demo_devices() -> Vec<WifiDevice> {
    let mut a = WifiDevice::hotspot("aa:aa:aa:00:00:01", -42.0);
    a.ssid = "Demo-AP-Near".into();
    a.channel = 6;
    a.freq_mhz = 2437;
    let mut b = WifiDevice::hotspot("aa:aa:aa:00:00:02", -68.0);
    b.ssid = "Demo-AP-Far".into();
    b.channel = 36;
    b.freq_mhz = 5180;
    let mut c = WifiDevice::hotspot("aa:aa:aa:00:00:03", -55.0);
    c.ssid = "Demo-Coffee".into();
    c.channel = 11;
    c.freq_mhz = 2462;
    vec![a, b, c]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_source_label() {
        let app = App::new("wlan0".into(), true);
        assert_eq!(app.backend_label(), "demo");
        assert!(matches!(app.source, ScanSource::Demo));
    }

    #[test]
    fn live_defaults_to_nl80211_label() {
        let app = App::new("wlan0".into(), false);
        assert_eq!(app.backend_label(), "nl80211");
        assert!(matches!(app.source, ScanSource::Live { .. }));
    }
}
