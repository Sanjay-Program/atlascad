//! 3D Mesh and solid slicing pipeline for additive manufacturing.

use crate::estimation::PrintEstimation;
use crate::gcode::GCodeEmitter;
use crate::infill::{InfillLine, generate_infill};
use crate::settings::SlicerSettings;
use serde::{Deserialize, Serialize};

/// Toolpath classification for additive visualization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MoveType {
    Travel,
    OuterWall,
    InnerWall,
    Infill,
    Support,
}

/// A single discrete layer within the sliced 3D model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SlicedLayer {
    pub layer_index: usize,
    pub z_height_mm: f64,
    pub wall_moves: Vec<InfillLine>,
    pub infill_moves: Vec<InfillLine>,
    pub extrusion_distance_mm: f64,
    pub travel_distance_mm: f64,
}

/// Complete slicing result for an additive manufacturing job.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SlicedModel {
    pub model_name: String,
    pub bounds_min: [f64; 3],
    pub bounds_max: [f64; 3],
    pub layers: Vec<SlicedLayer>,
    pub estimation: PrintEstimation,
    pub gcode: String,
}

/// Slices a 3D bounding geometry according to slicer settings.
#[allow(clippy::too_many_arguments)]
pub fn slice_solid_bounds(
    model_name: impl Into<String>,
    min_x: f64,
    min_y: f64,
    min_z: f64,
    max_x: f64,
    max_y: f64,
    max_z: f64,
    settings: &SlicerSettings,
) -> SlicedModel {
    let name = model_name.into();
    let mut current_z = min_z + settings.initial_layer_height_mm;
    let mut layer_idx = 0;

    let mut layers = Vec::new();
    let mut total_extrusion_dist = 0.0;
    let mut total_travel_dist = 0.0;
    let mut total_moves = 0;
    let mut retractions = 0;

    let mut emitter = GCodeEmitter::new(settings);
    emitter.emit_header();

    let mut last_x = 0.0;
    let mut last_y = 0.0;

    while current_z <= max_z + 1e-4 {
        emitter.emit_layer_start(layer_idx, current_z);

        let mut layer_extrusion = 0.0;
        let mut layer_travel = 0.0;
        let mut wall_moves = Vec::new();

        // 1. Generate perimeter walls
        for p in 0..settings.wall_perimeters {
            let offset = (p as f64) * settings.line_width_mm;
            let wx1 = min_x + offset;
            let wy1 = min_y + offset;
            let wx2 = max_x - offset;
            let wy2 = max_y - offset;

            if wx1 < wx2 && wy1 < wy2 {
                // Rapid travel to start of perimeter
                let travel_len = ((wx1 - last_x).powi(2) + (wy1 - last_y).powi(2)).sqrt();
                layer_travel += travel_len;
                emitter.emit_travel(wx1, wy1);

                // 4 perimeter edges
                let edges = [(wx2, wy1), (wx2, wy2), (wx1, wy2), (wx1, wy1)];

                let mut cur_px = wx1;
                let mut cur_py = wy1;
                for &(nx, ny) in &edges {
                    let edge_line = InfillLine::new(cur_px, cur_py, nx, ny);
                    let len = edge_line.length();
                    layer_extrusion += len;
                    emitter.emit_extrusion(nx, ny, len);
                    wall_moves.push(edge_line);
                    cur_px = nx;
                    cur_py = ny;
                    total_moves += 1;
                }

                last_x = cur_px;
                last_y = cur_py;
            }
        }

        // 2. Generate infill lines
        let wall_inset = (settings.wall_perimeters as f64) * settings.line_width_mm;
        let infill_min_x = min_x + wall_inset;
        let infill_min_y = min_y + wall_inset;
        let infill_max_x = max_x - wall_inset;
        let infill_max_y = max_y - wall_inset;

        let infill_moves = if infill_min_x < infill_max_x && infill_min_y < infill_max_y {
            generate_infill(
                settings.infill_pattern,
                infill_min_x,
                infill_min_y,
                infill_max_x,
                infill_max_y,
                settings.infill_density,
                settings.line_width_mm,
                layer_idx,
            )
        } else {
            Vec::new()
        };

        for infill_line in &infill_moves {
            // Travel to infill line start
            let travel_len = ((infill_line.start[0] - last_x).powi(2)
                + (infill_line.start[1] - last_y).powi(2))
            .sqrt();
            if travel_len > 1.0 {
                emitter.emit_retraction();
                retractions += 1;
                layer_travel += travel_len;
                emitter.emit_travel(infill_line.start[0], infill_line.start[1]);
                emitter.emit_unretraction();
            }

            let len = infill_line.length();
            layer_extrusion += len;
            emitter.emit_extrusion(infill_line.end[0], infill_line.end[1], len);
            total_moves += 1;

            last_x = infill_line.end[0];
            last_y = infill_line.end[1];
        }

        total_extrusion_dist += layer_extrusion;
        total_travel_dist += layer_travel;

        layers.push(SlicedLayer {
            layer_index: layer_idx,
            z_height_mm: current_z,
            wall_moves,
            infill_moves,
            extrusion_distance_mm: layer_extrusion,
            travel_distance_mm: layer_travel,
        });

        layer_idx += 1;
        current_z += settings.layer_height_mm;
    }

    let estimation = PrintEstimation::compute(
        total_extrusion_dist,
        total_travel_dist,
        layers.len(),
        total_moves,
        retractions,
        settings,
    );

    let gcode = emitter.finish();

    SlicedModel {
        model_name: name,
        bounds_min: [min_x, min_y, min_z],
        bounds_max: [max_x, max_y, max_z],
        layers,
        estimation,
        gcode,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slice_solid_bounds_complete() {
        let settings = SlicerSettings::default();
        let sliced = slice_solid_bounds("test-part", 0.0, 0.0, 0.0, 30.0, 20.0, 10.0, &settings);
        assert!(!sliced.layers.is_empty());
        assert!(sliced.layers.len() > 40);
        assert!(sliced.estimation.total_print_time_seconds > 0.0);
        assert!(sliced.estimation.filament_mass_grams > 0.0);
        assert!(sliced.gcode.contains(";LAYER:0"));
        assert!(sliced.gcode.contains("M104 S0"));
    }
}
