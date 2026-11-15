#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum CompositorError {
    #[error("Dimension mismatch: base {0}x{1} vs overlay {2}x{3}")]
    DimensionMismatch(u32, u32, u32, u32),
    #[error("Invalid LUT format: {0}")]
    InvalidLut(String),
}

pub type Rgba = [f64; 4];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageBuffer {
    pub width: u32,
    pub height: u32,
    pub data: Vec<Rgba>,
}

impl ImageBuffer {
    pub fn new(width: u32, height: u32, fill: Rgba) -> Self {
        let count = (width * height) as usize;
        Self {
            width,
            height,
            data: vec![fill; count],
        }
    }

    pub fn get_pixel(&self, x: u32, y: u32) -> Option<Rgba> {
        if x < self.width && y < self.height {
            let idx = (y * self.width + x) as usize;
            self.data.get(idx).copied()
        } else {
            None
        }
    }

    pub fn set_pixel(&mut self, x: u32, y: u32, color: Rgba) {
        if x < self.width && y < self.height {
            let idx = (y * self.width + x) as usize;
            self.data[idx] = color;
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlendMode {
    Normal,
    Multiply,
    Screen,
    Add,
    Overlay,
}

pub fn blend_pixels(base: Rgba, over: Rgba, mode: BlendMode, opacity: f64) -> Rgba {
    let alpha_b = base[3];
    let alpha_o = over[3] * opacity.clamp(0.0, 1.0);

    let blend_rgb = match mode {
        BlendMode::Normal => [over[0], over[1], over[2]],
        BlendMode::Multiply => [base[0] * over[0], base[1] * over[1], base[2] * over[2]],
        BlendMode::Screen => [
            1.0 - (1.0 - base[0]) * (1.0 - over[0]),
            1.0 - (1.0 - base[1]) * (1.0 - over[1]),
            1.0 - (1.0 - base[2]) * (1.0 - over[2]),
        ],
        BlendMode::Add => [
            (base[0] + over[0]).min(1.0),
            (base[1] + over[1]).min(1.0),
            (base[2] + over[2]).min(1.0),
        ],
        BlendMode::Overlay => {
            let blend_ch = |b: f64, o: f64| {
                if b < 0.5 {
                    2.0 * b * o
                } else {
                    1.0 - 2.0 * (1.0 - b) * (1.0 - o)
                }
            };
            [blend_ch(base[0], over[0]), blend_ch(base[1], over[1]), blend_ch(base[2], over[2])]
        }
    };

    // Standard Porter-Duff Over composite
    let out_a = alpha_o + alpha_b * (1.0 - alpha_o);
    if out_a < 1e-9 {
        return [0.0, 0.0, 0.0, 0.0];
    }

    let inv_out_a = 1.0 / out_a;
    let r = (blend_rgb[0] * alpha_o + base[0] * alpha_b * (1.0 - alpha_o)) * inv_out_a;
    let g = (blend_rgb[1] * alpha_o + base[1] * alpha_b * (1.0 - alpha_o)) * inv_out_a;
    let b = (blend_rgb[2] * alpha_o + base[2] * alpha_b * (1.0 - alpha_o)) * inv_out_a;

    [r.clamp(0.0, 1.0), g.clamp(0.0, 1.0), b.clamp(0.0, 1.0), out_a.clamp(0.0, 1.0)]
}

/// DaVinci Resolve-class 3-way color grade adjustments
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreeWayColorGrade {
    pub lift: [f64; 3],   // Shadows (-1.0 to +1.0, 0 = neutral)
    pub gamma: [f64; 3],  // Midtones (0.1 to 10.0, 1 = neutral)
    pub gain: [f64; 3],   // Highlights (0.0 to 10.0, 1 = neutral)
    pub offset: [f64; 3], // Master offset (-1.0 to +1.0, 0 = neutral)
    pub saturation: f64,  // Master saturation (0.0 = b/w, 1.0 = neutral)
}

impl Default for ThreeWayColorGrade {
    fn default() -> Self {
        Self {
            lift: [0.0, 0.0, 0.0],
            gamma: [1.0, 1.0, 1.0],
            gain: [1.0, 1.0, 1.0],
            offset: [0.0, 0.0, 0.0],
            saturation: 1.0,
        }
    }
}

impl ThreeWayColorGrade {
    pub fn apply_to_pixel(&self, mut px: Rgba) -> Rgba {
        for (c, ch) in px.iter_mut().enumerate().take(3) {
            let mut v = *ch;
            // 1. Lift
            v = v * (1.0 - self.lift[c]) + self.lift[c];
            // 2. Gain
            v *= self.gain[c];
            // 3. Gamma
            if self.gamma[c] > 1e-4 && v > 0.0 {
                v = v.powf(1.0 / self.gamma[c]);
            }
            // 4. Offset
            v += self.offset[c];

            *ch = v.clamp(0.0, 1.0);
        }

        // Saturation adjustment using Rec.709 luma coefficients
        let luma = 0.2126 * px[0] + 0.7152 * px[1] + 0.0722 * px[2];
        for ch in px.iter_mut().take(3) {
            *ch = (luma + (*ch - luma) * self.saturation).clamp(0.0, 1.0);
        }

        px
    }

    pub fn apply_to_buffer(&self, buf: &mut ImageBuffer) {
        for px in &mut buf.data {
            *px = self.apply_to_pixel(*px);
        }
    }
}

/// 3D LUT (Look-Up Table) for color grading
#[derive(Debug, Clone)]
pub struct CubeLut3D {
    pub title: String,
    pub size: usize,
    pub table: Vec<[f64; 3]>,
}

impl CubeLut3D {
    pub fn new(title: impl Into<String>, size: usize) -> Self {
        let count = size * size * size;
        Self {
            title: title.into(),
            size,
            table: vec![[0.0, 0.0, 0.0]; count],
        }
    }

    pub fn parse_cube(content: &str) -> Result<Self, CompositorError> {
        let mut title = "CubeLUT".to_string();
        let mut size = 0usize;
        let mut table = Vec::new();

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if line.starts_with("TITLE") {
                title = line.replace("TITLE", "").replace('"', "").trim().to_string();
            } else if line.starts_with("LUT_3D_SIZE") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    size = parts[1].parse::<usize>().map_err(|_| {
                        CompositorError::InvalidLut("Failed to parse LUT_3D_SIZE".into())
                    })?;
                }
            } else {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() == 3 {
                    let r = parts[0].parse::<f64>().unwrap_or(0.0);
                    let g = parts[1].parse::<f64>().unwrap_or(0.0);
                    let b = parts[2].parse::<f64>().unwrap_or(0.0);
                    table.push([r, g, b]);
                }
            }
        }

        if size == 0 || table.len() != size * size * size {
            return Err(CompositorError::InvalidLut(format!(
                "Table length {} does not match size {}^3",
                table.len(),
                size
            )));
        }

        Ok(Self { title, size, table })
    }

    /// Trilinear interpolation sample
    pub fn sample(&self, r: f64, g: f64, b: f64) -> [f64; 3] {
        let max_idx = (self.size - 1) as f64;
        let r_coord = r.clamp(0.0, 1.0) * max_idx;
        let g_coord = g.clamp(0.0, 1.0) * max_idx;
        let b_coord = b.clamp(0.0, 1.0) * max_idx;

        let r0 = (r_coord.floor() as usize).min(self.size - 1);
        let g0 = (g_coord.floor() as usize).min(self.size - 1);
        let b0 = (b_coord.floor() as usize).min(self.size - 1);

        let r1 = (r0 + 1).min(self.size - 1);
        let g1 = (g0 + 1).min(self.size - 1);
        let b1 = (b0 + 1).min(self.size - 1);

        let dr = r_coord - (r0 as f64);
        let dg = g_coord - (g0 as f64);
        let db = b_coord - (b0 as f64);

        let idx = |ir: usize, ig: usize, ib: usize| -> usize {
            ir + ig * self.size + ib * self.size * self.size
        };

        let c000 = self.table[idx(r0, g0, b0)];
        let c100 = self.table[idx(r1, g0, b0)];
        let c010 = self.table[idx(r0, g1, b0)];
        let c110 = self.table[idx(r1, g1, b0)];
        let c001 = self.table[idx(r0, g0, b1)];
        let c101 = self.table[idx(r1, g0, b1)];
        let c011 = self.table[idx(r0, g1, b1)];
        let c111 = self.table[idx(r1, g1, b1)];

        let mut out = [0.0; 3];
        for i in 0..3 {
            let c00 = c000[i] * (1.0 - dr) + c100[i] * dr;
            let c10 = c010[i] * (1.0 - dr) + c110[i] * dr;
            let c01 = c001[i] * (1.0 - dr) + c101[i] * dr;
            let c11 = c011[i] * (1.0 - dr) + c111[i] * dr;

            let c0 = c00 * (1.0 - dg) + c10 * dg;
            let c1 = c01 * (1.0 - dg) + c11 * dg;

            out[i] = (c0 * (1.0 - db) + c1 * db).clamp(0.0, 1.0);
        }

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_alpha_compositing_multiply() {
        let base = [1.0, 1.0, 1.0, 1.0];
        let over = [0.5, 0.2, 0.0, 1.0];
        let res = blend_pixels(base, over, BlendMode::Multiply, 1.0);

        assert!((res[0] - 0.5).abs() < 1e-4);
        assert!((res[1] - 0.2).abs() < 1e-4);
        assert!((res[2] - 0.0).abs() < 1e-4);
    }

    #[test]
    fn test_three_way_color_grade() {
        let grade = ThreeWayColorGrade {
            gain: [1.2, 1.0, 0.8],
            saturation: 1.5,
            ..Default::default()
        };

        let px = [0.5, 0.5, 0.5, 1.0];
        let graded = grade.apply_to_pixel(px);
        assert!(graded[0] > graded[2], "Red should be boosted over Blue");
    }

    #[test]
    fn test_3d_lut_sampling() {
        let cube_text = "TITLE \"Test\"\nLUT_3D_SIZE 2\n0.0 0.0 0.0\n1.0 0.0 0.0\n0.0 1.0 0.0\n1.0 1.0 0.0\n0.0 0.0 1.0\n1.0 0.0 1.0\n0.0 1.0 1.0\n1.0 1.0 1.0\n";
        let lut = CubeLut3D::parse_cube(cube_text);
        assert!(lut.is_ok());
        if let Ok(l) = lut {
            let sampled = l.sample(0.5, 0.5, 0.5);
            assert!((sampled[0] - 0.5).abs() < 1e-3);
        }
    }
}
