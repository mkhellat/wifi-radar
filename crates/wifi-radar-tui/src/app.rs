// SPDX-FileCopyrightText: 2026 wifi-radar contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Application state for the Ratatui MVP.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, TryLockError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use wifi_radar_core::{
    apply_scene_corrections, BearingCalibrator, CalibrationMode, CalibrationStore,
    CorrectionSummary, DeviceStore, ScanBackend, ScanError, WifiDevice,
};
use wifi_radar_linux::{
    is_root, sample_airodump, sample_tshark_probes, IwCliBackend, MonitorSession, Nl80211Backend,
    AIRODUMP_SAMPLE, TSHARK_SAMPLE,
};

use crate::polar::ScaleState;

/// One finished passive sample waiting for the UI thread to merge.
struct MonitorBatch {
    generation: u64,
    devices: Vec<WifiDevice>,
    note: String,
}

#[cfg(test)]
type MonitorSampler = Arc<dyn Fn() -> (Vec<WifiDevice>, String) + Send + Sync>;

/// Background airodump/tshark capture. The UI thread only polls.
///
/// At most one sample runs. Turning monitor off or shutting down bumps
/// `generation` so a late batch is dropped instead of merged.
struct MonitorHub {
    session: Arc<Mutex<MonitorSession>>,
    tx: Sender<MonitorBatch>,
    rx: Receiver<MonitorBatch>,
    generation: Arc<AtomicU64>,
    worker: Option<JoinHandle<()>>,
    #[cfg(test)]
    sampler: Option<MonitorSampler>,
}

impl MonitorHub {
    fn new(iface: impl Into<String>) -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            session: Arc::new(Mutex::new(MonitorSession::new(iface))),
            tx,
            rx,
            generation: Arc::new(AtomicU64::new(0)),
            worker: None,
            #[cfg(test)]
            sampler: None,
        }
    }

    #[cfg(test)]
    fn created(&self) -> bool {
        self.session
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .created()
    }

    fn sampling(&self) -> bool {
        self.worker
            .as_ref()
            .is_some_and(|handle| !handle.is_finished())
    }

    /// Drop queued batches and tear the VIF down. A sample already running
    /// is left to finish, but its generation no longer matches.
    fn invalidate(&mut self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
        while self.rx.try_recv().is_ok() {}
        match self.session.try_lock() {
            Ok(mut session) => session.teardown(),
            Err(TryLockError::Poisoned(err)) => err.into_inner().teardown(),
            Err(TryLockError::WouldBlock) => {}
        }
    }

    fn reap(&mut self) {
        let finished = self
            .worker
            .as_ref()
            .is_some_and(|handle| handle.is_finished());
        if finished {
            if let Some(handle) = self.worker.take() {
                let _ = handle.join();
            }
        }
    }

    /// Start one sample. False when a sample is already in flight.
    fn try_spawn(&mut self) -> bool {
        self.reap();
        if self.worker.is_some() {
            return false;
        }
        let gen = self.generation.load(Ordering::SeqCst);
        let session = Arc::clone(&self.session);
        let generation = Arc::clone(&self.generation);
        let tx = self.tx.clone();
        #[cfg(test)]
        let sampler = self.sampler.clone();
        self.worker = Some(thread::spawn(move || {
            let produced = produce_sample(
                &session,
                &generation,
                gen,
                #[cfg(test)]
                sampler,
            );
            let Some((devices, note)) = produced else {
                return;
            };
            if generation.load(Ordering::SeqCst) != gen {
                return;
            }
            let _ = tx.send(MonitorBatch {
                generation: gen,
                devices,
                note,
            });
        }));
        true
    }

    fn poll(&mut self) -> Option<MonitorBatch> {
        let mut found = None;
        while let Ok(batch) = self.rx.try_recv() {
            if batch.generation == self.generation.load(Ordering::SeqCst) {
                found = Some(batch);
            }
        }
        found
    }
}

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
    /// Full-screen help overlay is visible.
    pub help_overlay: bool,
    /// Session rotation calibrator (`c` / `:calib`).
    pub bearing_calibrator: BearingCalibrator,
    /// Increments on each successful scan (gates calibrator samples).
    pub scan_generation: u64,
    /// Passive monitor capture (`m` / `:monitor`). Off until toggled.
    pub mon_enabled: bool,
    /// Passive `{phy}mon` capture. The worker thread samples; the UI polls.
    monitor: MonitorHub,
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
        let monitor = MonitorHub::new(iface.clone());
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
            help_overlay: false,
            bearing_calibrator: BearingCalibrator::new(),
            scan_generation: 0,
            mon_enabled: false,
            monitor,
            should_quit: false,
            last_scan: Instant::now() - Duration::from_secs(60),
            scan_interval: Duration::from_secs(8),
        }
    }

    /// Toggle session rotation calibration (Python `c` / `:calib`).
    pub fn toggle_bearing_calib(&mut self) {
        if self.bearing_calibrator.active() {
            self.bearing_calibrator.stop();
            let n = self
                .bearing_calibrator
                .apply(&mut self.store, &self.calibration);
            self.refresh_snapshot();
            self.status = format!("Calibration stopped ({n} bearing(s) applied)");
        } else {
            self.bearing_calibrator.start();
            self.status = "CALIBRATING: rotate slowly, h/l to turn".into();
        }
    }

    /// Toggle passive monitor capture. Off tears the VIF down immediately.
    ///
    /// Demo mode never opens a radio. Without root the flag can still flip,
    /// but scans stay on nl80211 / iw and no VIF is created.
    pub fn toggle_monitor(&mut self) {
        self.mon_enabled = !self.mon_enabled;
        if !self.mon_enabled {
            self.monitor.invalidate();
            self.status = "Monitor OFF".into();
            return;
        }
        self.status = if matches!(self.source, ScanSource::Demo) {
            "Monitor ON (demo: no radio capture)".into()
        } else if !is_root() {
            "Monitor ON needs root — scans stay on nl80211/iw".into()
        } else {
            "Monitor ON — background airodump/tshark on the next scan".into()
        };
    }

    /// Drop the monitor VIF if this process created it. Also runs on drop.
    ///
    /// A sample still in flight is not joined. Its batch is discarded.
    pub fn shutdown(&mut self) {
        self.mon_enabled = false;
        self.monitor.invalidate();
    }

    /// Show the help overlay (dismiss with any key).
    pub fn show_help(&mut self) {
        self.help_overlay = true;
        self.command_mode = false;
        self.command_buf.clear();
    }

    /// Dismiss the help overlay.
    pub fn dismiss_help(&mut self) {
        self.help_overlay = false;
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
        if cmd == "h" || cmd == "help" {
            self.show_help();
            return;
        }
        if cmd == "x" || cmd == "clear" {
            self.clear_calibration_selected();
            return;
        }
        if cmd == "c" || cmd == "calib" || cmd == "calibrate" {
            self.toggle_bearing_calib();
            return;
        }
        if cmd == "m" || cmd == "monitor" {
            self.toggle_monitor();
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

    /// Header token for passive monitor: `off`, `on`, or `sampling`.
    ///
    /// `sampling` is only shown while monitor mode is still on and a
    /// background sample has not finished. Turning monitor off clears it
    /// even if the worker has not exited yet, because that batch is dropped.
    pub fn monitor_label(&self) -> &'static str {
        if self.mon_enabled && self.monitor_sampling() {
            "sampling"
        } else if self.mon_enabled {
            "on"
        } else {
            "off"
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
    ///
    /// nl80211 / `iw` (and demo) stay on this thread. Passive monitor
    /// capture is only polled here: a finished batch is merged, and a new
    /// sample is started without waiting for airodump or tshark.
    pub fn maybe_scan(&mut self, force: bool) {
        let completed = self.poll_monitor_sample();
        if !force && self.last_scan.elapsed() < self.scan_interval {
            if let Some(note) = completed {
                self.attach_monitor_note(&note);
            }
            return;
        }
        self.last_scan = Instant::now();

        let outcome = match &mut self.source {
            ScanSource::Demo => Ok((demo_devices(), "demo", None)),
            ScanSource::Live { nl, iw } => scan_live(nl, iw),
        };

        let managed = match outcome {
            Ok((batch, backend_name, note)) => {
                let n = batch.len();
                self.store.merge(batch);
                self.scan_generation = self.scan_generation.saturating_add(1);
                if backend_name != "demo" {
                    self.last_backend = Some(backend_name);
                }
                Ok((n, backend_name, note))
            }
            Err(e) => Err(e.to_string()),
        };

        if managed.is_ok() {
            self.refresh_snapshot();
            self.bearing_calibrator
                .feed(self.heading_deg, &self.devices, self.scan_generation);
        }

        match managed {
            Ok((n, backend_name, note)) => {
                if self.bearing_calibrator.active() {
                    self.status = format!(
                        "CALIBRATING  gen={}  {} device(s)  (h/l turn, c stop)",
                        self.scan_generation,
                        self.devices.len()
                    );
                } else if backend_name == "demo" {
                    self.status = format!(
                        "DEMO  {} device(s)  (j/k select, r rescan, q quit)",
                        self.devices.len()
                    );
                } else if let Some(note) = note {
                    self.status = format!(
                        "{} via {backend_name} ({note})  last {n} BSS  store {}  (j/k r q)",
                        self.iface,
                        self.devices.len()
                    );
                } else {
                    self.status = format!(
                        "{} via {backend_name}  last scan {n} BSS  store {}  (j/k r q)",
                        self.iface,
                        self.devices.len()
                    );
                }
            }
            Err(e) => {
                self.status = format!("scan error on {}: {e}", self.iface);
            }
        }
        if self.mon_enabled {
            let (note, _) = self.capture_monitor();
            if !self.bearing_calibrator.active() {
                let show = if self.monitor_sampling() {
                    note
                } else if let Some(done) = completed.clone() {
                    done
                } else {
                    note
                };
                self.status = format!("{}  |  {show}", self.status);
            }
        } else if let Some(note) = completed {
            self.attach_monitor_note(&note);
        }
    }

    /// True while a passive monitor sample is running off the UI thread.
    fn monitor_sampling(&self) -> bool {
        self.monitor.sampling()
    }

    /// Spawn one background sample. False when one is already in flight.
    fn spawn_monitor_sample(&mut self) -> bool {
        self.monitor.try_spawn()
    }

    #[cfg(test)]
    fn install_test_sampler(&mut self, sampler: MonitorSampler) {
        self.monitor.sampler = Some(sampler);
    }

    /// Merge a finished monitor batch, if one is waiting and still current.
    fn poll_monitor_sample(&mut self) -> Option<String> {
        let batch = self.monitor.poll()?;
        let note = batch.note;
        if !batch.devices.is_empty() {
            self.store.merge(batch.devices);
            self.refresh_snapshot();
            self.bearing_calibrator
                .feed(self.heading_deg, &self.devices, self.scan_generation);
        }
        if note.is_empty() {
            None
        } else {
            Some(note)
        }
    }

    fn attach_monitor_note(&mut self, note: &str) {
        if self.bearing_calibrator.active() {
            return;
        }
        let base = self.status.split("  |  ").next().unwrap_or("").to_string();
        self.status = format!("{base}  |  {note}");
    }

    /// Sync note when monitor capture must not touch a radio.
    ///
    /// `None` means the caller should spawn the background sample. A test
    /// sampler also returns `None` so CI can exercise that path without root.
    fn monitor_fast_path(&self) -> Option<(String, bool)> {
        #[cfg(test)]
        if self.monitor.sampler.is_some() {
            return None;
        }
        if matches!(self.source, ScanSource::Demo) {
            return Some(("monitor skipped (demo)".into(), false));
        }
        if !is_root() {
            return Some(("monitor needs root; staying on nl80211/iw".into(), false));
        }
        None
    }

    /// Decide whether this scan should sample the monitor VIF.
    ///
    /// Demo mode and non-root runs return immediately and do not spawn.
    /// A rooted live scan starts (or keeps) one background sample and
    /// returns without waiting for airodump-ng or tshark.
    fn capture_monitor(&mut self) -> (String, bool) {
        if let Some(note) = self.monitor_fast_path() {
            return note;
        }
        let _started = self.spawn_monitor_sample();
        ("monitor sampling…".into(), false)
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
                self.status =
                    format!("Distance calib {mac} → {distance_m} m (rssi_at_1m={ref_dbm:.1})");
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

impl Drop for App {
    fn drop(&mut self) {
        self.monitor.invalidate();
    }
}

/// Run the test sampler when one is installed; otherwise the live tools.
fn produce_sample(
    session: &Mutex<MonitorSession>,
    generation: &AtomicU64,
    gen: u64,
    #[cfg(test)] sampler: Option<MonitorSampler>,
) -> Option<(Vec<WifiDevice>, String)> {
    if generation.load(Ordering::SeqCst) != gen {
        return None;
    }
    #[cfg(test)]
    if let Some(sampler) = sampler {
        return Some(sampler());
    }
    sample_production(session, generation, gen)
}

/// `ensure` the VIF, then airodump-ng and optional tshark. The session
/// mutex is not held across the sample, so the UI can tear the VIF down
/// without waiting out the capture.
fn sample_production(
    session: &Mutex<MonitorSession>,
    generation: &AtomicU64,
    gen: u64,
) -> Option<(Vec<WifiDevice>, String)> {
    if generation.load(Ordering::SeqCst) != gen {
        return None;
    }
    let mon = {
        let mut guard = session.lock().unwrap_or_else(|err| err.into_inner());
        if generation.load(Ordering::SeqCst) != gen {
            guard.teardown();
            return None;
        }
        if let Err(e) = guard.ensure() {
            return Some((Vec::new(), format!("monitor interface failed: {e}")));
        }
        guard.mon_iface().to_string()
    };
    if generation.load(Ordering::SeqCst) != gen {
        session
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .teardown();
        return None;
    }
    Some(sample_passive(&mon))
}

fn sample_passive(mon: &str) -> (Vec<WifiDevice>, String) {
    match sample_airodump(mon, AIRODUMP_SAMPLE) {
        Err(e) => (Vec::new(), format!("monitor error: {e}")),
        Ok((aps, clients)) => {
            let n_ap = aps.len();
            let n_cl = clients.len();
            let mut devices = aps;
            devices.extend(clients);
            let (n_ad, extra) = match sample_tshark_probes(mon, TSHARK_SAMPLE) {
                Ok(adapters) => {
                    let n = adapters.len();
                    devices.extend(adapters);
                    (n, String::new())
                }
                Err(e) => (0, format!("; tshark: {e}")),
            };
            (
                devices,
                format!("monitor: {n_ap} APs, {n_cl} clients, {n_ad} adapters{extra}"),
            )
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
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::thread;

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
    fn help_command_shows_overlay() {
        let mut app = App::new("wlan0".into(), true);
        app.begin_command_mode();
        for ch in "help".chars() {
            app.command_push_char(ch);
        }
        app.execute_command();
        assert!(app.help_overlay);
        app.dismiss_help();
        assert!(!app.help_overlay);
    }

    #[test]
    fn clear_command_clears_selected_calib() {
        let mut app = App::new("wlan0".into(), true);
        app.maybe_scan(true);
        app.select_first();
        app.pin_selected(0.0);
        let mac = app.selected_mac().unwrap().to_string();
        assert!(app.calibration.has_manual_bearing(&mac));
        app.begin_command_mode();
        for ch in "clear".chars() {
            app.command_push_char(ch);
        }
        app.execute_command();
        assert!(app.calibration.manual_bearing(&mac).is_none());
    }

    #[test]
    fn toggle_bearing_calib_starts_and_stops() {
        let mut app = App::new("wlan0".into(), true);
        assert!(!app.bearing_calibrator.active());
        app.toggle_bearing_calib();
        assert!(app.bearing_calibrator.active());
        assert!(app.status.contains("CALIBRATING"));
        app.toggle_bearing_calib();
        assert!(!app.bearing_calibrator.active());
        assert!(app.status.contains("stopped"));
    }

    #[test]
    fn toggle_monitor_flips_flag() {
        let mut app = App::new("wlan0".into(), true);
        assert!(!app.mon_enabled);
        app.toggle_monitor();
        assert!(app.mon_enabled);
        assert!(app.status.contains("Monitor ON"));
        assert!(app.status.contains("demo"));
        assert!(!app.status.contains("flag only"));
        app.toggle_monitor();
        assert!(!app.mon_enabled);
        assert!(app.status.contains("Monitor OFF"));
        assert!(!app.monitor.created());
    }

    #[test]
    fn monitor_capture_demo_does_not_open_a_vif() {
        let mut app = App::new("wlan0".into(), true);
        app.mon_enabled = true;
        let (note, merged) = app.capture_monitor();
        assert!(!merged);
        assert!(note.contains("demo"));
        assert!(!app.monitor.created());
    }

    #[test]
    fn monitor_capture_without_root_stays_on_managed_scan() {
        if wifi_radar_linux::is_root() {
            return;
        }
        let mut app = App::new("wlan0".into(), false);
        app.mon_enabled = true;
        let (note, merged) = app.capture_monitor();
        assert!(!merged);
        assert!(note.contains("root"));
        assert!(note.contains("nl80211"));
        assert!(!app.monitor.created());
        assert!(!app.monitor_sampling());
    }

    #[test]
    fn demo_scan_appends_monitor_note_when_enabled() {
        let mut app = App::new("wlan0".into(), true);
        app.toggle_monitor();
        app.maybe_scan(true);
        assert!(app.status.contains("DEMO"));
        assert!(app.status.contains("demo"));
        assert!(!app.status.contains("flag only"));
        assert!(!app.status.contains("sampling"));
        assert!(!app.monitor_sampling());
        assert!(!app.monitor.created());
    }

    #[test]
    fn monitor_label_is_sampling_only_while_enabled_and_in_flight() {
        let entered = Arc::new(AtomicBool::new(false));
        let release = Arc::new(AtomicBool::new(false));
        let entered_c = Arc::clone(&entered);
        let release_c = Arc::clone(&release);
        let mut app = App::new("wlan0".into(), false);
        app.install_test_sampler(Arc::new(move || {
            entered_c.store(true, Ordering::SeqCst);
            let start = Instant::now();
            while !release_c.load(Ordering::SeqCst) {
                if start.elapsed() > Duration::from_secs(5) {
                    break;
                }
                thread::sleep(Duration::from_millis(5));
            }
            (Vec::new(), "scripted".into())
        }));
        assert_eq!(app.monitor_label(), "off");
        app.mon_enabled = true;
        assert_eq!(app.monitor_label(), "on");
        assert!(app.spawn_monitor_sample());
        while !entered.load(Ordering::SeqCst) {
            thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(app.monitor_label(), "sampling");
        app.toggle_monitor();
        assert_eq!(app.monitor_label(), "off");
        release.store(true, Ordering::SeqCst);
    }

    #[test]
    fn in_flight_monitor_sample_skips_second_spawn() {
        let entered = Arc::new(AtomicBool::new(false));
        let release = Arc::new(AtomicBool::new(false));
        let calls = Arc::new(AtomicUsize::new(0));
        let entered_c = Arc::clone(&entered);
        let release_c = Arc::clone(&release);
        let calls_c = Arc::clone(&calls);
        let mut app = App::new("wlan0".into(), false);
        app.install_test_sampler(Arc::new(move || {
            calls_c.fetch_add(1, Ordering::SeqCst);
            entered_c.store(true, Ordering::SeqCst);
            let start = Instant::now();
            while !release_c.load(Ordering::SeqCst) {
                if start.elapsed() > Duration::from_secs(5) {
                    break;
                }
                thread::sleep(Duration::from_millis(5));
            }
            (Vec::new(), "scripted".into())
        }));
        app.mon_enabled = true;
        assert!(app.spawn_monitor_sample());
        while !entered.load(Ordering::SeqCst) {
            thread::sleep(Duration::from_millis(5));
        }
        assert!(app.monitor_sampling());
        assert!(!app.spawn_monitor_sample());
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        release.store(true, Ordering::SeqCst);
    }

    #[test]
    fn finished_monitor_sample_allows_another_spawn() {
        let calls = Arc::new(AtomicUsize::new(0));
        let calls_c = Arc::clone(&calls);
        let mut app = App::new("wlan0".into(), false);
        app.install_test_sampler(Arc::new(move || {
            calls_c.fetch_add(1, Ordering::SeqCst);
            (Vec::new(), "scripted".into())
        }));
        app.mon_enabled = true;
        assert!(app.spawn_monitor_sample());
        for _ in 0..200 {
            if calls.load(Ordering::SeqCst) >= 1 && !app.monitor_sampling() {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(!app.monitor_sampling());
        assert!(app.spawn_monitor_sample());
        for _ in 0..200 {
            if calls.load(Ordering::SeqCst) >= 2 && !app.monitor_sampling() {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn scripted_monitor_sample_merges_on_poll() {
        let mut app = App::new("wlan0".into(), false);
        app.install_test_sampler(Arc::new(|| {
            let device = WifiDevice::hotspot("bb:bb:bb:00:00:09", -50.0);
            (vec![device], "monitor: 1 APs, 0 clients, 0 adapters".into())
        }));
        app.mon_enabled = true;
        assert!(app.spawn_monitor_sample());
        app.last_scan = Instant::now();
        let mut saw = false;
        for _ in 0..200 {
            app.maybe_scan(false);
            if app.devices.iter().any(|d| d.mac == "bb:bb:bb:00:00:09") {
                saw = true;
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        assert!(saw);
        assert!(app.status.contains("1 APs"));
        assert!(app.store.device_mut("bb:bb:bb:00:00:09").is_some());
    }

    #[test]
    fn late_monitor_sample_after_toggle_off_is_not_merged() {
        let entered = Arc::new(AtomicBool::new(false));
        let release = Arc::new(AtomicBool::new(false));
        let entered_c = Arc::clone(&entered);
        let release_c = Arc::clone(&release);
        let mut app = App::new("wlan0".into(), false);
        app.install_test_sampler(Arc::new(move || {
            entered_c.store(true, Ordering::SeqCst);
            let start = Instant::now();
            while !release_c.load(Ordering::SeqCst) {
                if start.elapsed() > Duration::from_secs(5) {
                    break;
                }
                thread::sleep(Duration::from_millis(5));
            }
            let device = WifiDevice::hotspot("cc:cc:cc:00:00:01", -40.0);
            (vec![device], "monitor: late".into())
        }));
        app.mon_enabled = true;
        assert!(app.spawn_monitor_sample());
        while !entered.load(Ordering::SeqCst) {
            thread::sleep(Duration::from_millis(5));
        }
        app.toggle_monitor();
        assert!(!app.mon_enabled);
        release.store(true, Ordering::SeqCst);
        app.last_scan = Instant::now();
        for _ in 0..200 {
            app.maybe_scan(false);
            thread::sleep(Duration::from_millis(5));
        }
        assert!(app.store.device_mut("cc:cc:cc:00:00:01").is_none());
        assert!(app.devices.iter().all(|d| d.mac != "cc:cc:cc:00:00:01"));
        assert!(!app.status.contains("late"));
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
