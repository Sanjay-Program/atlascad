//! Text views of a toolpath: a readable move list and a one-line summary.
//!
//! These are for reports and test failure messages. They are not a postprocessor: a control needs
//! G-code from [`crate::gcode`].

use crate::setup::Setup;
use crate::tool::Machine;
use crate::tool_path::Toolpath;

/// The human-readable name of a machine, for reporting only.
pub fn machine_name(machine: Machine) -> &'static str {
    match machine {
        Machine::Milling3Axis => "3-axis milling",
        Machine::Milling4Axis => "4-axis indexing",
        Machine::Milling5Axis => "5-axis simultaneous",
        Machine::Turning => "turning",
    }
}

/// Render the toolpath as one aligned line per move.
pub fn show_toolpath(toolpath: &Toolpath) -> String {
    let mut out = String::new();
    for (index, point) in toolpath.points.iter().enumerate() {
        out.push_str(&format!(
            "{index:>4} {:<6} X{:>10.4} Y{:>10.4} Z{:>10.4} F{:>8.1}\n",
            point.kind.name(),
            point.position[0],
            point.position[1],
            point.position[2],
            point.feed_mm_min
        ));
    }
    out
}

/// A one-line summary: toolpath id, tool, move count, cut length and estimated time.
pub fn summarize(toolpath: &Toolpath, setup: &Setup) -> String {
    format!(
        "{} with {}: {} moves, {:.1} mm cutting, {:.1} mm rapid, {:.2} min on {}",
        toolpath.id,
        toolpath.tool_id,
        toolpath.points.len(),
        toolpath.cutting_length_mm(),
        toolpath.rapid_length_mm(),
        toolpath.estimated_minutes(),
        machine_name(setup.machine)
    )
}
