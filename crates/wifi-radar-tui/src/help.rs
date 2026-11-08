//! Help overlay text (Python `_build_help_lines` parity).

use wifi_radar_core::CalibrationMode;

/// Build sectioned help lines for the current selection/mode.
pub fn help_lines(selected: bool, mode: CalibrationMode) -> Vec<String> {
    let mode_label = match mode {
        CalibrationMode::Honest => "honest",
        CalibrationMode::Anchor => "anchor",
    };
    let selected_state = if selected {
        "selected device: yes"
    } else {
        "selected device: no"
    };
    let selected_actions: &[&str] = if selected {
        &[
            "  8 / 4 / 6 / 2  Pin ahead / left / right / behind",
            "  D / d          Calibrate distance at ~0.25m / ~1m",
            "  x              Release saved calibration",
        ]
    } else {
        &[
            "  Select a device with j / k first",
            "  Then use pin / distance / clear actions",
        ]
    };

    let mut lines = vec![
        "wifi-radar-tui help".into(),
        format!("mode: {mode_label}    {selected_state}"),
        String::new(),
        "NAVIGATION".into(),
        "  h / Left       Rotate heading left (5 deg)".into(),
        "  l / Right      Rotate heading right (5 deg)".into(),
        "  H / L          Rotate heading fast (15 deg)".into(),
        "  j / Down       Select next device (weaker)".into(),
        "  k / Up         Select previous device (stronger)".into(),
        "  g / G          Jump to first / last device".into(),
        "  Esc            Deselect or cancel command / help".into(),
        String::new(),
        "GLOBAL ACTIONS".into(),
        "  q              Quit".into(),
        "  r              Force immediate rescan".into(),
        "  m              Toggle monitor mode (stub until wired)".into(),
        "  c              Start/stop bearing calibration".into(),
        "  ?              Show this help screen".into(),
        String::new(),
        "SELECTED DEVICE".into(),
    ];
    lines.extend(selected_actions.iter().map(|s| (*s).to_string()));
    lines.extend([
        String::new(),
        "COMMAND MODE".into(),
        "  :q   :quit     Quit".into(),
        "  :r   :rescan   Force rescan".into(),
        "  :m   :monitor  Toggle monitor".into(),
        "  :c   :calib    Toggle calibration".into(),
        "  :x   :clear    Release selected calibration".into(),
        "  :mode honest   Conservative scene correction".into(),
        "  :mode anchor   Heuristic anchor propagation".into(),
        "  :h   :help     Show help overlay".into(),
        String::new(),
        "DISPLAY".into(),
        "  rings          Auto-scaled distance reference rings".into(),
        "  N/E/S/W        Compass markers relative to heading".into(),
        "  sweep          Your forward direction".into(),
        "  <anchor>       Device contributes to scene correction".into(),
        "  <stale-cal>    Saved calibration rejected as stale".into(),
        String::new(),
        "CALIBRATION NOTES".into(),
        "  Rotation calibration needs visible RSSI variation.".into(),
        "  Manual pin and distance cal persist across runs.".into(),
        "  Use x or :clear when a calibrated device has moved.".into(),
        String::new(),
        "Press any key to return to radar".into(),
    ]);
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mentions_mode_and_dismiss_hint() {
        let lines = help_lines(false, CalibrationMode::Honest);
        let joined = lines.join("\n");
        assert!(joined.contains("mode: honest"));
        assert!(joined.contains("selected device: no"));
        assert!(joined.contains("Press any key"));
        assert!(joined.contains(":mode anchor"));
    }

    #[test]
    fn selected_shows_pin_keys() {
        let lines = help_lines(true, CalibrationMode::Anchor);
        let joined = lines.join("\n");
        assert!(joined.contains("mode: anchor"));
        assert!(joined.contains("8 / 4 / 6 / 2"));
    }
}
