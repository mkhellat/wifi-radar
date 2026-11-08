//! Ratatui primary UI for wifi-radar (MVP list view).

mod app;
mod help;
mod polar;
mod ui;

use std::env;
use std::io::{self, stdout};
use std::process::Command;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use app::App;

fn main() {
    let args: Vec<String> = env::args().collect();
    let demo = args.iter().any(|a| a == "--demo");
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
    // Prefer Interface lines: "	Interface wlan0"
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
