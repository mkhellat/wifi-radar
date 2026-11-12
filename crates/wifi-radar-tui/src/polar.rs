//! Polar radar geometry (parity with Python `ui/radar.py`).
//!
//! Pure mapping helpers plus a Ratatui buffer painter. Heading keys and
//! calibration land in follow-up PRs; this module accepts `heading` so the
//! canvas API stays stable.

use wifi_radar_core::{DeviceKind, WifiDevice};

/// Terminal character aspect ratio (height/width), matches Python `ASPECT`.
pub const ASPECT: f64 = 0.48;

/// Scale presets: `(max_distance_m, [(fraction, label), ...])`.
pub const SCALE_PRESETS: &[(f64, &[(f64, &str)])] = &[
    (10.0, &[(0.33, "1m"), (0.66, "5m"), (1.0, "10m")]),
    (30.0, &[(0.33, "3m"), (0.66, "10m"), (1.0, "30m")]),
    (
        100.0,
        &[(0.25, "5m"), (0.5, "20m"), (0.75, "50m"), (1.0, "100m")],
    ),
    (
        300.0,
        &[
            (0.25, "10m"),
            (0.5, "50m"),
            (0.75, "150m"),
            (1.0, "300m"),
        ],
    ),
];

/// Auto-scale state with hysteresis (avoids ring preset flicker).
#[derive(Debug, Clone)]
pub struct ScaleState {
    last_max_m: Option<f64>,
}

impl Default for ScaleState {
    fn default() -> Self {
        Self { last_max_m: None }
    }
}

impl ScaleState {
    /// Choose ring definitions and scale max from device distances.
    pub fn pick(&mut self, devices: &[WifiDevice]) -> (Vec<(f64, String)>, f64) {
        let ranges: Vec<f64> = devices.iter().map(|d| d.distance_m()).collect();
        self.pick_ranges(&ranges)
    }

    /// Choose ring definitions from raw range samples (metres).
    pub fn pick_ranges(&mut self, ranges: &[f64]) -> (Vec<(f64, String)>, f64) {
        if ranges.is_empty() {
            let threshold = self.last_max_m.unwrap_or(30.0);
            for &(t, rings) in SCALE_PRESETS {
                if (t - threshold).abs() < f64::EPSILON
                    || (self.last_max_m.is_none() && (t - 30.0).abs() < f64::EPSILON)
                {
                    return (
                        rings.iter().map(|(f, l)| (*f, (*l).to_string())).collect(),
                        t,
                    );
                }
            }
            let (t, rings) = SCALE_PRESETS[1];
            return (
                rings.iter().map(|(f, l)| (*f, (*l).to_string())).collect(),
                t,
            );
        }

        let max_dist = ranges.iter().copied().fold(0.0_f64, f64::max);

        let mut chosen = SCALE_PRESETS[SCALE_PRESETS.len() - 1].0;
        let mut rings = SCALE_PRESETS[SCALE_PRESETS.len() - 1].1;
        for &(threshold, ring_defs) in SCALE_PRESETS {
            if max_dist <= threshold * 1.1 {
                chosen = threshold;
                rings = ring_defs;
                break;
            }
        }

        if let Some(last) = self.last_max_m {
            if (chosen - last).abs() < f64::EPSILON {
                self.last_max_m = Some(chosen);
            } else {
                let zoom_out = max_dist > last * 1.25 && chosen > last;
                let zoom_in = max_dist < last * 0.45 && chosen < last;
                if zoom_out || zoom_in {
                    self.last_max_m = Some(chosen);
                } else {
                    for &(threshold, ring_defs) in SCALE_PRESETS {
                        if (threshold - last).abs() < f64::EPSILON {
                            return (
                                ring_defs
                                    .iter()
                                    .map(|(f, l)| (*f, (*l).to_string()))
                                    .collect(),
                                last,
                            );
                        }
                    }
                }
            }
        } else {
            self.last_max_m = Some(chosen);
        }

        (
            rings.iter().map(|(f, l)| (*f, (*l).to_string())).collect(),
            self.last_max_m.unwrap_or(chosen),
        )
    }
}

/// Local frame (`x` east, `y` north) → range (m) and world bearing (deg, 0 = +Y).
pub fn xy_to_range_bearing(x: f64, y: f64) -> (f64, f64) {
    let range = (x * x + y * y).sqrt();
    let bearing = x.atan2(y).to_degrees().rem_euclid(360.0);
    (range, bearing)
}

/// Map estimated metres to polar radius (centre = you, edge = `scale_max_m`).
pub fn distance_to_radius(distance_m: f64, scale_max_m: f64, max_radius: f64) -> f64 {
    if scale_max_m <= 0.0 {
        return max_radius;
    }
    let t = (distance_m / scale_max_m).clamp(0.0, 1.0);
    max_radius * t
}

/// Convert relative bearing (deg, 0 = up/heading) and radius to buffer cells.
pub fn polar_to_cell(cx: u16, cy: u16, bearing_rel_deg: f64, radius: f64) -> (u16, u16) {
    let rad = (bearing_rel_deg - 90.0).to_radians();
    let x = cx as f64 + radius * rad.cos();
    let y = cy as f64 + radius * rad.sin() * ASPECT;
    (x.round().clamp(0.0, u16::MAX as f64) as u16, y.round().clamp(0.0, u16::MAX as f64) as u16)
}

/// Glyph for device kind (Python `KIND_GLYPH` parity).
pub fn kind_glyph(kind: DeviceKind) -> &'static str {
    match kind {
        DeviceKind::Hotspot => "◉",
        DeviceKind::Client => "◎",
        DeviceKind::Adapter => "○",
    }
}

/// Paint polar rings, sweep, compass, and devices into a Ratatui buffer area.
pub fn paint_polar(
    buf: &mut ratatui::buffer::Buffer,
    area: ratatui::layout::Rect,
    devices: &[WifiDevice],
    heading_deg: f64,
    selected_mac: Option<&str>,
    scale: &mut ScaleState,
) {
    use ratatui::style::{Color, Modifier, Style};

    if area.width < 20 || area.height < 8 {
        buf.set_string(
            area.x,
            area.y,
            "polar: area too small",
            Style::default().fg(Color::DarkGray),
        );
        return;
    }

    let cx = area.x + area.width / 2;
    let cy = area.y + area.height / 2;
    let max_r = {
        let by_x = (area.width / 2).saturating_sub(4) as f64;
        let by_y = ((area.height / 2).saturating_sub(1) as f64) / ASPECT;
        by_x.min(by_y).max(4.0).min(28.0)
    };

    let dim = Style::default().fg(Color::DarkGray);
    let bold = Style::default().add_modifier(Modifier::BOLD);

    // Crosshair
    let max_ri = max_r as i32;
    for i in -max_ri..=max_ri {
        let x = (cx as i32 + i) as u16;
        if in_area(area, x, cy) {
            buf.set_string(x, cy, "─", dim);
        }
        let y = (cy as f64 + f64::from(i) * ASPECT).round() as u16;
        if in_area(area, cx, y) {
            buf.set_string(cx, y, "│", dim);
        }
    }
    if in_area(area, cx, cy) {
        buf.set_string(cx, cy, "┼", dim);
    }

    let (ring_defs, scale_max_m) = scale.pick(devices);
    for (frac, label) in &ring_defs {
        let ring = max_r * frac;
        for deg in (0..360).step_by(2) {
            let (x, y) = polar_to_cell(cx, cy, f64::from(deg), ring);
            if in_area(area, x, y) {
                buf.set_string(x, y, "∙", dim);
            }
        }
        let (lx, ly) = polar_to_cell(cx, cy, 45.0, ring);
        if in_area(area, lx.saturating_add(1), ly) {
            buf.set_string(lx.saturating_add(1), ly, label, dim);
        }
    }

    // Compass relative to heading
    for (deg, lbl) in [(0.0, "N"), (90.0, "E"), (180.0, "S"), (270.0, "W")] {
        let rel = (deg - heading_deg).rem_euclid(360.0);
        let (x, y) = polar_to_cell(cx, cy, rel, max_r + 2.0);
        if in_area(area, x, y) {
            buf.set_string(x, y, lbl, bold);
        }
    }

    // Sweep (visual "forward" = up)
    for step in 1..=(max_r as i32) {
        let t = f64::from(step) / max_r;
        let (x, y) = polar_to_cell(cx, cy, 0.0, max_r * t);
        if in_area(area, x, y) {
            let ch = if step == max_r as i32 { "▲" } else { "│" };
            buf.set_string(x, y, ch, Style::default().fg(Color::Green).add_modifier(Modifier::BOLD));
        }
    }

    for d in devices.iter().take(40) {
        let bearing = (d.display_bearing() - heading_deg).rem_euclid(360.0);
        let dist = distance_to_radius(d.distance_m(), scale_max_m, max_r);
        let (x, y) = polar_to_cell(cx, cy, bearing, dist);
        if !in_area(area, x, y) {
            continue;
        }
        let mut style = match d.kind {
            DeviceKind::Hotspot => Style::default().fg(Color::Cyan),
            DeviceKind::Client => Style::default().fg(Color::Green),
            DeviceKind::Adapter => Style::default().fg(Color::Yellow),
        };
        if selected_mac == Some(d.mac.as_str()) {
            style = style.add_modifier(Modifier::REVERSED | Modifier::BOLD);
        }
        buf.set_string(x, y, kind_glyph(d.kind), style);
    }
}

/// Paint polar rings and fused 2D tracks (`x`/`y` metres in the shared frame).
pub fn paint_fused_tracks(
    buf: &mut ratatui::buffer::Buffer,
    area: ratatui::layout::Rect,
    tracks: &[wifi_radar_fuse::FusedTrack],
    heading_deg: f64,
    selected_mac: Option<&str>,
    scale: &mut ScaleState,
) {
    use ratatui::style::{Color, Modifier, Style};

    if area.width < 20 || area.height < 8 {
        buf.set_string(
            area.x,
            area.y,
            "polar: area too small",
            Style::default().fg(Color::DarkGray),
        );
        return;
    }

    let cx = area.x + area.width / 2;
    let cy = area.y + area.height / 2;
    let max_r = {
        let by_x = (area.width / 2).saturating_sub(4) as f64;
        let by_y = ((area.height / 2).saturating_sub(1) as f64) / ASPECT;
        by_x.min(by_y).max(4.0).min(28.0)
    };

    let dim = Style::default().fg(Color::DarkGray);
    let bold = Style::default().add_modifier(Modifier::BOLD);

    let max_ri = max_r as i32;
    for i in -max_ri..=max_ri {
        let x = (cx as i32 + i) as u16;
        if in_area(area, x, cy) {
            buf.set_string(x, cy, "─", dim);
        }
        let y = (cy as f64 + f64::from(i) * ASPECT).round() as u16;
        if in_area(area, cx, y) {
            buf.set_string(cx, y, "│", dim);
        }
    }
    if in_area(area, cx, cy) {
        buf.set_string(cx, cy, "┼", dim);
    }

    let ranges: Vec<f64> = tracks
        .iter()
        .map(|t| xy_to_range_bearing(t.x, t.y).0)
        .collect();
    let (ring_defs, scale_max_m) = scale.pick_ranges(&ranges);
    for (frac, label) in &ring_defs {
        let ring = max_r * frac;
        for deg in (0..360).step_by(2) {
            let (x, y) = polar_to_cell(cx, cy, f64::from(deg), ring);
            if in_area(area, x, y) {
                buf.set_string(x, y, "∙", dim);
            }
        }
        let (lx, ly) = polar_to_cell(cx, cy, 45.0, ring);
        if in_area(area, lx.saturating_add(1), ly) {
            buf.set_string(lx.saturating_add(1), ly, label, dim);
        }
    }

    for (deg, lbl) in [(0.0, "N"), (90.0, "E"), (180.0, "S"), (270.0, "W")] {
        let rel = (deg - heading_deg).rem_euclid(360.0);
        let (x, y) = polar_to_cell(cx, cy, rel, max_r + 2.0);
        if in_area(area, x, y) {
            buf.set_string(x, y, lbl, bold);
        }
    }

    for step in 1..=(max_r as i32) {
        let t = f64::from(step) / max_r;
        let (x, y) = polar_to_cell(cx, cy, 0.0, max_r * t);
        if in_area(area, x, y) {
            let ch = if step == max_r as i32 { "▲" } else { "│" };
            buf.set_string(
                x,
                y,
                ch,
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            );
        }
    }

    for t in tracks.iter().take(40) {
        let (range_m, world_bearing) = xy_to_range_bearing(t.x, t.y);
        let bearing = (world_bearing - heading_deg).rem_euclid(360.0);
        let dist = distance_to_radius(range_m, scale_max_m, max_r);
        let (x, y) = polar_to_cell(cx, cy, bearing, dist);
        if !in_area(area, x, y) {
            continue;
        }
        let mut style = Style::default().fg(Color::Magenta);
        if selected_mac == Some(t.mac.as_str()) {
            style = style.add_modifier(Modifier::REVERSED | Modifier::BOLD);
        }
        buf.set_string(x, y, "◆", style);
    }
}

fn in_area(area: ratatui::layout::Rect, x: u16, y: u16) -> bool {
    x > area.x
        && y > area.y
        && x < area.x.saturating_add(area.width).saturating_sub(1)
        && y < area.y.saturating_add(area.height).saturating_sub(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use wifi_radar_core::WifiDevice;

    #[test]
    fn distance_to_radius_clamps() {
        assert!((distance_to_radius(0.0, 30.0, 10.0) - 0.0).abs() < 1e-9);
        assert!((distance_to_radius(30.0, 30.0, 10.0) - 10.0).abs() < 1e-9);
        assert!((distance_to_radius(100.0, 30.0, 10.0) - 10.0).abs() < 1e-9);
    }

    #[test]
    fn polar_forward_is_above_center() {
        let (x, y) = polar_to_cell(40, 20, 0.0, 10.0);
        assert_eq!(x, 40);
        assert!(y < 20, "forward/up should decrease row, got y={y}");
    }

    #[test]
    fn xy_to_range_bearing_cardinals() {
        let (r, b) = xy_to_range_bearing(0.0, 5.0);
        assert!((r - 5.0).abs() < 1e-9);
        assert!((b - 0.0).abs() < 1e-9, "north bearing={b}");
        let (r, b) = xy_to_range_bearing(3.0, 0.0);
        assert!((r - 3.0).abs() < 1e-9);
        assert!((b - 90.0).abs() < 1e-9, "east bearing={b}");
    }

    #[test]
    fn pick_scale_empty_defaults_near_30() {
        let mut s = ScaleState::default();
        let (_rings, max_m) = s.pick(&[]);
        assert!((max_m - 30.0).abs() < 1e-9);
    }

    #[test]
    fn pick_scale_near_device_uses_10m() {
        let mut s = ScaleState::default();
        let d = WifiDevice::hotspot("aa:bb:cc:dd:ee:01", -35.0); // ~1.5m-ish
        let (_rings, max_m) = s.pick(&[d]);
        assert!(max_m <= 30.0);
    }
}
