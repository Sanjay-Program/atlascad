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
        out
    }
}
