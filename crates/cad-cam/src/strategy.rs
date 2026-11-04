//! Strategies: pure functions that turn an operation into a toolpath.
//!
//! Every strategy works in the setup's work frame and produces a program that
//!
//! * rapids only above the clearance plane,
//! * plunges at the tool's plunge feed,
//! * retracts at feed to the clearance plane before the next rapid,
//! * splits a deep cut into passes no deeper than the tool's maximum depth of cut.
//!
//! 3D roughing additionally samples the part into a [`HeightField`] and cuts only where the part
//! surface is below the current Z level, so the part is never cut. Walls are left one tool radius
//! proud, which is the radial stock a finishing operation removes.
//!
//! The result is validated, so a strategy that breaks one of those rules fails here rather than at
//! the machine.

use cad_contract::mesh::Mesh;

use crate::setup::{Setup, Stock};
use crate::surface::HeightField;
use crate::tool::{Machine, Tool, ToolGeometry};
use crate::tool_path::{
    MoveKind, POSITION_TOLERANCE_MM, Toolpath, ToolpathError, ToolpathId, ToolpathPoint,
};

/// What to machine.
#[derive(Debug, Clone, PartialEq)]
pub enum Operation {
    /// Face the stock top: a zig-zag raster over the whole block.
    Facing {
        /// Total depth to remove, millimetres.
        depth_mm: f64,
        /// Distance between raster passes, millimetres.
        stepover_mm: f64,
    },
    /// Trace a closed profile, offset outwards by the tool radius plus `offset_mm`.
    Contour {
        /// The profile, counter-clockwise, in work xy.
        profile: Vec<[f64; 2]>,
        /// Total depth, millimetres.
        depth_mm: f64,
        /// Depth of one pass, millimetres (clamped to the tool's maximum).
        stepdown_mm: f64,
        /// Extra stock left outside the profile, millimetres.
        offset_mm: f64,
    },
    /// Clear a rectangular region: a boundary loop plus a raster at each depth.
    Pocket {
        /// Region corners in work xy, any winding; the bounding box is used.
        region: [[f64; 2]; 4],
        /// Total depth, millimetres.
        depth_mm: f64,
        /// Depth of one pass, millimetres.
        stepdown_mm: f64,
        /// Distance between raster passes, millimetres.
        stepover_mm: f64,
    },
    /// Drill a set of holes with peck retracts.
    Drill {
        /// Hole centres in work xy.
        holes: Vec<[f64; 2]>,
        /// Hole depth from the stock top, millimetres.
        depth_mm: f64,
        /// Maximum peck before a retract, millimetres.
        peck_mm: f64,
    },
    /// Cut a straight slot the width of the tool.
    Slot {
        /// Start of the slot centre line, work xy.
        start: [f64; 2],
        /// End of the slot centre line, work xy.
        end: [f64; 2],
        /// Total depth, millimetres.
        depth_mm: f64,
        /// Depth of one pass, millimetres.
        stepdown_mm: f64,
    },
    /// Finish a part with a ball nose cutter following the sampled surface.
    Finish3D {
        /// The part mesh, in work coordinates.
        part: Mesh,
        /// Distance between raster passes, millimetres.
        stepover_mm: f64,
        /// Material left above the surface, millimetres.
        stock_to_leave_mm: f64,
        /// Height-field sampling resolution, millimetres.
        resolution_mm: f64,
    },
    /// Finish the vertical walls a roughing pass left radial stock on.
    FinishWalls {
        /// The part mesh, in work coordinates.
        part: Mesh,
        /// Depth of one Z level, millimetres (clamped to the tool's maximum).
        stepdown_mm: f64,
        /// Material left on the wall, millimetres.
        stock_to_leave_mm: f64,
        /// Height-field sampling resolution, millimetres.
        resolution_mm: f64,
    },
    /// Rough the stock down to a part with a Z-level raster of the sampled part surface.
    Rough3D {
        /// The part mesh, in work coordinates.
        part: Mesh,
        /// Distance between raster passes, millimetres.
        stepover_mm: f64,
        /// Depth of one Z level, millimetres (clamped to the tool's maximum).
        stepdown_mm: f64,
        /// Material left above the part floor, millimetres.
        stock_to_leave_mm: f64,
        /// Height-field sampling resolution, millimetres.
        resolution_mm: f64,
    },
}

impl Operation {
    /// A stable name, used in reports and program headers.
    pub const fn name(&self) -> &'static str {
        match self {
            Operation::Facing { .. } => "facing",
            Operation::Contour { .. } => "2D contour",
            Operation::Pocket { .. } => "2D pocket",
            Operation::Drill { .. } => "drilling",
            Operation::Slot { .. } => "slotting",
            Operation::Rough3D { .. } => "3D roughing",
            Operation::Finish3D { .. } => "3D finishing",
            Operation::FinishWalls { .. } => "wall finishing",
        }
    }

    /// The total depth this operation removes, when it is a fixed depth.
    ///
    /// 3D roughing derives its depth from the part, so it returns `None`.
    pub fn depth_mm(&self) -> Option<f64> {
        match self {
            Operation::Facing { depth_mm, .. }
            | Operation::Contour { depth_mm, .. }
            | Operation::Pocket { depth_mm, .. }
            | Operation::Drill { depth_mm, .. }
            | Operation::Slot { depth_mm, .. } => Some(*depth_mm),
            Operation::Rough3D { .. } | Operation::Finish3D { .. } | Operation::FinishWalls { .. } => {
                None
            }
        }
    }
}

/// Generate and validate a toolpath for one operation.
pub fn generate(
    operation: &Operation,
    setup: &Setup,
    tool: &Tool,
    id: ToolpathId,
) -> Result<Toolpath, ToolpathError> {
    if setup.machine != Machine::Milling3Axis {
        return Err(ToolpathError::UnsupportedMachine(setup.machine));
    }
    tool.validate()
        .map_err(|error| ToolpathError::InvalidParameter(error.to_string()))?;

    let mut planner = Planner::new(setup, tool);
    match operation {
        Operation::Rough3D {
            part,
            stepover_mm,
            stepdown_mm,
            stock_to_leave_mm,
            resolution_mm,
        } => planner.rough3d(
            part,
            *stepover_mm,
            *stepdown_mm,
            *stock_to_leave_mm,
            *resolution_mm,
        )?,
        Operation::Finish3D {
            part,
            stepover_mm,
            stock_to_leave_mm,
            resolution_mm,
        } => planner.finish3d(
            part,
            *stepover_mm,
            *stock_to_leave_mm,
            *resolution_mm,
        )?,
        Operation::FinishWalls {
            part,
            stepdown_mm,
            stock_to_leave_mm,
            resolution_mm,
        } => planner.finish_walls(part, *stepdown_mm, *stock_to_leave_mm, *resolution_mm)?,
        other => {
            let depth = other.depth_mm().unwrap_or(0.0);
            if !depth.is_finite() || depth <= 0.0 {
                return Err(ToolpathError::InvalidParameter(format!(
                    "{} needs a positive depth, got {depth} mm",
                    other.name()
                )));
            }
            if depth > setup.stock.size()[2] + POSITION_TOLERANCE_MM {
                return Err(ToolpathError::InvalidParameter(format!(
                    "{} removes {depth} mm from a {} mm high block",
                    other.name(),
                    setup.stock.size()[2]
                )));
            }
            match other {
                Operation::Facing {
                    depth_mm,
                    stepover_mm,
                } => planner.facing(*depth_mm, *stepover_mm)?,
                Operation::Contour {
                    profile,
                    depth_mm,
                    stepdown_mm,
                    offset_mm,
                } => planner.contour(profile, *depth_mm, *stepdown_mm, *offset_mm)?,
                Operation::Pocket {
                    region,
                    depth_mm,
                    stepdown_mm,
                    stepover_mm,
                } => planner.pocket(*region, *depth_mm, *stepdown_mm, *stepover_mm)?,
                Operation::Drill {
                    holes,
                    depth_mm,
                    peck_mm,
                } => planner.drill(holes, *depth_mm, *peck_mm)?,
                Operation::Slot {
                    start,
                    end,
                    depth_mm,
                    stepdown_mm,
                } => planner.slot(*start, *end, *depth_mm, *stepdown_mm)?,
                Operation::Rough3D { .. }
                | Operation::Finish3D { .. }
                | Operation::FinishWalls { .. } => {
                    return Err(ToolpathError::InvalidParameter(
                        "3D operations are planned by their own branch".into(),
                    ));
                }
            }
        }
    }

    let toolpath = Toolpath::new(
        id,
        tool.id,
        planner.points,
        tool.cutting.spindle_rpm,
        setup.clearance_mm,
    );
    toolpath.validate(setup)?;
    Ok(toolpath)
}

/// The depth levels from the stock top to the operation bottom, as work z values.
fn levels(top: f64, depth: f64, stepdown: f64) -> Vec<f64> {
    let mut levels = Vec::new();
    let mut cut = 0.0;
    while cut < depth - POSITION_TOLERANCE_MM {
        cut = (cut + stepdown).min(depth);
        levels.push(top - cut);
    }
    levels
}

/// Include both ends, spaced no more than `step` apart.
fn raster(min: f64, max: f64, step: f64) -> Vec<f64> {
    let span = max - min;
    if span <= 0.0 {
        return vec![min];
    }
    let steps = (span / step).ceil().max(1.0) as usize;
    (0..=steps)
        .map(|index| min + span * index as f64 / steps as f64)
        .collect()
}

/// The outward normal of an edge of a counter-clockwise polygon.
fn outward_normal(a: [f64; 2], b: [f64; 2]) -> Result<[f64; 2], ToolpathError> {
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    let length = (dx * dx + dy * dy).sqrt();
    if length < 1.0e-12 {
        return Err(ToolpathError::InvalidParameter(
            "the profile has a zero-length edge".into(),
        ));
    }
    Ok([dy / length, -dx / length])
}

/// Offset a counter-clockwise polygon outwards by `distance`.
fn offset_polygon(profile: &[[f64; 2]], distance: f64) -> Result<Vec<[f64; 2]>, ToolpathError> {
    if profile.len() < 3 {
        return Err(ToolpathError::InvalidParameter(
            "a contour profile needs at least three points".into(),
        ));
    }
    if !distance.is_finite() || distance < 0.0 {
        return Err(ToolpathError::InvalidParameter(format!(
            "a contour offset must be finite and non-negative, got {distance} mm"
        )));
    }
    let mut area = 0.0;
    for index in 0..profile.len() {
        let current = profile[index];
        let next = profile[(index + 1) % profile.len()];
        area += current[0] * next[1] - next[0] * current[1];
    }
    if area <= 0.0 {
        return Err(ToolpathError::InvalidParameter(
            "a contour profile must be counter-clockwise in work xy".into(),
        ));
    }

    let mut offset = Vec::with_capacity(profile.len());
    for index in 0..profile.len() {
        let previous = profile[(index + profile.len() - 1) % profile.len()];
        let current = profile[index];
        let next = profile[(index + 1) % profile.len()];
        let n_previous = outward_normal(previous, current)?;
        let n_next = outward_normal(current, next)?;
        let mut bisector = [n_previous[0] + n_next[0], n_previous[1] + n_next[1]];
        let length = (bisector[0] * bisector[0] + bisector[1] * bisector[1]).sqrt();
        if length < 1.0e-9 {
            return Err(ToolpathError::InvalidParameter(
                "the profile reverses on itself, so it cannot be offset".into(),
            ));
        }
        bisector = [bisector[0] / length, bisector[1] / length];
        let cosine = bisector[0] * n_previous[0] + bisector[1] * n_previous[1];
        if cosine.abs() < 1.0e-6 {
            return Err(ToolpathError::InvalidParameter(
                "the profile has a cusp, so it cannot be offset".into(),
            ));
        }
        let scale = distance / cosine;
        offset.push([
            current[0] + bisector[0] * scale,
            current[1] + bisector[1] * scale,
        ]);
    }
    Ok(offset)
}

/// Accumulates points while enforcing the retract-before-rapid discipline.
struct Planner<'a> {
    setup: &'a Setup,
    tool: &'a Tool,
    points: Vec<ToolpathPoint>,
    top: f64,
    clearance: f64,
}

impl<'a> Planner<'a> {
    fn new(setup: &'a Setup, tool: &'a Tool) -> Self {
        Planner {
            setup,
            tool,
            points: Vec::new(),
            top: setup.stock.max[2],
            clearance: setup.clearance_mm,
        }
    }

    fn rapid(&mut self, position: [f64; 3]) {
        self.points.push(ToolpathPoint::rapid(position));
    }

    fn plunge(&mut self, position: [f64; 3]) {
        self.points.push(ToolpathPoint::plunge(
            position,
            self.tool.cutting.plunge_feed_mm_min,
        ));
    }

    fn feed(&mut self, position: [f64; 3]) {
        self.points
            .push(ToolpathPoint::feed(position, self.tool.cutting.feed_mm_min));
    }

    /// Rapid to the clearance plane over an xy point, then plunge to `z`.
    ///
    /// When the previous retract already left the tool at the clearance plane over this point (the
    /// next depth level of a closed contour, or the first raster line inside a pocket loop), the
    /// rapid would be a zero-length move, so only the plunge is emitted.
    fn enter(&mut self, xy: [f64; 2], z: f64) {
        let already_above = match self.points.last() {
            Some(last) => {
                (last.position[2] - self.clearance).abs() <= POSITION_TOLERANCE_MM
                    && (last.position[0] - xy[0]).abs() <= POSITION_TOLERANCE_MM
                    && (last.position[1] - xy[1]).abs() <= POSITION_TOLERANCE_MM
            }
            None => false,
        };
        if !already_above {
            self.rapid([xy[0], xy[1], self.clearance]);
        }
        self.plunge([xy[0], xy[1], z]);
    }

    /// Feed straight up to the clearance plane: the only legal way to leave a cut.
    fn retract(&mut self) {
        let Some(last) = self.points.last().copied() else {
            return;
        };
        self.feed([last.position[0], last.position[1], self.clearance]);
    }

    /// The stepdown to use, clamped to the tool's maximum depth of cut.
    fn stepdown(&self, requested: f64, operation: &str) -> Result<f64, ToolpathError> {
        if !requested.is_finite() || requested <= 0.0 {
            return Err(ToolpathError::InvalidParameter(format!(
                "{operation} needs a positive stepdown, got {requested} mm"
            )));
        }
        Ok(requested.min(self.tool.max_depth_of_cut_mm))
    }

    fn facing(&mut self, depth: f64, stepover: f64) -> Result<(), ToolpathError> {
        if !stepover.is_finite() || stepover <= 0.0 {
            return Err(ToolpathError::InvalidParameter(format!(
                "facing needs a positive stepover, got {stepover} mm"
            )));
        }
        let radius = self.tool.radius_mm();
        let stock = self.setup.stock;
        let x0 = stock.min[0] - radius;
        let x1 = stock.max[0] + radius;
        let levels = levels(self.top, depth, self.tool.max_depth_of_cut_mm);
        let lines = raster(stock.min[1] - radius, stock.max[1] + radius, stepover);
        for z in levels {
            for (index, y) in lines.iter().enumerate() {
                let (start, end) = if index % 2 == 0 {
                    ([x0, *y], [x1, *y])
                } else {
                    ([x1, *y], [x0, *y])
                };
                self.enter(start, z);
                self.feed([end[0], end[1], z]);
                self.retract();
            }
        }
        Ok(())
    }

    fn contour(
        &mut self,
        profile: &[[f64; 2]],
        depth: f64,
        stepdown: f64,
        offset_mm: f64,
    ) -> Result<(), ToolpathError> {
        if !offset_mm.is_finite() || offset_mm < 0.0 {
            return Err(ToolpathError::InvalidParameter(format!(
                "a contour offset must be finite and non-negative, got {offset_mm} mm"
            )));
        }
        let stepdown = self.stepdown(stepdown, "a contour")?;
        let path = offset_polygon(profile, self.tool.radius_mm() + offset_mm)?;
        for z in levels(self.top, depth, stepdown) {
            self.enter(path[0], z);
            for point in path.iter().skip(1) {
                self.feed([point[0], point[1], z]);
            }
            self.feed([path[0][0], path[0][1], z]);
            self.retract();
        }
        Ok(())
    }

    fn pocket(
        &mut self,
        region: [[f64; 2]; 4],
        depth: f64,
        stepdown: f64,
        stepover: f64,
    ) -> Result<(), ToolpathError> {
        if !stepover.is_finite() || stepover <= 0.0 {
            return Err(ToolpathError::InvalidParameter(format!(
                "a pocket needs a positive stepover, got {stepover} mm"
            )));
        }
        let stepdown = self.stepdown(stepdown, "a pocket")?;
        let radius = self.tool.radius_mm();
        let mut min = [f64::INFINITY; 2];
        let mut max = [f64::NEG_INFINITY; 2];
        for corner in region {
            for axis in 0..2 {
                min[axis] = min[axis].min(corner[axis]);
                max[axis] = max[axis].max(corner[axis]);
            }
        }
        let inner_min = [min[0] + radius, min[1] + radius];
        let inner_max = [max[0] - radius, max[1] - radius];
        if inner_max[0] - inner_min[0] <= POSITION_TOLERANCE_MM
            || inner_max[1] - inner_min[1] <= POSITION_TOLERANCE_MM
        {
            return Err(ToolpathError::InvalidParameter(format!(
                "a {} mm tool does not fit the pocket region",
                self.tool.diameter_mm()
            )));
        }
        let loop_path = [
            [inner_min[0], inner_min[1]],
            [inner_max[0], inner_min[1]],
            [inner_max[0], inner_max[1]],
            [inner_min[0], inner_max[1]],
        ];
        for z in levels(self.top, depth, stepdown) {
            self.enter(loop_path[0], z);
            for point in loop_path.iter().skip(1) {
                self.feed([point[0], point[1], z]);
            }
            self.feed([loop_path[0][0], loop_path[0][1], z]);
            self.retract();

            for (index, y) in raster(inner_min[1], inner_max[1], stepover)
                .iter()
                .enumerate()
            {
                let (start, end) = if index % 2 == 0 {
                    ([inner_min[0], *y], [inner_max[0], *y])
                } else {
                    ([inner_max[0], *y], [inner_min[0], *y])
                };
                self.enter(start, z);
                self.feed([end[0], end[1], z]);
                self.retract();
            }
        }
        Ok(())
    }

    fn drill(&mut self, holes: &[[f64; 2]], depth: f64, peck: f64) -> Result<(), ToolpathError> {
        if holes.is_empty() {
            return Err(ToolpathError::InvalidParameter(
                "drilling needs at least one hole".into(),
            ));
        }
        if !peck.is_finite() || peck <= 0.0 {
            return Err(ToolpathError::InvalidParameter(format!(
                "drilling needs a positive peck, got {peck} mm"
            )));
        }
        for hole in holes {
            if !hole[0].is_finite()
                || !hole[1].is_finite()
                || !self
                    .setup
                    .stock
                    .contains_xy(*hole, -POSITION_TOLERANCE_MM)
            {
                return Err(ToolpathError::InvalidParameter(format!(
                    "hole at ({}, {}) is not inside the stock",
                    hole[0], hole[1]
                )));
            }
            self.rapid([hole[0], hole[1], self.clearance]);
            let mut cut = 0.0;
            while cut < depth - POSITION_TOLERANCE_MM {
                cut = (cut + peck).min(depth);
                self.plunge([hole[0], hole[1], self.top - cut]);
                if cut < depth - POSITION_TOLERANCE_MM {
                    // Peck retract clears chips back to the stock top without leaving the hole.
                    self.feed([hole[0], hole[1], self.top]);
                }
            }
            self.retract();
        }
        Ok(())
    }

    fn slot(
        &mut self,
        start: [f64; 2],
        end: [f64; 2],
        depth: f64,
        stepdown: f64,
    ) -> Result<(), ToolpathError> {
        let stepdown = self.stepdown(stepdown, "a slot")?;
        if start == end {
            return Err(ToolpathError::InvalidParameter(
                "a slot needs two distinct endpoints".into(),
            ));
        }
        for point in [start, end] {
            if !self
                .setup
                .stock
                .contains_xy(point, -POSITION_TOLERANCE_MM)
            {
                return Err(ToolpathError::InvalidParameter(format!(
                    "slot endpoint ({}, {}) is not inside the stock",
                    point[0], point[1]
                )));
            }
        }
        for z in levels(self.top, depth, stepdown) {
            self.enter(start, z);
            self.feed([end[0], end[1], z]);
            self.retract();
        }
        Ok(())
    }

    /// Z-level roughing from the sampled part surface.
    fn rough3d(
        &mut self,
        part: &Mesh,
        stepover: f64,
        stepdown: f64,
        stock_to_leave: f64,
        resolution: f64,
    ) -> Result<(), ToolpathError> {
        if !stepover.is_finite() || stepover <= 0.0 {
            return Err(ToolpathError::InvalidParameter(format!(
                "3D roughing needs a positive stepover, got {stepover} mm"
            )));
        }
        if !stock_to_leave.is_finite() || stock_to_leave < 0.0 {
            return Err(ToolpathError::InvalidParameter(format!(
                "3D roughing needs a finite, non-negative stock to leave, got {stock_to_leave} mm"
            )));
        }
        let stepdown = self.stepdown(stepdown, "3D roughing")?;
        let stock = self.setup.stock;
        let field = HeightField::from_mesh(part, stock, resolution)
            .map_err(|error| ToolpathError::InvalidParameter(error.to_string()))?;
        let lowest = field.lowest().ok_or(ToolpathError::NothingToMachine)?;
        if lowest + stock_to_leave >= self.top - POSITION_TOLERANCE_MM {
            return Err(ToolpathError::NothingToMachine);
        }

        // The block bottom keeps the allowance as a skin; outside the part footprint there is no
        // part, so the stock there is cleared all the way down to that skin.
        let floor = stock.min[2] + stock_to_leave;
        let radius = self.tool.radius_mm();
        let cell = field.cell_size();
        let sample_step = cell[0].max(POSITION_TOLERANCE_MM * 10.0);
        let xs = raster(stock.min[0], stock.max[0], sample_step);
        let ys = raster(stock.min[1], stock.max[1], stepover);
        let mut cut_any = false;
        for z in levels(self.top, self.top - floor, stepdown) {
            for y in &ys {
                let mut start: Option<f64> = None;
                let mut last = stock.min[0];
                for x in &xs {
                    let height = field.height_at(*x, *y);
                    // No part here: clear it. Above part material: cut only if the allowance would
                    // still be left, so a part face is never touched.
                    let cuttable = !height.is_finite()
                        || height + stock_to_leave < z - POSITION_TOLERANCE_MM;
                    match (cuttable, start) {
                        (true, None) => start = Some(*x),
                        (false, Some(begin)) => {
                            cut_any |= self.cut_interval(begin, last, *y, z, radius, sample_step);
                            start = None;
                        }
                        _ => {}
                    }
                    last = *x;
                }
                if let Some(begin) = start {
                    cut_any |= self.cut_interval(begin, last, *y, z, radius, sample_step);
                }
            }
        }
        if !cut_any {
            return Err(ToolpathError::NothingToMachine);
        }
        Ok(())
    }

    /// Ball-nose surface finishing along a raster of the sampled part surface.
    ///
    /// The tool centre is held one ball radius above the surface point, which is exact on a
    /// horizontal face and conservative on a slope (the ball would need less than a radius along z
    /// there, so the path stays clear instead of gouging).
    fn finish3d(
        &mut self,
        part: &Mesh,
        stepover: f64,
        stock_to_leave: f64,
        resolution: f64,
    ) -> Result<(), ToolpathError> {
        if !matches!(self.tool.geometry, ToolGeometry::BallEndMill { .. }) {
            return Err(ToolpathError::InvalidParameter(
                "3D finishing follows the surface with a ball nose cutter; use a ball end mill"
                    .into(),
            ));
        }
        if !stepover.is_finite() || stepover <= 0.0 {
            return Err(ToolpathError::InvalidParameter(format!(
                "3D finishing needs a positive stepover, got {stepover} mm"
            )));
        }
        if !stock_to_leave.is_finite() || stock_to_leave < 0.0 {
            return Err(ToolpathError::InvalidParameter(format!(
                "3D finishing needs a finite, non-negative stock to leave, got {stock_to_leave} mm"
            )));
        }
        let stock = self.setup.stock;
        let field = HeightField::from_mesh(part, stock, resolution)
            .map_err(|error| ToolpathError::InvalidParameter(error.to_string()))?;
        let radius = self.tool.radius_mm();
        let highest = field.highest().ok_or(ToolpathError::NothingToMachine)?;
        if highest + radius + stock_to_leave >= self.clearance - POSITION_TOLERANCE_MM {
            return Err(ToolpathError::InvalidParameter(format!(
                "the clearance plane ({:.1} mm) is below the finishing surface plus the ball radius ({:.1} mm)",
                self.clearance,
                highest + radius + stock_to_leave
            )));
        }

        let cell = field.cell_size();
        let sample = cell[0].max(POSITION_TOLERANCE_MM * 10.0);
        let xs = raster(stock.min[0], stock.max[0], sample);
        let ys = raster(stock.min[1], stock.max[1], stepover);
        let mut cut_any = false;
        for (line_index, y) in ys.iter().enumerate() {
            let mut points: Vec<[f64; 3]> = Vec::new();
            for x in &xs {
                let height = field.height_at(*x, *y);
                if height.is_finite() {
                    points.push([*x, *y, height + radius + stock_to_leave]);
                }
            }
            if line_index % 2 == 1 {
                points.reverse();
            }
            let mut run: Vec<[f64; 3]> = Vec::new();
            let mut previous: Option<[f64; 3]> = None;
            for point in points {
                let gap = match previous {
                    Some(previous) => (point[0] - previous[0]).abs() > sample * 1.5 + POSITION_TOLERANCE_MM,
                    None => false,
                };
                if gap && run.len() >= 2 {
                    self.cut_surface(&run);
                    cut_any = true;
                    run.clear();
                } else if gap {
                    run.clear();
                }
                run.push(point);
                previous = Some(point);
            }
            if run.len() >= 2 {
                self.cut_surface(&run);
                cut_any = true;
            }
        }
        if !cut_any {
            return Err(ToolpathError::NothingToMachine);
        }
        Ok(())
    }

    /// Z-level wall finishing from the sampled part cross-section.
    ///
    /// At each level the cells where part material still stands form a mask. The tool centre follows
    /// the contour of the distance to that mask at `radius + leave + cell`, so the tool edge stays at
    /// least one sampling cell outside every sampled wall: the pass removes the one-radius shell a
    /// roughing pass left behind without gouging the part. About one cell of stock stays on the wall,
    /// which is the price of a grid-sampled wall; sample finer for a thinner wall.
    fn finish_walls(
        &mut self,
        part: &Mesh,
        stepdown: f64,
        stock_to_leave: f64,
        resolution: f64,
    ) -> Result<(), ToolpathError> {
        if !stock_to_leave.is_finite() || stock_to_leave < 0.0 {
            return Err(ToolpathError::InvalidParameter(format!(
                "wall finishing needs a finite, non-negative stock to leave, got {stock_to_leave} mm"
            )));
        }
        let stepdown = self.stepdown(stepdown, "wall finishing")?;
        let stock = self.setup.stock;
        let field = HeightField::from_mesh(part, stock, resolution)
            .map_err(|error| ToolpathError::InvalidParameter(error.to_string()))?;
        if field.highest().is_none() {
            return Err(ToolpathError::NothingToMachine);
        }
        let (columns, rows) = field.dims();
        if columns < 2 || rows < 2 {
            return Err(ToolpathError::InvalidParameter(
                "wall finishing needs a sampling grid of at least 2 x 2 cells".into(),
            ));
        }
        let cell = field.cell_size();
        let origin = field.origin();
        let heights = field.heights();
        let radius = self.tool.radius_mm();
        let margin = cell[0].max(cell[1]);
        let target = radius + stock_to_leave + margin;
        let reach = (target / cell[0].min(cell[1])).ceil() as isize + 1;
        let floor = stock.min[2] + stock_to_leave;
        let mut mask = vec![false; columns * rows];
        let mut distance = vec![f64::INFINITY; columns * rows];
        let mut cut_any = false;
        for z in levels(self.top, self.top - floor, stepdown) {
            let mut any = false;
            for index in 0..mask.len() {
                let height = heights[index];
                mask[index] =
                    height.is_finite() && height + stock_to_leave >= z - POSITION_TOLERANCE_MM;
                any |= mask[index];
            }
            if !any {
                continue;
            }
            // Exact distance to the nearest standing cell, examined only as far as the contour can
            // reach, so the cost stays bounded by the tool size rather than the stock area.
            for row in 0..rows {
                for column in 0..columns {
                    let mut nearest = f64::INFINITY;
                    for row_offset in -reach..=reach {
                        for column_offset in -reach..=reach {
                            let r = row as isize + row_offset;
                            let c = column as isize + column_offset;
                            if r < 0 || c < 0 || r >= rows as isize || c >= columns as isize {
                                continue;
                            }
                            if !mask[r as usize * columns + c as usize] {
                                continue;
                            }
                            let dx = column_offset as f64 * cell[0];
                            let dy = row_offset as f64 * cell[1];
                            nearest = nearest.min((dx * dx + dy * dy).sqrt());
                        }
                    }
                    distance[row * columns + column] = nearest;
                }
            }
            for line in contour_lines(&distance, columns, rows, target) {
                for run in wall_runs(&line, origin, cell, stock, z) {
                    self.cut_surface(&run);
                    cut_any = true;
                }
            }
        }
        if !cut_any {
            return Err(ToolpathError::NothingToMachine);
        }
        Ok(())
    }

    /// Follow one continuous run of surface points, retracting at both ends.
    fn cut_surface(&mut self, run: &[[f64; 3]]) {
        self.rapid([run[0][0], run[0][1], self.clearance]);
        self.plunge(run[0]);
        for point in &run[1..] {
            self.feed(*point);
        }
        self.retract();
    }

    /// Cut one raster interval at depth `z`, leaving one tool radius at a part wall.
    ///
    /// At the stock boundary the tool is allowed to overhang, so the edge of the block is cleared
    /// rather than left with a rim.
    fn cut_interval(
        &mut self,
        begin: f64,
        end: f64,
        y: f64,
        z: f64,
        radius: f64,
        sample_step: f64,
    ) -> bool {
        let stock = self.setup.stock;
        let start = if begin > stock.min[0] + sample_step {
            begin + radius
        } else {
            begin
        };
        let finish = if end < stock.max[0] - sample_step {
            end - radius
        } else {
            end
        };
        if finish - start <= POSITION_TOLERANCE_MM {
            return false;
        }
        self.enter([start, y], z);
        self.feed([finish, y, z]);
        self.retract();
        true
    }
}

/// The polylines where `values` crosses `level`, in grid-index coordinates.
fn contour_lines(values: &[f64], columns: usize, rows: usize, level: f64) -> Vec<Vec<[f64; 2]>> {
    let mut segments: Vec<([f64; 2], [f64; 2])> = Vec::new();
    for row in 0..rows.saturating_sub(1) {
        for column in 0..columns.saturating_sub(1) {
            let corners = [
                values[row * columns + column],
                values[row * columns + column + 1],
                values[(row + 1) * columns + column + 1],
                values[(row + 1) * columns + column],
            ];
            let points = [
                [column as f64, row as f64],
                [column as f64 + 1.0, row as f64],
                [column as f64 + 1.0, row as f64 + 1.0],
                [column as f64, row as f64 + 1.0],
            ];
            let mut crossings: Vec<[f64; 2]> = Vec::new();
            for edge in 0..4 {
                let (a, b) = (corners[edge], corners[(edge + 1) % 4]);
                if (a >= level) == (b >= level) {
                    continue;
                }
                let t = (level - a) / (b - a);
                let (start, end) = (points[edge], points[(edge + 1) % 4]);
                crossings.push([
                    start[0] + (end[0] - start[0]) * t,
                    start[1] + (end[1] - start[1]) * t,
                ]);
            }
            match crossings.len() {
                2 => segments.push((crossings[0], crossings[1])),
                // A saddle: either pairing keeps every point on the same iso line.
                4 => {
                    segments.push((crossings[0], crossings[1]));
                    segments.push((crossings[2], crossings[3]));
                }
                _ => {}
            }
        }
    }
    stitch(segments)
}

/// Join segments that share an endpoint into polylines.
fn stitch(segments: Vec<([f64; 2], [f64; 2])>) -> Vec<Vec<[f64; 2]>> {
    let mut remaining: Vec<Option<([f64; 2], [f64; 2])>> =
        segments.into_iter().map(Some).collect();
    let mut lines: Vec<Vec<[f64; 2]>> = Vec::new();
    for start in 0..remaining.len() {
        let Some((head, tail)) = remaining[start].take() else {
            continue;
        };
        let mut line = vec![head, tail];
        let mut extended = true;
        while extended {
            extended = false;
            for slot in &mut remaining {
                let Some((a, b)) = *slot else {
                    continue;
                };
                let tail_point = line[line.len() - 1];
                if close(a, tail_point) {
                    line.push(b);
                } else if close(b, tail_point) {
                    line.push(a);
                } else if close(a, line[0]) {
                    line.insert(0, b);
                } else if close(b, line[0]) {
                    line.insert(0, a);
                } else {
                    continue;
                }
                *slot = None;
                extended = true;
            }
        }
        lines.push(line);
    }
    lines
}

fn close(a: [f64; 2], b: [f64; 2]) -> bool {
    (a[0] - b[0]).abs() <= 1.0e-9 && (a[1] - b[1]).abs() <= 1.0e-9
}

/// Convert one grid polyline into cutting runs in work coordinates, clipped to the stock footprint.
fn wall_runs(
    line: &[[f64; 2]],
    origin: [f64; 2],
    cell: [f64; 2],
    stock: Stock,
    z: f64,
) -> Vec<Vec<[f64; 3]>> {
    let mut runs: Vec<Vec<[f64; 3]>> = Vec::new();
    let mut run: Vec<[f64; 3]> = Vec::new();
    let mut previous: Option<[f64; 2]> = None;
    let gap = cell[0].hypot(cell[1]) * 1.5;
    for node in line {
        let point = [
            origin[0] + (node[0] + 0.5) * cell[0],
            origin[1] + (node[1] + 0.5) * cell[1],
        ];
        let inside = stock.contains_xy(point, POSITION_TOLERANCE_MM);
        let adjacent = previous
            .map(|previous| (point[0] - previous[0]).hypot(point[1] - previous[1]) <= gap)
            .unwrap_or(false);
        if !inside || (!run.is_empty() && !adjacent) {
            if run.len() >= 2 {
                runs.push(std::mem::take(&mut run));
            } else {
                run.clear();
            }
        }
        if inside {
            run.push([point[0], point[1], z]);
            previous = Some(point);
        } else {
            previous = None;
        }
    }
    if run.len() >= 2 {
        runs.push(run);
    }
    runs
}

/// The kinds of moves a toolpath contains, in order, without the geometry: a compact signature a
/// regression test can compare.
pub fn move_signature(toolpath: &Toolpath) -> Vec<MoveKind> {
    toolpath.points.iter().map(|point| point.kind).collect()
}

/// The stock a roughing operation starts from, for a caller that wants to display it.
pub fn stock_reference(stock: Stock) -> Mesh {
    stock.to_mesh()
}
