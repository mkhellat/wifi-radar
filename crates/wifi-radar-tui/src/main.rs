//! Ratatui entry point (stub until the radar shell is ported).

fn main() {
    println!(
        "wifi-radar-tui {} (core {}, linux {}) — stub; Ratatui shell not yet wired",
        env!("CARGO_PKG_VERSION"),
        wifi_radar_core::VERSION,
        wifi_radar_linux::VERSION,
    );
}
