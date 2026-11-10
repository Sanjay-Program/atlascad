//! Setup sheet and tool list, both as Markdown.
//!
//! A setup sheet is the document a machinist works from: the machine, the stock, the clearance
//! plane, the tools in the order they are used and the operations with their measured move counts.
//! It is generated from the same values that produced the toolpath, so it cannot drift from the
//! program it describes.

use crate::setup::{Setup, Stock};
use crate::tool::{Machine, Tool, ToolId};
use crate::tool_path::Toolpath;
use crate::tool_path_visitor::machine_name;

/// One tool as it appears on the sheet.
#[derive(Debug, Clone, PartialEq)]
pub struct SheetTool {
    pub id: ToolId,
    pub name: String,
    pub family: &'static str,
    pub diameter_mm: f64,
    pub spindle_rpm: f64,
    pub feed_mm_min: f64,
    pub plunge_feed_mm_min: f64,
    pub max_depth_of_cut_mm: f64,
}

impl SheetTool {
    /// Capture a tool's shop-floor numbers.
    pub fn from_tool(tool: &Tool) -> Self {
        SheetTool {
            id: tool.id,
            name: tool.name.clone(),
            family: tool.geometry.family(),
            diameter_mm: tool.diameter_mm(),
            spindle_rpm: tool.cutting.spindle_rpm,
            feed_mm_min: tool.cutting.feed_mm_min,
            plunge_feed_mm_min: tool.cutting.plunge_feed_mm_min,
            max_depth_of_cut_mm: tool.max_depth_of_cut_mm,
        }
    }
}

/// One operation as it appears on the sheet.
#[derive(Debug, Clone, PartialEq)]
pub struct SheetOperation {
    pub name: String,
    pub tool_id: ToolId,
    pub moves: usize,
    pub cutting_mm: f64,
    pub rapid_mm: f64,
    pub estimated_minutes: f64,
    pub depth_mm: Option<f64>,
    pub bounds: ([f64; 3], [f64; 3]),
}

/// A setup sheet for one job.
#[derive(Debug, Clone, PartialEq)]
pub struct SetupSheet {
    pub title: String,
    pub machine: Machine,
    pub stock: Stock,
    pub clearance_mm: f64,
    pub wcs_origin: [f64; 3],
    pub wcs_z_axis: [f64; 3],
    pub tools: Vec<SheetTool>,
    pub operations: Vec<SheetOperation>,
    pub material: Option<crate::feeds::Material>,
}

impl SetupSheet {
    /// A sheet for an existing setup.
    pub fn new(title: impl Into<String>, setup: &Setup) -> Self {
        SetupSheet {
            title: title.into(),
            machine: setup.machine,
            stock: setup.stock,
            clearance_mm: setup.clearance_mm,
            wcs_origin: setup.wcs.origin,
            wcs_z_axis: setup.wcs.z_axis,
            tools: Vec::new(),
            operations: Vec::new(),
            material: None,
        }
    }

    /// Attach a workpiece material for tool wear and cutting physics estimation.
    pub fn with_material(mut self, material: crate::feeds::Material) -> Self {
        self.material = Some(material);
        self
    }

    /// Calculate tool wear estimates for all tools used in this setup based on the workpiece material.
    pub fn tool_wear_estimates(
        &self,
        material: crate::feeds::Material,
    ) -> Vec<(ToolId, crate::feeds::ToolWearEstimate)> {
        let mut estimates = Vec::new();
        for tool in &self.tools {
            let cutting_time_min: f64 = self
                .operations
                .iter()
                .filter(|op| op.tool_id == tool.id)
                .map(|op| {
                    if tool.feed_mm_min > 0.0 {
                        op.cutting_mm / tool.feed_mm_min
                    } else {
                        op.estimated_minutes
                    }
                })
                .sum();
            let surface_speed = std::f64::consts::PI * tool.diameter_mm * tool.spindle_rpm / 1000.0;
            let wear = material.estimate_tool_wear(surface_speed, cutting_time_min);
            estimates.push((tool.id, wear));
        }
        estimates
    }

    /// Estimate kinematic cycle time across all operations in this setup.
    pub fn kinematic_cycle_time(
        &self,
        limits: &crate::tool_path::KinematicLimits,
    ) -> crate::tool_path::CycleTimeEstimate {
        let mut cutting_seconds = 0.0;
        let mut rapid_seconds = 0.0;
        let accel_cut = limits.cutting_acceleration_mm_s2.max(10.0);
        let accel_rapid = limits.rapid_acceleration_mm_s2.max(10.0);
        let v_rapid = limits.rapid_feed_mm_min / 60.0;

        for op in &self.operations {
            let tool = self.tools.iter().find(|t| t.id == op.tool_id);
            let feed_mm_min = tool.map(|t| t.feed_mm_min).unwrap_or(500.0);
            let v_cut = (feed_mm_min / 60.0).max(0.1);

            let avg_moves = op.moves.max(1) as f64;
            let avg_cut_len = op.cutting_mm / avg_moves;
            let avg_rapid_len = op.rapid_mm / avg_moves;

            let d_acc_cut = (v_cut * v_cut) / accel_cut;
            let t_cut_seg = if avg_cut_len >= d_acc_cut {
                (v_cut / accel_cut) + (avg_cut_len / v_cut)
            } else {
                2.0 * (avg_cut_len / accel_cut).sqrt()
            };
            cutting_seconds += t_cut_seg * avg_moves;

            let d_acc_rapid = (v_rapid * v_rapid) / accel_rapid;
            let t_rapid_seg = if avg_rapid_len >= d_acc_rapid {
                (v_rapid / accel_rapid) + (avg_rapid_len / v_rapid)
            } else {
                2.0 * (avg_rapid_len / accel_rapid).sqrt()
            };
            rapid_seconds += t_rapid_seg * avg_moves;
        }

        let spindle_dwell = limits.spindle_dwell_seconds * (self.operations.len().max(1) as f64);
        let tool_change = limits.tool_change_seconds * (self.tools.len().saturating_sub(1) as f64);
        let total_seconds = cutting_seconds + rapid_seconds + spindle_dwell + tool_change;

        crate::tool_path::CycleTimeEstimate {
            cutting_time_seconds: cutting_seconds,
            rapid_time_seconds: rapid_seconds,
            dwell_time_seconds: spindle_dwell,
            tool_change_time_seconds: tool_change,
            total_seconds,
            total_minutes: total_seconds / 60.0,
        }
    }

    /// Add a tool once, keeping the sheet's tool list free of duplicates.
    pub fn add_tool(&mut self, tool: &Tool) {
        if !self.tools.iter().any(|listed| listed.id == tool.id) {
            self.tools.push(SheetTool::from_tool(tool));
        }
    }

    /// Add an operation and the tool it uses.
    pub fn add_operation(&mut self, name: impl Into<String>, tool: &Tool, toolpath: &Toolpath) {
        self.add_tool(tool);
        self.operations.push(SheetOperation {
            name: name.into(),
            tool_id: tool.id,
            moves: toolpath.points.len(),
            cutting_mm: toolpath.cutting_length_mm(),
            rapid_mm: toolpath.rapid_length_mm(),
            estimated_minutes: toolpath.estimated_minutes(),
            depth_mm: None,
            bounds: toolpath.bounds(),
        });
    }

    /// Record the total depth of the operation just added.
    pub fn set_last_depth(&mut self, depth_mm: Option<f64>) {
        if let Some(operation) = self.operations.last_mut() {
            operation.depth_mm = depth_mm;
        }
    }

    /// The tool table on its own.
    pub fn tool_list(&self) -> String {
        if self.tools.is_empty() {
            return "_No tools in this setup._\n".to_string();
        }
        let mut out = String::from(
            "| Tool | Name | Type | Diameter (mm) | Spindle (rpm) | Feed (mm/min) | Plunge (mm/min) | Max DOC (mm) |\n\
             |---|---|---|---|---|---|---|---|\n",
        );
        for tool in &self.tools {
            out.push_str(&format!(
                "| T{} | {} | {} | {:.1} | {:.0} | {:.0} | {:.0} | {:.1} |\n",
                tool.id.get(),
                tool.name,
                tool.family,
                tool.diameter_mm,
                tool.spindle_rpm,
                tool.feed_mm_min,
                tool.plunge_feed_mm_min,
                tool.max_depth_of_cut_mm
            ));
        }
        out
    }

    /// The whole sheet as Markdown.
    pub fn render(&self) -> String {
        let size = self.stock.size();
        let mut out = String::new();
        out.push_str(&format!("# Setup sheet — {}\n\n", self.title));
        out.push_str(&format!(
            "| Field | Value |\n|---|---|\n| Machine | {} |\n| Stock (work frame) | {:.1} x {:.1} x {:.1} mm |\n| Stock volume | {:.0} mm³ |\n| Clearance plane | {:.1} mm |\n| WCS origin | ({:.1}, {:.1}, {:.1}) |\n| WCS z axis | ({:.1}, {:.1}, {:.1}) |\n\n",
            machine_name(self.machine),
            size[0],
            size[1],
            size[2],
            size[0] * size[1] * size[2],
            self.clearance_mm,
            self.wcs_origin[0],
            self.wcs_origin[1],
            self.wcs_origin[2],
            self.wcs_z_axis[0],
            self.wcs_z_axis[1],
            self.wcs_z_axis[2]
        ));
        out.push_str("## Tools\n\n");
        out.push_str(&self.tool_list());
        out.push('\n');
        out.push_str("## Operations\n\n");
        if self.operations.is_empty() {
            out.push_str("_No operations in this job._\n");
            return out;
        }
        out.push_str(
            "| Operation | Tool | Moves | Cutting (mm) | Rapid (mm) | Time (min) | Depth (mm) | X range | Y range | Z range |\n\
             |---|---|---|---|---|---|---|---|---|---|\n",
        );
        for operation in &self.operations {
            let depth = match operation.depth_mm {
                Some(depth) => format!("{depth:.1}"),
                None => "—".to_string(),
            };
            let (min, max) = operation.bounds;
            out.push_str(&format!(
                "| {} | T{} | {} | {:.1} | {:.1} | {:.2} | {} | {:.1} .. {:.1} | {:.1} .. {:.1} | {:.1} .. {:.1} |\n",
                operation.name,
                operation.tool_id.get(),
                operation.moves,
                operation.cutting_mm,
                operation.rapid_mm,
                operation.estimated_minutes,
                depth,
                min[0],
                max[0],
                min[1],
                max[1],
                min[2],
                max[2]
            ));
        }

        if let Some(mat) = self.material {
            out.push_str("\n## Kinematics & Tool Wear\n\n");
            let limits = crate::tool_path::KinematicLimits::default();
            let cycle = self.kinematic_cycle_time(&limits);
            out.push_str(&format!(
                "Workpiece material: **{}**\n\n\
                 - Estimated Kinematic Cycle Time: **{:.2} min** ({:.1} s)\n\
                 - Motion Breakdown: cutting {:.1} s, rapids {:.1} s, dwell {:.1} s, tool changes {:.1} s\n\n",
                mat.label(),
                cycle.total_minutes,
                cycle.total_seconds,
                cycle.cutting_time_seconds,
                cycle.rapid_time_seconds,
                cycle.dwell_time_seconds,
                cycle.tool_change_time_seconds,
            ));

            out.push_str(
                "| Tool | Surface Speed (m/min) | Cut Time (min) | Taylor Life (min) | Wear Fraction | Wear % | Remaining (min) |\n\
                 |---|---|---|---|---|---|---|\n",
            );
            for (tool_id, wear) in self.tool_wear_estimates(mat) {
                out.push_str(&format!(
                    "| T{} | {:.1} | {:.2} | {:.1} | {:.4} | {:.2}% | {:.1} |\n",
                    tool_id.get(),
                    wear.surface_speed_m_min,
                    wear.cutting_time_minutes,
                    wear.tool_life_minutes,
                    wear.wear_fraction,
                    wear.wear_percent,
                    wear.remaining_life_minutes,
                ));
            }
        }

        out
    }
}
