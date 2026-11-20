use super::model::IconState;
use tray_icon::{BadIcon, Icon};

const ICON_SIZE: u32 = 36;
const ICON_CENTER: f32 = (ICON_SIZE as f32 - 1.0) / 2.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IconGlyph {
    Check,
    Alert,
    Question,
    Unconfigured,
}

/// Builds a 36×36 template RGBA buffer. Normal and unknown use a filled
/// disc, alert uses a warning triangle, and unconfigured uses a hollow ring.
pub fn icon_rgba_for_state(state: IconState) -> Vec<u8> {
    let glyph = match state {
        IconState::Normal => IconGlyph::Check,
        IconState::Alert => IconGlyph::Alert,
        IconState::Unknown => IconGlyph::Question,
        IconState::Unconfigured => IconGlyph::Unconfigured,
    };
    template_icon_rgba(glyph)
}

fn template_icon_rgba(glyph: IconGlyph) -> Vec<u8> {
    let mut rgba = vec![0; (ICON_SIZE * ICON_SIZE * 4) as usize];
    match glyph {
        IconGlyph::Check => {
            draw_filled_disc(&mut rgba);
            carve_check(&mut rgba);
        }
        IconGlyph::Alert => {
            draw_filled_triangle(&mut rgba);
            carve_warning(&mut rgba);
        }
        IconGlyph::Question => {
            draw_filled_disc(&mut rgba);
            carve_question(&mut rgba);
        }
        IconGlyph::Unconfigured => draw_ring(&mut rgba),
    }
    rgba
}

fn blend_ink(rgba: &mut [u8], x: i32, y: i32, coverage: f32) {
    if coverage <= 0.0 || x < 0 || y < 0 || x >= ICON_SIZE as i32 || y >= ICON_SIZE as i32 {
        return;
    }
    let alpha = (coverage.clamp(0.0, 1.0) * 255.0).round() as u8;
    if alpha == 0 {
        return;
    }
    let index = ((y as u32 * ICON_SIZE + x as u32) * 4) as usize;
    if alpha <= rgba[index + 3] {
        return;
    }
    rgba[index] = 0;
    rgba[index + 1] = 0;
    rgba[index + 2] = 0;
    rgba[index + 3] = alpha;
}

fn carve_ink(rgba: &mut [u8], x: i32, y: i32, coverage: f32) {
    if coverage <= 0.0 || x < 0 || y < 0 || x >= ICON_SIZE as i32 || y >= ICON_SIZE as i32 {
        return;
    }
    let index = ((y as u32 * ICON_SIZE + x as u32) * 4) as usize;
    let keep = 1.0 - coverage.clamp(0.0, 1.0);
    let alpha = (rgba[index + 3] as f32 * keep).round() as u8;
    rgba[index + 3] = alpha;
    if alpha == 0 {
        rgba[index] = 0;
        rgba[index + 1] = 0;
        rgba[index + 2] = 0;
    }
}

fn coverage_from_distance(distance: f32, half_width: f32) -> f32 {
    let aa = 0.65_f32;
    let solid = (half_width - aa).max(0.0);
    let edge = half_width + aa;
    if distance <= solid {
        1.0
    } else if distance >= edge {
        0.0
    } else {
        1.0 - (distance - solid) / (edge - solid)
    }
}

fn disc_coverage(distance: f32, radius: f32) -> f32 {
    let aa = 0.65_f32;
    let coverage = if distance <= radius - aa {
        1.0
    } else if distance >= radius + aa {
        0.0
    } else {
        1.0 - (distance - (radius - aa)) / (2.0 * aa)
    };
    if coverage < 0.25 { 0.0 } else { coverage }
}

fn draw_filled_disc(rgba: &mut [u8]) {
    const RADIUS: f32 = 14.6;

    for y in 0..ICON_SIZE {
        for x in 0..ICON_SIZE {
            let dx = x as f32 - ICON_CENTER;
            let dy = y as f32 - ICON_CENTER;
            let coverage = disc_coverage((dx * dx + dy * dy).sqrt(), RADIUS);
            blend_ink(rgba, x as i32, y as i32, coverage);
        }
    }
}

/// Hollow ring for the "no expected IP configured" state.
fn draw_ring(rgba: &mut [u8]) {
    const OUTER_RADIUS: f32 = 14.6;
    const INNER_RADIUS: f32 = 10.8;

    for y in 0..ICON_SIZE {
        for x in 0..ICON_SIZE {
            let dx = x as f32 - ICON_CENTER;
            let dy = y as f32 - ICON_CENTER;
            let distance = (dx * dx + dy * dy).sqrt();
            let coverage = (disc_coverage(distance, OUTER_RADIUS)
                - disc_coverage(distance, INNER_RADIUS))
            .max(0.0);
            blend_ink(rgba, x as i32, y as i32, coverage);
        }
    }
}

/// Filled warning triangle with supersampled antialiasing.
fn draw_filled_triangle(rgba: &mut [u8]) {
    for y in 0..ICON_SIZE {
        for x in 0..ICON_SIZE {
            let coverage = triangle_coverage(x as f32, y as f32);
            blend_ink(rgba, x as i32, y as i32, coverage);
        }
    }
}

fn triangle_coverage(x: f32, y: f32) -> f32 {
    const SAMPLES_PER_AXIS: usize = 4;
    let mut inside = 0;
    for sample_y in 0..SAMPLES_PER_AXIS {
        for sample_x in 0..SAMPLES_PER_AXIS {
            let px = x + (sample_x as f32 + 0.5) / SAMPLES_PER_AXIS as f32;
            let py = y + (sample_y as f32 + 0.5) / SAMPLES_PER_AXIS as f32;
            if point_in_triangle(px, py) {
                inside += 1;
            }
        }
    }
    inside as f32 / (SAMPLES_PER_AXIS * SAMPLES_PER_AXIS) as f32
}

fn point_in_triangle(px: f32, py: f32) -> bool {
    const TOP: (f32, f32) = (17.5, 3.6);
    const LEFT: (f32, f32) = (2.8, 29.0);
    const RIGHT: (f32, f32) = (32.2, 29.0);

    let d1 = edge_sign(px, py, TOP, LEFT);
    let d2 = edge_sign(px, py, LEFT, RIGHT);
    let d3 = edge_sign(px, py, RIGHT, TOP);
    let has_negative = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
    let has_positive = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
    !(has_negative && has_positive)
}

fn edge_sign(px: f32, py: f32, a: (f32, f32), b: (f32, f32)) -> f32 {
    (px - b.0) * (a.1 - b.1) - (a.0 - b.0) * (py - b.1)
}

fn carve_thick_segment(rgba: &mut [u8], x0: f32, y0: f32, x1: f32, y1: f32, thickness: f32) {
    let steps = (((x1 - x0).abs().max((y1 - y0).abs()) * 3.0).ceil() as i32).max(1);
    let half = thickness / 2.0;
    for step in 0..=steps {
        let t = step as f32 / steps as f32;
        let cx = x0 + (x1 - x0) * t;
        let cy = y0 + (y1 - y0) * t;
        let min_x = (cx - half - 1.0).floor() as i32;
        let max_x = (cx + half + 1.0).ceil() as i32;
        let min_y = (cy - half - 1.0).floor() as i32;
        let max_y = (cy + half + 1.0).ceil() as i32;
        for y in min_y..=max_y {
            for x in min_x..=max_x {
                let dx = x as f32 - cx;
                let dy = y as f32 - cy;
                let coverage = coverage_from_distance((dx * dx + dy * dy).sqrt(), half);
                carve_ink(rgba, x, y, coverage);
            }
        }
    }
}

fn carve_check(rgba: &mut [u8]) {
    carve_thick_segment(rgba, 10.0, 18.0, 16.0, 24.0, 2.6);
    carve_thick_segment(rgba, 16.0, 24.0, 26.0, 12.0, 2.6);
}

fn carve_warning(rgba: &mut [u8]) {
    carve_thick_segment(rgba, 17.5, 11.0, 17.5, 20.0, 3.2);
    carve_disc(rgba, 17.5, 24.2, 1.7);
}

fn carve_disc(rgba: &mut [u8], cx: f32, cy: f32, radius: f32) {
    let min_x = (cx - radius - 1.0).floor() as i32;
    let max_x = (cx + radius + 1.0).ceil() as i32;
    let min_y = (cy - radius - 1.0).floor() as i32;
    let max_y = (cy + radius + 1.0).ceil() as i32;
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            let coverage = disc_coverage((dx * dx + dy * dy).sqrt(), radius);
            carve_ink(rgba, x, y, coverage);
        }
    }
}

fn carve_question(rgba: &mut [u8]) {
    carve_thick_segment(rgba, 14.0, 12.0, 18.0, 10.0, 2.4);
    carve_thick_segment(rgba, 18.0, 10.0, 22.0, 12.0, 2.4);
    carve_thick_segment(rgba, 22.0, 12.0, 22.0, 16.0, 2.4);
    carve_thick_segment(rgba, 22.0, 16.0, 18.0, 19.0, 2.4);
    carve_thick_segment(rgba, 18.0, 19.0, 18.0, 22.0, 2.4);
    carve_thick_segment(rgba, 17.0, 25.0, 19.0, 25.0, 2.4);
    carve_thick_segment(rgba, 18.0, 24.0, 18.0, 26.0, 2.4);
}

pub(crate) struct IconSet {
    normal: Icon,
    alert: Icon,
    unknown: Icon,
    unconfigured: Icon,
}

impl IconSet {
    pub(crate) fn new() -> Result<Self, BadIcon> {
        Ok(Self {
            normal: Icon::from_rgba(icon_rgba_for_state(IconState::Normal), ICON_SIZE, ICON_SIZE)?,
            alert: Icon::from_rgba(icon_rgba_for_state(IconState::Alert), ICON_SIZE, ICON_SIZE)?,
            unknown: Icon::from_rgba(
                icon_rgba_for_state(IconState::Unknown),
                ICON_SIZE,
                ICON_SIZE,
            )?,
            unconfigured: Icon::from_rgba(
                icon_rgba_for_state(IconState::Unconfigured),
                ICON_SIZE,
                ICON_SIZE,
            )?,
        })
    }

    pub(crate) fn for_state(&self, state: IconState) -> &Icon {
        match state {
            IconState::Normal => &self.normal,
            IconState::Alert => &self.alert,
            IconState::Unknown => &self.unknown,
            IconState::Unconfigured => &self.unconfigured,
        }
    }
}
