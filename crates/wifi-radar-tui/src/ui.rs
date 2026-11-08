//! Ratatui drawing for the MVP list view.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table};
use ratatui::Frame;

use crate::app::App;

/// Draw header, device table, and status/footer.
pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(3),
        ])
        .split(area);

    draw_header(frame, chunks[0], app);
    draw_table(frame, chunks[1], app);
    draw_footer(frame, chunks[2], app);
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
            "v{}  iface={}  backend={}",
            env!("CARGO_PKG_VERSION"),
            app.iface,
            match &app.source {
                crate::app::ScanSource::Demo => "demo",
                crate::app::ScanSource::Iw(_) => "iw-cli",
            }
        )),
    ]);
    let block = Block::default()
        .borders(Borders::ALL)
        .title("Passive observation only — no injection / deauth");
    frame.render_widget(Paragraph::new(title).block(block), area);
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

    let rows = app.devices.iter().enumerate().map(|(i, d)| {
        let style = if i == app.selected {
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
    let help = " j/k=select  r=rescan  q=quit ";
    let text = Line::from(vec![
        Span::raw(app.status.clone()),
        Span::raw("  "),
        Span::styled(help, Style::default().fg(Color::DarkGray)),
    ]);
    frame.render_widget(
        Paragraph::new(text).block(Block::default().borders(Borders::ALL)),
        area,
    );
}
