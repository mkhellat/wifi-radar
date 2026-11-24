// SPDX-FileCopyrightText: 2026 wifi-radar contributors
// SPDX-License-Identifier: GPL-3.0-or-later

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

/// Confidence at or above this paints cyan. A display bin, not a probability.
const FUSED_CONF_HIGH: f64 = 0.70;
/// Confidence at or above this, and below [`FUSED_CONF_HIGH`], paints yellow.
const FUSED_CONF_MID: f64 = 0.40;

/// Map fused-track confidence to a polar glyph color.
///
/// High (≥ 0.70) is cyan, mid (≥ 0.40) is yellow, and low or non-finite
/// is dark gray. The bins are a solver-heuristic display, not survey-grade
/// accuracy.
pub fn fused_confidence_color(confidence: f64) -> ratatui::style::Color {
    use ratatui::style::Color;
    if !confidence.is_finite() {
        return Color::DarkGray;
    }
    if confidence >= FUSED_CONF_HIGH {
        Color::Cyan
    } else if confidence >= FUSED_CONF_MID {
        Color::Yellow
    } else {
        Color::DarkGray
    }
}

/// `◆` when the solver status is `ok`; `?` for every other status.
pub fn fused_track_glyph(status: &str) -> &'static str {
    if status == "ok" {
        "◆"
    } else {
        "?"
    }
}

/// Glyph style: confidence color when `ok`, dim dark gray otherwise.
pub fn fused_track_style(confidence: f64, status: &str) -> ratatui::style::Style {
    use ratatui::style::{Color, Modifier, Style};
    if status == "ok" {
        Style::default().fg(fused_confidence_color(confidence))
    } else {
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::DIM)
    }
}

/// Screen radius for a residual ring, clamped to the polar scale.
///
/// Non-finite, negative, or zero RMS yields 0. A non-positive scale
/// yields 0 so a broken scale cannot paint a full-canvas ring. This is
/// the residual RMS drawn as a radius, not a covariance ellipse.
pub fn residual_ring_radius(residual_rms_m: f64, scale_max_m: f64, max_radius: f64) -> f64 {
    if !residual_rms_m.is_finite()
        || residual_rms_m <= 0.0
        || !scale_max_m.is_finite()
        || scale_max_m <= 0.0
        || !max_radius.is_finite()
        || max_radius <= 0.0
    {
        return 0.0;
    }
    distance_to_radius(residual_rms_m, scale_max_m, max_radius)
}

/// Paint a coarse dotted circle of `·` around `(cx, cy)`.
///
/// Radius is in the same cell space as [`polar_to_cell`]. Rings under
/// one cell are skipped. The mark is an isotropic residual hint, not an
/// error ellipse.
pub fn paint_residual_ring(
    buf: &mut ratatui::buffer::Buffer,
    area: ratatui::layout::Rect,
    cx: u16,
    cy: u16,
    radius: f64,
) {
    use ratatui::style::{Color, Style};
    if !radius.is_finite() || radius < 1.0 {
        return;
    }
    let dim = Style::default().fg(Color::DarkGray);
    for deg in (0..360).step_by(15) {
        let (x, y) = polar_to_cell(cx, cy, f64::from(deg), radius);
        if (x, y) == (cx, cy) || !in_area(area, x, y) {
            continue;
        }
        buf.set_string(x, y, "·", dim);
    }
}

/// Paint polar rings and fused 2D tracks (`x`/`y` metres in the shared frame).
///
/// Glyph color follows [`fused_confidence_color`] when status is `ok`.
/// Any other status is a dim `?`. The selected track also gets a dotted
/// residual ring ([`paint_residual_ring`]). Best-effort 2D display only.
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
        let selected = selected_mac == Some(t.mac.as_str());
        if selected {
            let ring_r = residual_ring_radius(t.residual_rms_m, scale_max_m, max_r);
            paint_residual_ring(buf, area, x, y, ring_r);
        }
        let mut style = fused_track_style(t.confidence, &t.status);
        if selected {
            style = style.add_modifier(Modifier::REVERSED | Modifier::BOLD);
        }
        buf.set_string(x, y, fused_track_glyph(&t.status), style);
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

    #[test]
    fn confidence_color_bands_are_cyan_yellow_dark_gray() {
        use ratatui::style::Color;

        assert_eq!(fused_confidence_color(0.70), Color::Cyan);
        assert_eq!(fused_confidence_color(1.0), Color::Cyan);
        assert_eq!(fused_confidence_color(0.40), Color::Yellow);
        assert_eq!(fused_confidence_color(0.69), Color::Yellow);
        assert_eq!(fused_confidence_color(0.0), Color::DarkGray);
        assert_eq!(fused_confidence_color(0.39), Color::DarkGray);
        assert_eq!(fused_confidence_color(f64::NAN), Color::DarkGray);
        assert_eq!(fused_confidence_color(-1.0), Color::DarkGray);
    }

    #[test]
    fn fused_glyph_is_diamond_only_when_status_ok() {
        assert_eq!(fused_track_glyph("ok"), "◆");
        assert_eq!(fused_track_glyph("insufficient_nodes"), "?");
        assert_eq!(fused_track_glyph("insufficient_geometry"), "?");
        assert_eq!(fused_track_glyph(""), "?");
    }

    #[test]
    fn non_ok_style_is_dim_and_ignores_high_confidence() {
        use ratatui::style::{Color, Modifier};

        let style = fused_track_style(0.95, "insufficient_nodes");
        assert_eq!(style.fg, Some(Color::DarkGray));
        assert!(style.add_modifier.contains(Modifier::DIM));
        let ok = fused_track_style(0.95, "ok");
        assert_eq!(ok.fg, Some(Color::Cyan));
        assert!(!ok.add_modifier.contains(Modifier::DIM));
    }

    #[test]
    fn residual_ring_radius_clamps_to_scale() {
        assert!((residual_ring_radius(15.0, 30.0, 10.0) - 5.0).abs() < 1e-9);
        assert!((residual_ring_radius(1000.0, 30.0, 10.0) - 10.0).abs() < 1e-9);
        assert!((residual_ring_radius(0.0, 30.0, 10.0)).abs() < 1e-9);
        assert!((residual_ring_radius(-3.0, 30.0, 10.0)).abs() < 1e-9);
        assert!((residual_ring_radius(f64::NAN, 30.0, 10.0)).abs() < 1e-9);
        assert!((residual_ring_radius(f64::INFINITY, 30.0, 10.0)).abs() < 1e-9);
        assert!((residual_ring_radius(4.0, 0.0, 10.0)).abs() < 1e-9);
    }

    #[test]
    fn residual_ring_paints_a_coarse_dotted_circle() {
        use ratatui::buffer::Buffer;
        use ratatui::layout::Rect;
        use ratatui::style::Color;

        let area = Rect::new(0, 0, 80, 40);
        let mut buf = Buffer::empty(area);
        paint_residual_ring(&mut buf, area, 40, 20, 8.0);
        let dots = cells_with_symbol(&buf, "·");
        assert!(
            dots.len() >= 8,
            "expected a dotted ring, got {} cells",
            dots.len()
        );
        assert!(dots.iter().all(|(_, _, fg)| *fg == Color::DarkGray));
        let (min_x, max_x, min_y, max_y) = span_of(&dots);
        assert!(max_x - min_x >= 8, "ring should spread in x");
        assert!(max_y - min_y >= 3, "ring should spread in y");
        assert!(dots.iter().all(|(x, y, _)| *x != 40 || *y != 20));
    }

    #[test]
    fn paint_selected_high_confidence_shows_cyan_diamond_and_residual_ring() {
        use ratatui::buffer::Buffer;
        use ratatui::layout::Rect;
        use ratatui::style::{Color, Modifier};

        let area = Rect::new(0, 0, 80, 40);
        let mut buf = Buffer::empty(area);
        let mut scale = ScaleState::default();
        let track = sample_fused_track(0.91, 4.0, "ok");
        paint_fused_tracks(
            &mut buf,
            area,
            &[track],
            0.0,
            Some("aa:aa:aa:00:00:10"),
            &mut scale,
        );
        let glyphs = cells_with_symbol(&buf, "◆");
        assert_eq!(glyphs.len(), 1, "one fused glyph");
        assert_eq!(glyphs[0].2, Color::Cyan);
        let cell = &buf[(glyphs[0].0, glyphs[0].1)];
        assert!(cell.modifier.contains(Modifier::REVERSED));
        assert!(cell.modifier.contains(Modifier::BOLD));
        let dots = cells_with_symbol(&buf, "·");
        assert!(
            dots.len() >= 8,
            "selected residual ring should be a dotted arc, got {}",
            dots.len()
        );
        let (min_x, max_x, min_y, max_y) = span_of(&dots);
        assert!(max_x > min_x && max_y > min_y, "ring is not a line");
    }

    #[test]
    fn paint_unselected_track_skips_residual_ring() {
        use ratatui::buffer::Buffer;
        use ratatui::layout::Rect;
        use ratatui::style::Color;

        let area = Rect::new(0, 0, 80, 40);
        let mut buf = Buffer::empty(area);
        let mut scale = ScaleState::default();
        let track = sample_fused_track(0.55, 4.0, "ok");
        paint_fused_tracks(&mut buf, area, &[track], 0.0, None, &mut scale);
        assert!(cells_with_symbol(&buf, "·").is_empty());
        let glyphs = cells_with_symbol(&buf, "◆");
        assert_eq!(glyphs.len(), 1);
        assert_eq!(glyphs[0].2, Color::Yellow);
    }

    #[test]
    fn paint_non_ok_track_is_dim_question_mark() {
        use ratatui::buffer::Buffer;
        use ratatui::layout::Rect;
        use ratatui::style::{Color, Modifier};

        let area = Rect::new(0, 0, 80, 40);
        let mut buf = Buffer::empty(area);
        let mut scale = ScaleState::default();
        let track = sample_fused_track(0.95, 4.0, "insufficient_geometry");
        paint_fused_tracks(&mut buf, area, &[track], 0.0, None, &mut scale);
        assert!(cells_with_symbol(&buf, "◆").is_empty());
        let marks = cells_with_symbol(&buf, "?");
        assert_eq!(marks.len(), 1);
        assert_eq!(marks[0].2, Color::DarkGray);
        assert!(buf[(marks[0].0, marks[0].1)]
            .modifier
            .contains(Modifier::DIM));
        assert!(cells_with_symbol(&buf, "·").is_empty());
    }

    fn sample_fused_track(
        confidence: f64,
        residual_rms_m: f64,
        status: &str,
    ) -> wifi_radar_fuse::FusedTrack {
        wifi_radar_fuse::FusedTrack {
            mac: "aa:aa:aa:00:00:10".into(),
            x: 0.0,
            y: 5.0,
            confidence,
            residual_rms_m,
            contributing_nodes: 3,
            solver: "mlat".into(),
            status: status.into(),
        }
    }

    fn cells_with_symbol(
        buf: &ratatui::buffer::Buffer,
        symbol: &str,
    ) -> Vec<(u16, u16, ratatui::style::Color)> {
        let area = buf.area;
        let mut out = Vec::new();
        for y in area.y..area.y.saturating_add(area.height) {
            for x in area.x..area.x.saturating_add(area.width) {
                let cell = &buf[(x, y)];
                if cell.symbol() == symbol {
                    out.push((x, y, cell.fg));
                }
            }
        }
        out
    }

    fn span_of(cells: &[(u16, u16, ratatui::style::Color)]) -> (u16, u16, u16, u16) {
        let min_x = cells.iter().map(|c| c.0).min().unwrap();
        let max_x = cells.iter().map(|c| c.0).max().unwrap();
        let min_y = cells.iter().map(|c| c.1).min().unwrap();
        let max_y = cells.iter().map(|c| c.1).max().unwrap();
        (min_x, max_x, min_y, max_y)
    }
}
