//! Analytical print time, filament mass, and manufacturing cost estimation.

use crate::settings::SlicerSettings;
use serde::{Deserialize, Serialize};

/// Detailed manufacturing estimation metrics for additive manufacturing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PrintEstimation {
    pub total_print_time_seconds: f64,
    pub formatted_time: String,
    pub filament_length_mm: f64,
    pub filament_volume_cm3: f64,
    pub filament_mass_grams: f64,
    pub estimated_cost_usd: f64,
    pub total_layers: usize,
    pub total_moves: usize,
    pub retraction_count: usize,
}

impl PrintEstimation {
    /// Compute analytical estimation from toolpath distances and slicer settings.
    pub fn compute(
        extrusion_distance_mm: f64,
        travel_distance_mm: f64,
        total_layers: usize,
        total_moves: usize,
        retraction_count: usize,
        settings: &SlicerSettings,
    ) -> Self {
        // Volumetric extrusion: cross section = line_width * layer_height
        // Volume deposited = cross_section * distance
        let layer_h = settings.layer_height_mm;
        let line_w = settings.line_width_mm;
        let bead_area_mm2 = line_w * layer_h;
        let deposited_vol_mm3 = bead_area_mm2 * extrusion_distance_mm;

        // Filament cross section = pi * (d/2)^2
        let fil_radius = settings.filament_diameter_mm * 0.5;
        let fil_area_mm2 = std::f64::consts::PI * fil_radius * fil_radius;

        let filament_length_mm = if fil_area_mm2 > 1e-6 {
            deposited_vol_mm3 / fil_area_mm2
        } else {
            0.0
        };

        let filament_volume_cm3 = deposited_vol_mm3 * 1e-3;
        let filament_mass_grams = filament_volume_cm3 * settings.material.density_g_cm3;
        let estimated_cost_usd = (filament_mass_grams * 1e-3) * settings.material.cost_per_kg_usd;

        // Kinematic time model: trapezoidal motion approximation
        // T = d / v_eff, where v_eff ~ 0.7 * v_nominal due to acceleration limits
        let print_speed = settings.print_speed_mm_s.max(5.0);
        let travel_speed = settings.travel_speed_mm_s.max(10.0);

        let print_time_s = extrusion_distance_mm / (print_speed * 0.75);
        let travel_time_s = travel_distance_mm / (travel_speed * 0.75);
        let retract_time_s = (retraction_count as f64)
            * (settings.retract_distance_mm * 2.0 / settings.retract_speed_mm_s.max(10.0));
        let layer_change_time_s = (total_layers as f64) * 0.5; // 0.5s per Z-hop/layer change

        let total_time_s = print_time_s + travel_time_s + retract_time_s + layer_change_time_s;

        let hours = (total_time_s / 3600.0).floor() as u64;
        let rem_s = total_time_s - (hours as f64) * 3600.0;
        let minutes = (rem_s / 60.0).floor() as u64;
        let seconds = (rem_s - (minutes as f64) * 60.0).round() as u64;

        let formatted_time = if hours > 0 {
            format!("{}h {}m {}s", hours, minutes, seconds)
        } else {
            format!("{}m {}s", minutes, seconds)
        };

        Self {
            total_print_time_seconds: total_time_s,
            formatted_time,
            filament_length_mm,
            filament_volume_cm3,
            filament_mass_grams,
            estimated_cost_usd,
            total_layers,
            total_moves,
            retraction_count,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_print_estimation_calculation() {
        let settings = SlicerSettings::default();
        let est = PrintEstimation::compute(10_000.0, 5_000.0, 50, 200, 10, &settings);
        assert!(est.total_print_time_seconds > 0.0);
        assert!(est.filament_mass_grams > 0.0);
        assert!(est.estimated_cost_usd > 0.0);
        assert!(!est.formatted_time.is_empty());
    }
}
