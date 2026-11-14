//! Mathematical infill pattern generation.

use crate::settings::InfillPattern;
use serde::{Deserialize, Serialize};

/// 2D line segment representing a single infill pass.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InfillLine {
    pub start: [f64; 2],
    pub end: [f64; 2],
}

impl InfillLine {
    pub const fn new(x1: f64, y1: f64, x2: f64, y2: f64) -> Self {
        Self {
            start: [x1, y1],
            end: [x2, y2],
        }
    }

    pub fn length(&self) -> f64 {
        let dx = self.end[0] - self.start[0];
        let dy = self.end[1] - self.start[1];
        (dx * dx + dy * dy).sqrt()
    }
}

/// Generate interior infill lines within the specified bounding rectangle.
#[allow(clippy::too_many_arguments)]
pub fn generate_infill(
    pattern: InfillPattern,
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
    density: f64,
    line_width: f64,
    layer_index: usize,
) -> Vec<InfillLine> {
    if density <= 0.001 || min_x >= max_x || min_y >= max_y {
        return Vec::new();
    }

    // Clamp density between 0.05 and 1.0 to avoid division by zero or infinite density
    let clamped_density = density.clamp(0.05, 1.0);
    let spacing = line_width / clamped_density;

    match pattern {
        InfillPattern::Rectilinear => {
            // Alternate between horizontal and vertical (or +/-45 deg)
            if layer_index.is_multiple_of(2) {
                generate_parallel_lines_x(min_x, min_y, max_x, max_y, spacing)
            } else {
                generate_parallel_lines_y(min_x, min_y, max_x, max_y, spacing)
            }
        }
        InfillPattern::Grid => {
            let mut lines = generate_parallel_lines_x(min_x, min_y, max_x, max_y, spacing);
            lines.extend(generate_parallel_lines_y(
                min_x, min_y, max_x, max_y, spacing,
            ));
            lines
        }
        InfillPattern::Honeycomb => generate_honeycomb_lines(min_x, min_y, max_x, max_y, spacing),
        InfillPattern::Gyroid => {
            generate_gyroid_lines(min_x, min_y, max_x, max_y, spacing, layer_index)
        }
        InfillPattern::Concentric => generate_concentric_lines(min_x, min_y, max_x, max_y, spacing),
    }
}

fn generate_parallel_lines_x(
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
    spacing: f64,
) -> Vec<InfillLine> {
    let mut lines = Vec::new();
    let mut y = min_y + spacing * 0.5;
    let mut flip = false;
    while y <= max_y {
        if flip {
            lines.push(InfillLine::new(max_x, y, min_x, y));
        } else {
            lines.push(InfillLine::new(min_x, y, max_x, y));
        }
        flip = !flip;
        y += spacing;
    }
    lines
}

fn generate_parallel_lines_y(
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
    spacing: f64,
) -> Vec<InfillLine> {
    let mut lines = Vec::new();
    let mut x = min_x + spacing * 0.5;
    let mut flip = false;
    while x <= max_x {
        if flip {
            lines.push(InfillLine::new(x, max_y, x, min_y));
        } else {
            lines.push(InfillLine::new(x, min_y, x, max_y));
        }
        flip = !flip;
        x += spacing;
    }
    lines
}

fn generate_honeycomb_lines(
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
    spacing: f64,
) -> Vec<InfillLine> {
    let mut lines = Vec::new();
    let cell_w = spacing * 1.5;
    let cell_h = spacing * 0.866025; // sqrt(3)/2

    let mut y = min_y;
    let mut row: usize = 0;
    while y <= max_y {
        let x_offset = if row.is_multiple_of(2) {
            0.0
        } else {
            cell_w * 0.5
        };
        let mut x = min_x + x_offset;
        while x <= max_x {
            let x2 = (x + cell_w * 0.5).min(max_x);
            let y2 = (y + cell_h).min(max_y);
            lines.push(InfillLine::new(x, y, x2, y2));
            lines.push(InfillLine::new(x2, y2, x + cell_w, y));
            x += cell_w;
        }
        y += cell_h;
        row += 1;
    }
    lines
}

fn generate_gyroid_lines(
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
    spacing: f64,
    layer_index: usize,
) -> Vec<InfillLine> {
    let mut lines = Vec::new();
    let phase = (layer_index as f64) * 0.2;
    let wave_len = spacing * 2.0;
    let mut y = min_y + spacing * 0.5;

    while y <= max_y {
        let mut prev_x = min_x;
        let mut prev_y =
            y + (spacing * 0.3) * (prev_x / wave_len * std::f64::consts::PI + phase).sin();
        let step = (max_x - min_x) / 10.0;
        let mut x = min_x + step;
        while x <= max_x + 1e-4 {
            let cur_x = x.min(max_x);
            let cur_y =
                y + (spacing * 0.3) * (cur_x / wave_len * std::f64::consts::PI + phase).sin();
            lines.push(InfillLine::new(prev_x, prev_y, cur_x, cur_y));
            prev_x = cur_x;
            prev_y = cur_y;
            x += step;
        }
        y += spacing;
    }
    lines
}

fn generate_concentric_lines(
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
    spacing: f64,
) -> Vec<InfillLine> {
    let mut lines = Vec::new();
    let mut offset = spacing * 0.5;

    while (min_x + offset) < (max_x - offset) && (min_y + offset) < (max_y - offset) {
        let x1 = min_x + offset;
        let y1 = min_y + offset;
        let x2 = max_x - offset;
        let y2 = max_y - offset;

        // 4 outer boundary edges
        lines.push(InfillLine::new(x1, y1, x2, y1));
        lines.push(InfillLine::new(x2, y1, x2, y2));
        lines.push(InfillLine::new(x2, y2, x1, y2));
        lines.push(InfillLine::new(x1, y2, x1, y1));

        offset += spacing;
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rectilinear_infill_generation() {
        let lines_0 = generate_infill(
            InfillPattern::Rectilinear,
            0.0,
            0.0,
            50.0,
            50.0,
            0.2,
            0.4,
            0,
        );
        assert!(!lines_0.is_empty());
        let lines_1 = generate_infill(
            InfillPattern::Rectilinear,
            0.0,
            0.0,
            50.0,
            50.0,
            0.2,
            0.4,
            1,
        );
        assert!(!lines_1.is_empty());
    }

    #[test]
    fn test_honeycomb_and_gyroid() {
        let honey = generate_infill(InfillPattern::Honeycomb, 0.0, 0.0, 40.0, 40.0, 0.25, 0.4, 0);
        assert!(!honey.is_empty());
        let gyroid = generate_infill(InfillPattern::Gyroid, 0.0, 0.0, 40.0, 40.0, 0.25, 0.4, 0);
        assert!(!gyroid.is_empty());
    }
}
