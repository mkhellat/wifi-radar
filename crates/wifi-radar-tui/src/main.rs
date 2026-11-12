//! Ratatui primary UI for wifi-radar (scan view + fused-track viewer).

mod app;
mod fused;
mod help;
mod polar;
mod ui;

use std::env;
use std::fs::OpenOptions;
use std::io::{self, stdout, IsTerminal, Write};
use std::path::Path;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use app::App;
use fused::FusedApp;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.iter().any(|a| a == "-h" || a == "--help") {
        eprintln!(
            "Usage: wifi-radar-tui [--demo] [-i IFACE]\n\
             \n\
                    wifi-radar-tui --fused [--demo] [TRACK.ndjson...]\n\
             \n\
             Default: live/demo polar scan TUI.\n\
             --fused: view fused-track NDJSON (files, --demo lab, or stdin pipe).\n\
             Pipe: wifi-radar-fuse --listen … | wifi-radar-tui --fused\n\
             Best-effort only — not survey-grade localization."
        );
        std::process::exit(2);
    }

    let fused = args.iter().any(|a| a == "--fused");
    let demo = args.iter().any(|a| a == "--demo");

    if fused {
        let files: Vec<&Path> = args
            .iter()
            .filter(|a| !a.starts_with('-'))
            .map(Path::new)
            .collect();
        if let Err(e) = run_fused(demo, &files) {
            eprintln!("wifi-radar-tui --fused error: {e}");
            std::process::exit(1);
        }
        return;
    }

    let iface = args
        .windows(2)
        .find(|w| w[0] == "-i" || w[0] == "--interface")
        .map(|w| w[1].clone())
        .or_else(detect_iface)
        .unwrap_or_else(|| "wlan0".into());

    if let Err(e) = run(iface, demo) {
        eprintln!("wifi-radar-tui error: {e}");
        std::process::exit(1);
    }
}

fn detect_iface() -> Option<String> {
    let out = Command::new("iw").arg("dev").output().ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    for line in text.lines() {
        let t = line.trim();
        if let Some(name) = t.strip_prefix("Interface ") {
            return Some(name.trim().to_string());
        }
    }
    None
}

fn run(iface: String, demo: bool) -> io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(iface, demo);
    app.maybe_scan(true);

    let result = event_loop(&mut terminal, &mut app);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    result
}

fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> io::Result<()> {
    loop {
        app.maybe_scan(false);
        terminal.draw(|f| ui::draw(f, app))?;

        if event::poll(Duration::from_millis(200))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                if app.help_overlay {
                    app.dismiss_help();
                    continue;
                }
                if app.command_mode {
                    match key.code {
                        KeyCode::Esc => app.cancel_command_mode(),
                        KeyCode::Enter => app.execute_command(),
                        KeyCode::Backspace => app.command_backspace(),
                        KeyCode::Char(ch) => app.command_push_char(ch),
                        _ => {}
                    }
                } else {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Char('Q') => app.should_quit = true,
                        KeyCode::Char('r') | KeyCode::Char('R') => app.maybe_scan(true),
                        KeyCode::Char('j') | KeyCode::Down => app.select_next(),
                        KeyCode::Char('k') | KeyCode::Up => app.select_prev(),
                        KeyCode::Char('g') => app.select_first(),
                        KeyCode::Char('G') => app.select_last(),
                        KeyCode::Esc => app.clear_selection(),
                        KeyCode::Char('h') | KeyCode::Left => app.rotate_heading(-5.0),
                        KeyCode::Char('l') | KeyCode::Right => app.rotate_heading(5.0),
                        KeyCode::Char('H') => app.rotate_heading(-15.0),
                        KeyCode::Char('L') => app.rotate_heading(15.0),
                        KeyCode::Char('8') => app.pin_selected(0.0),
                        KeyCode::Char('6') => app.pin_selected(90.0),
                        KeyCode::Char('2') => app.pin_selected(180.0),
                        KeyCode::Char('4') => app.pin_selected(-90.0),
                        KeyCode::Char('D') => app.calibrate_distance_selected(0.25),
                        KeyCode::Char('d') => app.calibrate_distance_selected(1.0),
                        KeyCode::Char('x') | KeyCode::Char('X') => {
                            app.clear_calibration_selected()
                        }
                        KeyCode::Char('?') => app.show_help(),
                        KeyCode::Char('c') => app.toggle_bearing_calib(),
                        KeyCode::Char('m') => app.toggle_monitor(),
                        KeyCode::Char(':') => app.begin_command_mode(),
                        _ => {}
                    }
                }
            }
        }

        if app.should_quit {
            break;
        }
    }
    Ok(())
}

fn run_fused(demo: bool, files: &[&Path]) -> Result<(), String> {
    let mut app = FusedApp::new("fused viewer — best-effort 2D, not survey-grade");
    let status_shared: Arc<Mutex<String>> = Arc::new(Mutex::new(app.status.clone()));

    if demo {
        let tracks = fused::demo_lab_tracks()?;
        app.set_tracks(tracks);
        app.status = "fused demo — three-node lab mlat (not survey-grade)".into();
    } else if !files.is_empty() {
        let tracks = fused::load_track_files(files)?;
        let n = tracks.len();
        app.set_tracks(tracks);
        app.status = format!("loaded {n} track(s) from files — not survey-grade");
    } else if io::stdin().is_terminal() {
        return Err(
            "stdin is a TTY: pass TRACK.ndjson files, --demo, or pipe fuse NDJSON".into(),
        );
    } else {
        fused::spawn_stdin_track_reader(Arc::clone(&app.tracks), Arc::clone(&status_shared));
        app.status = "reading fused NDJSON from stdin — not survey-grade".into();
    }

    // Keyboard/display on /dev/tty so stdin can be the fuse pipe.
    let tty = OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/tty")
        .map_err(|e| format!("open /dev/tty: {e}"))?;
    let mut tty_out = tty
        .try_clone()
        .map_err(|e| format!("clone /dev/tty: {e}"))?;
    // Keep the original fd alive for the process lifetime.
    let _tty_hold = tty;

    enable_raw_mode().map_err(|e| e.to_string())?;
    execute!(tty_out, EnterAlternateScreen).map_err(|e| e.to_string())?;
    let backend = CrosstermBackend::new(tty_out);
    let mut terminal = Terminal::new(backend).map_err(|e| e.to_string())?;

    let result = fused_event_loop(&mut terminal, &mut app, &status_shared);

    disable_raw_mode().map_err(|e| e.to_string())?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen).map_err(|e| e.to_string())?;
    terminal.show_cursor().map_err(|e| e.to_string())?;
    let _ = terminal.backend_mut().flush();
    result.map_err(|e| e.to_string())
}

fn fused_event_loop<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    app: &mut FusedApp,
    status_shared: &Arc<Mutex<String>>,
) -> io::Result<()> {
    loop {
        if let Ok(s) = status_shared.lock() {
            if *s != app.status {
                app.status = s.clone();
            }
        }
        terminal.draw(|f| fused::draw(f, app))?;

        if event::poll(Duration::from_millis(200))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                if app.help_overlay {
                    app.dismiss_help();
                    continue;
                }
                match key.code {
                    KeyCode::Char('q') | KeyCode::Char('Q') => app.should_quit = true,
                    KeyCode::Char('j') | KeyCode::Down => app.select_next(),
                    KeyCode::Char('k') | KeyCode::Up => app.select_prev(),
                    KeyCode::Char('h') | KeyCode::Left => app.rotate_heading(-5.0),
                    KeyCode::Char('l') | KeyCode::Right => app.rotate_heading(5.0),
                    KeyCode::Char('H') => app.rotate_heading(-15.0),
                    KeyCode::Char('L') => app.rotate_heading(15.0),
                    KeyCode::Esc => app.selected_mac = None,
                    KeyCode::Char('?') => app.show_help(),
                    _ => {}
                }
            }
        }

        if app.should_quit {
            break;
        }
    }
    Ok(())
}
