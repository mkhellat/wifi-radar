//! Application state for the Ratatui MVP.

use std::time::{Duration, Instant};

use wifi_radar_core::{DeviceStore, ScanBackend, WifiDevice};
use wifi_radar_linux::IwCliBackend;

/// How the TUI obtains scan samples.
pub enum ScanSource {
    /// Run `iw` on a real interface.
    Iw(IwCliBackend),
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
            ScanSource::Iw(IwCliBackend::new(iface.clone()))
        };
        Self {
            store: DeviceStore::new(),
            devices: Vec::new(),
            selected: 0,
            status: if demo {
                "DEMO mode — synthetic devices (no iw scan)".into()
            } else {
                format!("Scanning {iface} via iw-cli …")
            },
            iface,
            source,
            should_quit: false,
            last_scan: Instant::now() - Duration::from_secs(60),
            scan_interval: Duration::from_secs(8),
        }
    }

    /// Run a scan if the interval elapsed or `force` is set.
    pub fn maybe_scan(&mut self, force: bool) {
        if !force && self.last_scan.elapsed() < self.scan_interval {
            return;
        }
        self.last_scan = Instant::now();

        // Finish the source borrow before refreshing the snapshot.
        let outcome = match &mut self.source {
            ScanSource::Demo => Ok((demo_devices(), "demo")),
            ScanSource::Iw(backend) => {
                let name = backend.name();
                backend.scan().map(|batch| (batch, name))
            }
        };

        match outcome {
            Ok((batch, backend_name)) => {
                let n = batch.len();
                self.store.merge(batch);
                self.refresh_snapshot();
                self.status = if backend_name == "demo" {
                    format!(
                        "DEMO  {} device(s)  (j/k select, r rescan, q quit)",
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
