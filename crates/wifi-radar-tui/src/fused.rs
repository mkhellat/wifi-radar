//! Fused-track viewer mode (stdin / file NDJSON from wifi-radar-fuse).

use std::fs;
use std::io::{self, BufRead, BufReader};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table};
use ratatui::Frame;
use wifi_radar_core::DEFAULT_FUSION_WINDOW_SEC;
use wifi_radar_fuse::{
    fuse_batches, load_observation_files, parse_track_ndjson_line, parse_tracks_ndjson, FusedTrack,
    SolverKind,
};

use crate::polar::{self, ScaleState};

/// Idle silence after which buffered stdin lines become the active track set.
pub const FUSED_EPOCH_IDLE_MS: u64 = 80;

/// Mutable state for `--fused` polar viewer.
pub struct FusedApp {
    /// Latest fused tracks (shared with stdin reader when piping).
    pub tracks: Arc<Mutex<Vec<FusedTrack>>>,
    pub selected_mac: Option<String>,
    pub heading_deg: f64,
    pub scale: ScaleState,
    pub status: String,
    pub should_quit: bool,
    pub help_overlay: bool,
}

impl FusedApp {
    /// Empty viewer with a status line.
    pub fn new(status: impl Into<String>) -> Self {
        Self {
            tracks: Arc::new(Mutex::new(Vec::new())),
            selected_mac: None,
            heading_deg: 0.0,
            scale: ScaleState::default(),
            status: status.into(),
            should_quit: false,
            help_overlay: false,
        }
    }

    /// Replace the active track set.
    pub fn set_tracks(&self, tracks: Vec<FusedTrack>) {
        if let Ok(mut g) = self.tracks.lock() {
            *g = tracks;
        }
    }

    /// Snapshot tracks for drawing / selection.
    pub fn snapshot(&self) -> Vec<FusedTrack> {
        self.tracks.lock().map(|g| g.clone()).unwrap_or_default()
    }

    /// Sorted MAC list for j/k navigation.
    pub fn macs(&self) -> Vec<String> {
        let mut macs: Vec<_> = self.snapshot().into_iter().map(|t| t.mac).collect();
        macs.sort();
        macs.dedup();
        macs
    }

    pub fn select_next(&mut self) {
        let macs = self.macs();
        if macs.is_empty() {
            self.selected_mac = None;
            return;
        }
        let idx = self
            .selected_mac
            .as_ref()
            .and_then(|m| macs.iter().position(|x| x == m))
            .map(|i| (i + 1) % macs.len())
            .unwrap_or(0);
        self.selected_mac = Some(macs[idx].clone());
    }

    pub fn select_prev(&mut self) {
        let macs = self.macs();
        if macs.is_empty() {
            self.selected_mac = None;
            return;
        }
        let idx = self
            .selected_mac
            .as_ref()
            .and_then(|m| macs.iter().position(|x| x == m))
            .map(|i| if i == 0 { macs.len() - 1 } else { i - 1 })
            .unwrap_or(0);
        self.selected_mac = Some(macs[idx].clone());
    }

    pub fn rotate_heading(&mut self, delta: f64) {
        self.heading_deg = (self.heading_deg + delta).rem_euclid(360.0);
    }

    pub fn show_help(&mut self) {
        self.help_overlay = true;
    }

    pub fn dismiss_help(&mut self) {
        self.help_overlay = false;
    }
}

/// Load fused-track NDJSON from files (one or more tracks per file).
pub fn load_track_files(paths: &[&Path]) -> Result<Vec<FusedTrack>, String> {
    let mut out = Vec::new();
    for path in paths {
        let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        out.extend(parse_tracks_ndjson(&text)?);
    }
    Ok(out)
}

/// Build demo tracks from the three-node lab observation fixtures.
pub fn demo_lab_tracks() -> Result<Vec<FusedTrack>, String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../wifi-radar-core/tests/fixtures");
    let paths = [
        root.join("three_node_lab_node_a.json"),
        root.join("three_node_lab_node_b.json"),
        root.join("three_node_lab_node_c.json"),
    ];
    let refs: Vec<&Path> = paths.iter().map(|p| p.as_path()).collect();
    let batches = load_observation_files(&refs)?;
    Ok(fuse_batches(
        &batches,
        DEFAULT_FUSION_WINDOW_SEC,
        SolverKind::Multilateration,
    ))
}

fn commit_epoch(
    tracks: &Arc<Mutex<Vec<FusedTrack>>>,
    status: &Arc<Mutex<String>>,
    epoch: &mut Vec<FusedTrack>,
) {
    let n = epoch.len();
    if let Ok(mut g) = tracks.lock() {
        *g = std::mem::take(epoch);
    }
    if let Ok(mut s) = status.lock() {
        *s = format!("fused epoch: {n} track(s) — best-effort 2D, not survey-grade");
    }
}

/// Spawn a stdin reader that groups NDJSON lines into epochs via idle silence.
///
/// Keyboard I/O must use `/dev/tty` when this is active (stdin is the pipe).
pub fn spawn_stdin_track_reader(tracks: Arc<Mutex<Vec<FusedTrack>>>, status: Arc<Mutex<String>>) {
    let pending: Arc<Mutex<Vec<FusedTrack>>> = Arc::new(Mutex::new(Vec::new()));
    let last_line: Arc<Mutex<Instant>> = Arc::new(Mutex::new(Instant::now()));
    let pending_flush = Arc::clone(&pending);
    let last_flush = Arc::clone(&last_line);
    let tracks_flush = Arc::clone(&tracks);
    let status_flush = Arc::clone(&status);

    thread::spawn(move || loop {
        thread::sleep(Duration::from_millis(FUSED_EPOCH_IDLE_MS / 2));
        let elapsed = last_flush
            .lock()
            .map(|t| t.elapsed())
            .unwrap_or(Duration::ZERO);
        if elapsed < Duration::from_millis(FUSED_EPOCH_IDLE_MS) {
            continue;
        }
        let mut epoch = match pending_flush.lock() {
            Ok(mut g) if !g.is_empty() => std::mem::take(&mut *g),
            _ => continue,
        };
        commit_epoch(&tracks_flush, &status_flush, &mut epoch);
    });

    thread::spawn(move || {
        let stdin = io::stdin();
        let mut reader = BufReader::new(stdin.lock());
        let mut buf = String::new();
        loop {
            buf.clear();
            match reader.read_line(&mut buf) {
                Ok(0) => {
                    let mut epoch = pending.lock().map(|mut g| std::mem::take(&mut *g)).unwrap_or_default();
                    if !epoch.is_empty() {
                        commit_epoch(&tracks, &status, &mut epoch);
                    }
                    if let Ok(mut s) = status.lock() {
                        if !s.contains("[stdin EOF]") {
                            s.push_str("  [stdin EOF]");
                        }
                    }
                    break;
                }
                Ok(_) => {
                    let line = buf.trim_end();
                    if line.is_empty() {
                        continue;
                    }
                    match parse_track_ndjson_line(line) {
                        Ok(t) => {
                            if let Ok(mut g) = pending.lock() {
                                g.push(t);
                            }
                            if let Ok(mut t) = last_line.lock() {
                                *t = Instant::now();
                            }
                        }
                        Err(e) => {
                            if let Ok(mut s) = status.lock() {
                                *s = format!("stdin parse: {e}");
                            }
                        }
                    }
                }
                Err(e) => {
                    if let Ok(mut s) = status.lock() {
                        *s = format!("stdin: {e}");
                    }
                    break;
                }
            }
        }
    });
}

/// Draw fused-track UI (header, polar, table, footer).
pub fn draw(frame: &mut Frame, app: &mut FusedApp) {
    if app.help_overlay {
        let text = vec![
            Line::from("Fused-track viewer"),
            Line::from("  q quit   h/l heading   j/k select   ? help"),
            Line::from("  Pipe: wifi-radar-fuse --listen … | wifi-radar-tui --fused"),
            Line::from("  Best-effort 2D only — not survey-grade localization."),
        ];
        frame.render_widget(
            Paragraph::new(text).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Help — any key to return"),
            ),
            frame.area(),
        );
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Percentage(55),
            Constraint::Min(5),
            Constraint::Length(3),
        ])
        .split(frame.area());

    draw_header(frame, chunks[0], app);
    draw_polar(frame, chunks[1], app);
    draw_table(frame, chunks[2], app);
    draw_footer(frame, chunks[3], app);
}

fn draw_header(frame: &mut Frame, area: Rect, app: &FusedApp) {
    let n = app.snapshot().len();
    let title = Line::from(vec![
        Span::styled(
            " wifi-radar-tui --fused ",
            Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!(
            "v{}  tracks={n}  hdg={:.0}°",
            env!("CARGO_PKG_VERSION"),
            app.heading_deg
        )),
    ]);
    frame.render_widget(
        Paragraph::new(title).block(
            Block::default()
                .borders(Borders::ALL)
                .title("Best-effort 2D fused tracks — not survey-grade"),
        ),
        area,
    );
}

fn draw_polar(frame: &mut Frame, area: Rect, app: &mut FusedApp) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title("Fused polar  h/l=heading  j/k=select");
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let tracks = app.snapshot();
    let selected = app.selected_mac.clone();
    polar::paint_fused_tracks(
        frame.buffer_mut(),
        inner,
        &tracks,
        app.heading_deg,
        selected.as_deref(),
        &mut app.scale,
    );
}

fn draw_table(frame: &mut Frame, area: Rect, app: &FusedApp) {
    let header = Row::new([
        Cell::from("MAC"),
        Cell::from("X"),
        Cell::from("Y"),
        Cell::from("r(m)"),
        Cell::from("conf"),
        Cell::from("rms"),
        Cell::from("nodes"),
        Cell::from("solver"),
        Cell::from("status"),
    ])
    .style(Style::default().add_modifier(Modifier::BOLD));

    let tracks = app.snapshot();
    let rows = tracks.iter().map(|t| {
        let (range, _) = polar::xy_to_range_bearing(t.x, t.y);
        let selected = app.selected_mac.as_deref() == Some(t.mac.as_str());
        let style = if selected {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        };
        Row::new([
            Cell::from(t.mac.clone()),
            Cell::from(format!("{:.2}", t.x)),
            Cell::from(format!("{:.2}", t.y)),
            Cell::from(format!("{range:.2}")),
            Cell::from(format!("{:.2}", t.confidence)),
            Cell::from(format!("{:.2}", t.residual_rms_m)),
            Cell::from(t.contributing_nodes.to_string()),
            Cell::from(t.solver.clone()),
            Cell::from(t.status.clone()),
        ])
        .style(style)
    });

    let table = Table::new(
        rows,
        [
            Constraint::Length(18),
            Constraint::Length(7),
            Constraint::Length(7),
            Constraint::Length(7),
            Constraint::Length(6),
            Constraint::Length(6),
            Constraint::Length(6),
            Constraint::Length(10),
            Constraint::Min(8),
        ],
    )
    .header(header)
    .block(Block::default().borders(Borders::ALL).title("Tracks"));

    frame.render_widget(table, area);
}

fn draw_footer(frame: &mut Frame, area: Rect, app: &FusedApp) {
    let line = Line::from(app.status.clone());
    frame.render_widget(
        Paragraph::new(line).block(
            Block::default()
                .borders(Borders::ALL)
                .title("q quit  ? help"),
        ),
        area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_lab_tracks_recover_target() {
        let tracks = demo_lab_tracks().unwrap();
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].mac, "aa:aa:aa:00:00:10");
        assert!((tracks[0].x - 2.0).abs() < 0.05);
        assert!((tracks[0].y - 1.0).abs() < 0.05);
    }

    #[test]
    fn fused_app_selection_cycles() {
        let mut app = FusedApp::new("test");
        app.set_tracks(vec![
            FusedTrack {
                mac: "aa:aa:aa:00:00:01".into(),
                x: 1.0,
                y: 0.0,
                confidence: 0.5,
                residual_rms_m: 0.1,
                contributing_nodes: 2,
                solver: "mlat".into(),
                status: "ok".into(),
            },
            FusedTrack {
                mac: "aa:aa:aa:00:00:02".into(),
                x: 0.0,
                y: 2.0,
                confidence: 0.5,
                residual_rms_m: 0.1,
                contributing_nodes: 2,
                solver: "mlat".into(),
                status: "ok".into(),
            },
        ]);
        app.select_next();
        assert_eq!(app.selected_mac.as_deref(), Some("aa:aa:aa:00:00:01"));
        app.select_next();
        assert_eq!(app.selected_mac.as_deref(), Some("aa:aa:aa:00:00:02"));
    }
}
