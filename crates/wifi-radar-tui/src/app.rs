//! Application state for the Ratatui MVP.

use std::time::{Duration, Instant};

use wifi_radar_core::{
    apply_scene_corrections, CalibrationMode, CalibrationStore, CorrectionSummary, DeviceStore,
    ScanBackend, ScanError, WifiDevice,
};
use wifi_radar_linux::{IwCliBackend, Nl80211Backend};

use crate::polar::ScaleState;

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
    /// Selected device MAC (stable across RSSI re-sorts).
    pub selected_mac: Option<String>,
    pub status: String,
    pub iface: String,
    pub source: ScanSource,
    /// Last successful live backend name (`nl80211` / `iw-cli`), if any.
    pub last_backend: Option<&'static str>,
    /// Compass heading in degrees (h/l rotate).
    pub heading_deg: f64,
    /// Auto-scale hysteresis for polar rings.
    pub scale: ScaleState,
    /// Per-MAC pin / distance calibration (Python JSON-compatible).
    pub calibration: CalibrationStore,
    /// Last scene-correction summary (updated on snapshot refresh).
    pub correction: CorrectionSummary,
    /// Vi-style `:` command entry is active.
    pub command_mode: bool,
    /// Buffer contents after the leading `:`.
    pub command_buf: String,
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
            selected_mac: None,
            status: if demo {
                "DEMO mode — synthetic devices (no radio scan)".into()
            } else {
                format!("Scanning {iface} via nl80211 (iw-cli fallback) …")
            },
            iface,
            source,
            last_backend: None,
            heading_deg: 0.0,
            scale: ScaleState::default(),
            calibration: if demo {
                CalibrationStore::memory()
            } else {
                CalibrationStore::new(CalibrationStore::default_path())
            },
            correction: CorrectionSummary {
                mode: CalibrationMode::Honest,
                visible_anchors: 0,
                active_anchors: 0,
                distance_scale: 1.0,
                bearing_offset_deg: 0.0,
            },
            command_mode: false,
            command_buf: String::new(),
            should_quit: false,
            last_scan: Instant::now() - Duration::from_secs(60),
            scan_interval: Duration::from_secs(8),
        }
    }

    /// Active calibration mode label (`honest` / `anchor`).
    pub fn mode_label(&self) -> &'static str {
        match self.calibration.mode() {
            CalibrationMode::Honest => "honest",
            CalibrationMode::Anchor => "anchor",
        }
    }

    /// Enter vi-style command mode (`:`).
    pub fn begin_command_mode(&mut self) {
        self.command_mode = true;
        self.command_buf.clear();
    }

    /// Cancel command mode without executing.
    pub fn cancel_command_mode(&mut self) {
        self.command_mode = false;
        self.command_buf.clear();
    }

    /// Append a printable character to the command buffer.
    pub fn command_push_char(&mut self, ch: char) {
        if !self.command_mode || !ch.is_ascii() || ch.is_control() {
            return;
        }
        self.command_buf.push(ch);
    }

    /// Backspace in command mode; leave command mode if buffer empties.
    pub fn command_backspace(&mut self) {
        if !self.command_mode {
            return;
        }
        self.command_buf.pop();
        if self.command_buf.is_empty() {
            self.command_mode = false;
        }
    }

    /// Execute the current command buffer (Python `:mode` / `:q` / `:r` parity subset).
    pub fn execute_command(&mut self) {
        if !self.command_mode {
            return;
        }
        let cmd = self.command_buf.trim().to_ascii_lowercase();
        self.command_mode = false;
        self.command_buf.clear();
        if cmd.is_empty() {
            return;
        }
        if cmd == "q" || cmd == "quit" {
            self.should_quit = true;
            return;
        }
        if cmd == "r" || cmd == "rescan" {
            self.maybe_scan(true);
            return;
        }
        if let Some(rest) = cmd.strip_prefix("mode ") {
            let mode = rest.trim();
            let parsed = match mode {
                "honest" => Some(CalibrationMode::Honest),
                "anchor" => Some(CalibrationMode::Anchor),
                _ => None,
            };
            match parsed {
                Some(m) => match self.calibration.set_mode(m) {
                    Ok(()) => {
                        self.refresh_snapshot();
                        self.status = format!("Calibration mode: {mode}");
                    }
                    Err(e) => self.status = format!("mode failed: {e}"),
                },
                None => self.status = format!("Unknown mode: {mode}"),
            }
            return;
        }
        self.status = format!("Unknown command: {cmd}");
    }

    /// Selected MAC as `&str`, if any.
    pub fn selected_mac(&self) -> Option<&str> {
        self.selected_mac.as_deref()
    }

    /// Index of the selected device in the current snapshot, if present.
    pub fn selected_index(&self) -> Option<usize> {
        let mac = self.selected_mac.as_ref()?;
        self.devices.iter().position(|d| &d.mac == mac)
    }

    /// Rotate heading by `delta_deg` (normalized to \[0, 360)).
    pub fn rotate_heading(&mut self, delta_deg: f64) {
        self.heading_deg = (self.heading_deg + delta_deg).rem_euclid(360.0);
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
        self.calibration.apply_to_devices(&mut self.devices);
        self.correction = apply_scene_corrections(&mut self.devices, &self.calibration);
        if let Some(mac) = &self.selected_mac {
            if !self.devices.iter().any(|d| &d.mac == mac) {
                self.selected_mac = None;
            }
        }
    }

    /// Pin selected device at `heading + offset_deg` (world bearing).
    pub fn pin_selected(&mut self, offset_deg: f64) {
        let Some(mac) = self.selected_mac.clone() else {
            self.status = "select a device before pinning (j/k)".into();
            return;
        };
        let bearing = (self.heading_deg + offset_deg).rem_euclid(360.0);
        match self.calibration.set_manual_bearing(&mac, bearing) {
            Ok(()) => {
                self.refresh_snapshot();
                self.status = format!("Pinned {mac} at {bearing:.0}° (manual)");
            }
            Err(e) => self.status = format!("pin failed: {e}"),
        }
    }

    /// Calibrate selected device distance reference at `distance_m`.
    pub fn calibrate_distance_selected(&mut self, distance_m: f64) {
        let Some(mac) = self.selected_mac.clone() else {
            self.status = "select a device before distance calib (j/k)".into();
            return;
        };
        let Some(dev) = self.devices.iter().find(|d| d.mac == mac) else {
            self.status = "selected device missing from snapshot".into();
            return;
        };
        let rssi = dev.rssi_dbm;
        match self
            .calibration
            .set_distance_reference(&mac, rssi, distance_m)
        {
            Ok(ref_dbm) => {
                self.refresh_snapshot();
                self.status = format!(
                    "Distance calib {mac} → {distance_m} m (rssi_at_1m={ref_dbm:.1})"
                );
            }
            Err(e) => self.status = format!("distance calib failed: {e}"),
        }
    }

    /// Clear saved calibration for the selected MAC.
    pub fn clear_calibration_selected(&mut self) {
        let Some(mac) = self.selected_mac.clone() else {
            self.status = "select a device before clear (j/k)".into();
            return;
        };
        match self.calibration.clear(&mac) {
            Ok(()) => {
                self.refresh_snapshot();
                self.status = format!("Cleared calibration for {mac}");
            }
            Err(e) => self.status = format!("clear failed: {e}"),
        }
    }

    pub fn select_next(&mut self) {
        if self.devices.is_empty() {
            self.selected_mac = None;
            return;
        }
        let next = match self.selected_index() {
            None => 0,
            Some(i) => (i + 1) % self.devices.len(),
        };
        self.selected_mac = Some(self.devices[next].mac.clone());
    }

    pub fn select_prev(&mut self) {
        if self.devices.is_empty() {
            self.selected_mac = None;
            return;
        }
        let prev = match self.selected_index() {
            None => self.devices.len() - 1,
            Some(0) => self.devices.len() - 1,
            Some(i) => i - 1,
        };
        self.selected_mac = Some(self.devices[prev].mac.clone());
    }

    pub fn select_first(&mut self) {
        self.selected_mac = self.devices.first().map(|d| d.mac.clone());
    }

    pub fn select_last(&mut self) {
        self.selected_mac = self.devices.last().map(|d| d.mac.clone());
    }

    pub fn clear_selection(&mut self) {
        self.selected_mac = None;
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

    #[test]
    fn selection_tracks_mac_across_resort() {
        let mut app = App::new("wlan0".into(), true);
        app.maybe_scan(true);
        app.select_next(); // strongest first after snapshot; pick first
        let mac = app.selected_mac().unwrap().to_string();
        // Simulate resort by reversing devices while keeping selection MAC.
        app.devices.reverse();
        assert_eq!(app.selected_mac(), Some(mac.as_str()));
        assert!(app.selected_index().is_some());
    }

    #[test]
    fn rotate_heading_wraps() {
        let mut app = App::new("wlan0".into(), true);
        app.rotate_heading(-10.0);
        assert!((app.heading_deg - 350.0).abs() < 1e-9);
        app.rotate_heading(20.0);
        assert!((app.heading_deg - 10.0).abs() < 1e-9);
    }

    #[test]
    fn pin_and_distance_on_selected_demo() {
        let mut app = App::new("wlan0".into(), true);
        app.maybe_scan(true);
        app.select_first();
        app.heading_deg = 10.0;
        app.pin_selected(0.0);
        let mac = app.selected_mac().unwrap().to_string();
        assert!((app.calibration.manual_bearing(&mac).unwrap() - 10.0).abs() < 1e-9);
        app.calibrate_distance_selected(1.0);
        assert!(app.calibration.rssi_at_1m(&mac).is_some());
        app.clear_calibration_selected();
        assert!(app.calibration.manual_bearing(&mac).is_none());
    }

    #[test]
    fn mode_command_sets_anchor() {
        let mut app = App::new("wlan0".into(), true);
        assert_eq!(app.mode_label(), "honest");
        app.begin_command_mode();
        for ch in "mode anchor".chars() {
            app.command_push_char(ch);
        }
        app.execute_command();
        assert!(!app.command_mode);
        assert_eq!(app.mode_label(), "anchor");
        assert_eq!(app.calibration.mode(), CalibrationMode::Anchor);
        assert!(app.status.contains("anchor"));
    }

    #[test]
    fn unknown_mode_leaves_store_unchanged() {
        let mut app = App::new("wlan0".into(), true);
        app.begin_command_mode();
        for ch in "mode weird".chars() {
            app.command_push_char(ch);
        }
        app.execute_command();
        assert_eq!(app.calibration.mode(), CalibrationMode::Honest);
        assert!(app.status.contains("Unknown mode"));
    }

    #[test]
    fn command_backspace_exits_when_empty() {
        let mut app = App::new("wlan0".into(), true);
        app.begin_command_mode();
        app.command_push_char('m');
        app.command_backspace();
        assert!(!app.command_mode);
    }

    #[test]
    fn refresh_applies_scene_distance_in_honest_mode() {
        let mut app = App::new("wlan0".into(), true);
        app.maybe_scan(true);
        app.select_first();
        let mac = app.selected_mac().unwrap().to_string();
        app.calibrate_distance_selected(1.0);
        assert!(app.calibration.has_distance_calibration(&mac));
        assert!(app.correction.active_anchors >= 1);
        let other = app
            .devices
            .iter()
            .find(|d| d.mac != mac)
            .expect("demo has multiple devices");
        assert_eq!(other.correction_note, "scene distance");
        assert!(other.distance_override_m.is_some());
    }

    #[test]
    fn anchor_mode_propagates_bearing_override() {
        let mut app = App::new("wlan0".into(), true);
        app.maybe_scan(true);
        app.select_first();
        app.heading_deg = 0.0;
        app.pin_selected(90.0);
        app.begin_command_mode();
        for ch in "mode anchor".chars() {
            app.command_push_char(ch);
        }
        app.execute_command();
        assert_eq!(app.calibration.mode(), CalibrationMode::Anchor);
        let pin_mac = app.selected_mac().unwrap().to_string();
        let other = app
            .devices
            .iter()
            .find(|d| d.mac != pin_mac)
            .expect("demo has multiple devices");
        assert_eq!(other.correction_note, "scene anchor");
        assert!(other.bearing_override_deg.is_some());
    }
}
