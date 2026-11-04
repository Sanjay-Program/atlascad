//! Material-removal preview and holder-clearance checking, both on a heightmap stock model.
//!
//! This is a 2.5D stock model. Each grid cell stores the height of the remaining material, starting
//! at the stock top. Every cutting move lowers the cells its tool disc covers to the move's z, and
//! the removed volume is the sum of the cell height drops. Rapids remove nothing. A cell model
//! cannot represent an undercut (a cavity opening sideways), which is exactly the limitation of the
//! 2.5D/3-axis process it previews.
//!
//! [`check_holder_clearance`] uses the same model in program order: before each move, the holder's
//! lower face is compared with the material still standing at the tool's xy position. Because the
//! model is carved as the program advances, a holder that is safe only *after* earlier passes have
//! cleared material is accepted, and one that enters stock the program has not removed is refused.

use crate::setup::Stock;
use crate::tool::Tool;
use crate::tool_path::{Toolpath, ToolpathPoint};

/// The resolution used when a caller does not ask for one, in millimetres.
pub const DEFAULT_PREVIEW_RESOLUTION_MM: f64 = 1.0;

/// A heightmap stock model that a toolpath can carve.
#[derive(Debug, Clone, PartialEq)]
pub struct MaterialRemoval {
    stock: Stock,
    cell: [f64; 2],
    columns: usize,
    rows: usize,
    heights: Vec<f64>,
    removed_mm3: f64,
}

impl MaterialRemoval {
    /// A full stock block at `resolution_mm`.
    pub fn new(stock: Stock, resolution_mm: f64) -> Result<Self, SimulationError> {
        if !resolution_mm.is_finite() || resolution_mm <= 0.0 {
            return Err(SimulationError::InvalidResolution(resolution_mm));
        }
        let size = stock.size();
        let columns = ((size[0] / resolution_mm).ceil() as usize).max(1);
        let rows = ((size[1] / resolution_mm).ceil() as usize).max(1);
        let cell = [size[0] / columns as f64, size[1] / rows as f64];
        Ok(MaterialRemoval {
            stock,
            cell,
            columns,
            rows,
            heights: vec![stock.max[2]; columns * rows],
            removed_mm3: 0.0,
        })
    }

    /// Apply every cutting move of `toolpath` to the model.
    ///
    /// The toolpath is expected to have been validated already; this function only clips a cut to
    /// the stock floor so a bad program cannot make the preview taller than the block.
    pub fn carve(&mut self, toolpath: &Toolpath, tool: &Tool) {
        let radius = tool.radius_mm();
        for pair in toolpath.points.windows(2) {
            self.carve_segment(pair[0].position, pair[1], radius);
        }
    }

    /// Carve one cutting move. A rapid is ignored.
    fn carve_segment(&mut self, from: [f64; 3], target: ToolpathPoint, radius: f64) {
        if !target.kind.is_cutting() {
            return;
        }
        let z = target.position[2].max(self.stock.min[2]);
        let start = [from[0], from[1]];
        let end = [target.position[0], target.position[1]];
        let dx = end[0] - start[0];
        let dy = end[1] - start[1];
        let length = (dx * dx + dy * dy).sqrt();
        let step = (self.cell[0].min(self.cell[1]) * 0.5).max(1.0e-6);
        let steps = (length / step).ceil() as usize;
        if steps == 0 {
            self.lower_disc(end[0], end[1], radius, z);
        } else {
            for index in 0..=steps {
                let t = index as f64 / steps as f64;
                self.lower_disc(start[0] + dx * t, start[1] + dy * t, radius, z);
            }
        }
    }

    /// Lower every cell whose centre lies under the tool disc.
    fn lower_disc(&mut self, x: f64, y: f64, radius: f64, z: f64) {
        let area = self.cell[0] * self.cell[1];
        let origin = [self.stock.min[0], self.stock.min[1]];
        let x0 = ((x - radius - origin[0]) / self.cell[0]).floor().max(0.0) as usize;
        let y0 = ((y - radius - origin[1]) / self.cell[1]).floor().max(0.0) as usize;
        let x1 =
            (((x + radius - origin[0]) / self.cell[0]).ceil()).clamp(0.0, self.columns as f64)
                as usize;
        let y1 = (((y + radius - origin[1]) / self.cell[1]).ceil()).clamp(0.0, self.rows as f64)
            as usize;
        if x0 >= x1 || y0 >= y1 || x0 >= self.columns || y0 >= self.rows {
            return;
        }
        for row in y0..y1 {
            let centre_y = origin[1] + (row as f64 + 0.5) * self.cell[1];
            for column in x0..x1 {
                let centre_x = origin[0] + (column as f64 + 0.5) * self.cell[0];
                let dx = centre_x - x;
                let dy = centre_y - y;
                if dx * dx + dy * dy > radius * radius {
                    continue;
                }
                let index = row * self.columns + column;
                let current = self.heights[index];
                if z < current {
                    self.removed_mm3 += (current - z) * area;
                    self.heights[index] = z;
                }
            }
        }
    }

    /// The stock volume before any cut, in mm³.
    pub fn stock_volume_mm3(&self) -> f64 {
        let size = self.stock.size();
        size[0] * size[1] * size[2]
    }

    /// The volume removed so far, in mm³.
    pub fn removed_volume_mm3(&self) -> f64 {
        self.removed_mm3
    }

    /// The fraction of the stock removed, in `0..=1`.
    pub fn removal_fraction(&self) -> f64 {
        if self.stock_volume_mm3() <= 0.0 {
            return 0.0;
        }
        (self.removed_mm3 / self.stock_volume_mm3()).clamp(0.0, 1.0)
    }

    /// The deepest cut below the stock top, in millimetres.
    pub fn max_cut_depth_mm(&self) -> f64 {
        let lowest = self.heights.iter().copied().fold(f64::INFINITY, f64::min);
        if !lowest.is_finite() {
            return 0.0;
        }
        (self.stock.max[2] - lowest).max(0.0)
    }

    /// The remaining height at an xy point, in work coordinates.
    pub fn height_at(&self, x: f64, y: f64) -> f64 {
        let column = (((x - self.stock.min[0]) / self.cell[0]).floor())
            .clamp(0.0, (self.columns - 1) as f64) as usize;
        let row = (((y - self.stock.min[1]) / self.cell[1]).floor())
            .clamp(0.0, (self.rows - 1) as f64) as usize;
        self.heights[row * self.columns + column]
    }

    /// The highest remaining material within `radius` of an xy point.
    ///
    /// A holder is a wide body, so the cells beside the tool axis matter: a narrow slot cleared by
    /// the tool does not clear a 20 mm holder above it.
    pub fn max_height_within(&self, x: f64, y: f64, radius: f64) -> f64 {
        let origin = [self.stock.min[0], self.stock.min[1]];
        let x0 = ((x - radius - origin[0]) / self.cell[0]).floor().max(0.0) as usize;
        let y0 = ((y - radius - origin[1]) / self.cell[1]).floor().max(0.0) as usize;
        let x1 = (((x + radius - origin[0]) / self.cell[0]).ceil())
            .clamp(0.0, self.columns as f64) as usize;
        let y1 = (((y + radius - origin[1]) / self.cell[1]).ceil())
            .clamp(0.0, self.rows as f64) as usize;
        if x0 >= x1 || y0 >= y1 || x0 >= self.columns || y0 >= self.rows {
            return self.stock.max[2];
        }
        let mut highest = f64::NEG_INFINITY;
        for row in y0..y1 {
            let centre_y = origin[1] + (row as f64 + 0.5) * self.cell[1];
            for column in x0..x1 {
                let centre_x = origin[0] + (column as f64 + 0.5) * self.cell[0];
                let dx = centre_x - x;
                let dy = centre_y - y;
                if dx * dx + dy * dy > radius * radius {
                    continue;
                }
                highest = highest.max(self.heights[row * self.columns + column]);
            }
        }
        if highest.is_finite() {
            highest
        } else {
            self.stock.max[2]
        }
    }

    /// The grid dimensions (columns, rows).
    pub fn dims(&self) -> (usize, usize) {
        (self.columns, self.rows)
    }

    /// The stock model as text: one character per cell, top of the plan first.
    ///
    /// The legend is depth-removed, so `#` is material fully removed at this resolution.
    pub fn render(&self) -> String {
        let mut out = String::new();
        let size = self.stock.size();
        out.push_str(&format!(
            "stock {:.1} x {:.1} x {:.1} mm, {:.2} x {:.2} mm per character, +y at the top\n",
            size[0], size[1], size[2], self.cell[0], self.cell[1]
        ));
        out.push_str("legend: ' ' untouched, '.' <25% removed, ':' <50%, '+' <75%, '#' >=75%\n");
        for row in (0..self.rows).rev() {
            let mut line = String::with_capacity(self.columns);
            for column in 0..self.columns {
                let height = self.heights[row * self.columns + column];
                let removed = (self.stock.max[2] - height) / size[2];
                line.push(match removed {
                    value if value <= 0.001 => ' ',
                    value if value < 0.25 => '.',
                    value if value < 0.5 => ':',
                    value if value < 0.75 => '+',
                    _ => '#',
                });
            }
            out.push_str(line.trim_end());
            out.push('\n');
        }
        out.push_str(&format!(
            "removed {:.1} mm3 of {:.1} mm3 ({:.1}%), deepest cut {:.2} mm\n",
            self.removed_mm3,
            self.stock_volume_mm3(),
            self.removal_fraction() * 100.0,
            self.max_cut_depth_mm()
        ));
        out
    }
}

/// Check that the modelled holder never enters material the program has not removed yet.
///
/// The stock is carved in program order, so the check uses the material state at each move rather
/// than the untouched block. A tool without a holder returns immediately.
pub fn check_holder_clearance(
    toolpath: &Toolpath,
    tool: &Tool,
    stock: Stock,
    resolution_mm: f64,
) -> Result<(), SimulationError> {
    if tool.holder.is_none() {
        return Ok(());
    }
    let mut model = MaterialRemoval::new(stock, resolution_mm)?;
    let flute = tool.flute_length_mm();
    let radius = tool.radius_mm();
    let holder_radius = tool
        .holder
        .map(|holder| holder.diameter_mm * 0.5)
        .unwrap_or(0.0);
    for (index, point) in toolpath.points.iter().enumerate() {
        let Some(previous) = index.checked_sub(1) else {
            if let Some((holder_bottom, material_top)) =
                holder_conflict(&model, stock, point.position, flute, holder_radius)
            {
                return Err(SimulationError::HolderCollision {
                    index,
                    holder_bottom,
                    material_top,
                });
            }
            model.carve_segment(point.position, *point, radius);
            continue;
        };
        let from = toolpath.points[previous].position;
        let to = point.position;
        // A holder is a body that travels, so a long move is checked along its whole length, not
        // only where the toolpath happens to place a point.
        let length = ((to[0] - from[0]).powi(2) + (to[1] - from[1]).powi(2)).sqrt();
        let steps = (length / resolution_mm).ceil().max(1.0) as usize;
        let mut conflict = None;
        for step in 0..=steps {
            let t = step as f64 / steps as f64;
            let position = [
                from[0] + (to[0] - from[0]) * t,
                from[1] + (to[1] - from[1]) * t,
                from[2] + (to[2] - from[2]) * t,
            ];
            if let Some(found) = holder_conflict(&model, stock, position, flute, holder_radius) {
                conflict = Some(found);
                break;
            }
        }
        if let Some((holder_bottom, material_top)) = conflict {
            return Err(SimulationError::HolderCollision {
                index,
                holder_bottom,
                material_top,
            });
        }
        model.carve_segment(from, *point, radius);
    }
    Ok(())
}

/// The holder bottom and the highest material under the holder at one position, when they overlap.
fn holder_conflict(
    model: &MaterialRemoval,
    stock: Stock,
    position: [f64; 3],
    flute: f64,
    holder_radius: f64,
) -> Option<(f64, f64)> {
    if !stock.contains_xy([position[0], position[1]], 0.0) {
        return None;
    }
    let holder_bottom = position[2] + flute;
    let material_top = model.max_height_within(position[0], position[1], holder_radius);
    (holder_bottom < material_top - 1.0e-6).then_some((holder_bottom, material_top))
}

/// Why a preview or a holder check failed.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum SimulationError {
    #[error("the preview resolution must be finite and positive, got {0} mm")]
    InvalidResolution(f64),
    #[error(
        "the holder enters the stock at move {index}: its lower face is at {holder_bottom} mm, below the material top {material_top} mm"
    )]
    HolderCollision {
        index: usize,
        holder_bottom: f64,
        material_top: f64,
    },
}
