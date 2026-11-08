//! Ratatui drawing: polar canvas + device list + status.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table};
use ratatui::Frame;

use crate::app::App;
use crate::help;
use crate::polar;

/// Draw header, polar canvas, device table, and status/footer.
pub fn draw(frame: &mut Frame, app: &mut App) {
    if app.help_overlay {
        draw_help(frame, app);
        return;
    }
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Percentage(55),
            Constraint::Min(5),
            Constraint::Length(3),
        ])
        .split(area);

    draw_header(frame, chunks[0], app);
    draw_polar(frame, chunks[1], app);
    draw_table(frame, chunks[2], app);
    draw_footer(frame, chunks[3], app);
}

fn draw_help(frame: &mut Frame, app: &App) {
    let lines = help::help_lines(app.selected_mac().is_some(), app.calibration.mode());
    let text: Vec<Line> = lines.into_iter().map(Line::from).collect();
    let block = Block::default()
        .borders(Borders::ALL)
        .title("Help — any key to return");
    frame.render_widget(
        Paragraph::new(text)
            .block(block)
            .style(Style::default().fg(Color::White)),
        frame.area(),
    );
}

fn draw_header(frame: &mut Frame, area: Rect, app: &App) {
    let title = Line::from(vec![
        Span::styled(
            " wifi-radar-tui ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!(
            "v{}  iface={}  backend={}  hdg={:.0}°  {}",
            env!("CARGO_PKG_VERSION"),
            app.iface,
            app.backend_label(),
            app.heading_deg,
            app.correction.short_label()
        )),
    ]);
    let block = Block::default()
        .borders(Borders::ALL)
        .title("Passive observation only — no injection / deauth");
    frame.render_widget(Paragraph::new(title).block(block), area);
}

fn draw_polar(frame: &mut Frame, area: Rect, app: &mut App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title("Polar  h/l=heading  j/k=select");
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let selected = app.selected_mac().map(str::to_string);
    polar::paint_polar(
        frame.buffer_mut(),
        inner,
        &app.devices,
        app.heading_deg,
        selected.as_deref(),
        &mut app.scale,
    );
}

fn draw_table(frame: &mut Frame, area: Rect, app: &App) {
    let header = Row::new([
        Cell::from("MAC"),
        Cell::from("SSID"),
        Cell::from("Ch"),
        Cell::from("RSSI"),
        Cell::from("Dist"),
        Cell::from("Brg"),
    ])
    .style(Style::default().add_modifier(Modifier::BOLD))
    .bottom_margin(0);

    let selected_idx = app.selected_index();
    let rows = app.devices.iter().enumerate().map(|(i, d)| {
        let style = if Some(i) == selected_idx {
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        Row::new([
            Cell::from(d.mac.clone()),
            Cell::from(d.label()),
            Cell::from(if d.channel == 0 {
                "?".into()
            } else {
                d.channel.to_string()
            }),
            Cell::from(format!("{:.0}", d.rssi_dbm)),
            Cell::from(format!("{:.1}m", d.distance_m())),
            Cell::from(format!("{:.0}°", d.display_bearing())),
        ])
        .style(style)
    });

    let table = Table::new(
        rows,
        [
            Constraint::Length(18),
            Constraint::Min(12),
            Constraint::Length(4),
            Constraint::Length(6),
            Constraint::Length(7),
            Constraint::Length(6),
        ],
    )
    .header(header)
    .block(Block::default().borders(Borders::ALL).title("Devices"));

    frame.render_widget(table, area);
}

fn draw_footer(frame: &mut Frame, area: Rect, app: &App) {
    let text = if app.command_mode {
        Line::from(vec![
            Span::styled(
                format!(":{}", app.command_buf),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("█", Style::default().fg(Color::Yellow)),
            Span::styled(
                "  Enter=run  Esc=cancel  (:mode :clear :help)",
                Style::default().fg(Color::DarkGray),
            ),
        ])
    } else {
        let help = if app.selected_mac().is_some() {
            " ?=help  :=cmd  8/4/6/2=pin  D/d=dist  x=clear  h/l  j/k  Esc  r  q "
        } else {
            " ?=help  :=cmd  h/l=hdg  j/k=select  g/G  Esc  r=rescan  q "
        };
        Line::from(vec![
            Span::raw(app.status.clone()),
            Span::raw("  "),
            Span::styled(help, Style::default().fg(Color::DarkGray)),
        ])
    };
    frame.render_widget(
        Paragraph::new(text).block(Block::default().borders(Borders::ALL)),
        area,
    );
}
