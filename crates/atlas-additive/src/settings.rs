//! Slicing configuration and 3D printing parameters.

use serde::{Deserialize, Serialize};

/// Target 3D printer firmware and G-code flavor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PrinterDialect {
    Marlin,
    Klipper,
    RepRap,
    Prusa,
}

impl PrinterDialect {
    pub const fn default_flavor() -> Self {
        Self::Marlin
    }
}

/// Infill pattern algorithm for interior material deposition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InfillPattern {
    Rectilinear,
    Grid,
    Honeycomb,
    Gyroid,
    Concentric,
}

/// Standard 3D printing filament material with thermodynamic and density parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FilamentMaterial {
    pub name: String,
    pub density_g_cm3: f64,
    pub default_nozzle_temp_c: f64,
    pub default_bed_temp_c: f64,
    pub cost_per_kg_usd: f64,
}

impl FilamentMaterial {
    pub fn pla() -> Self {
        Self {
            name: "PLA (Polylactic Acid)".to_string(),
            density_g_cm3: 1.24,
            default_nozzle_temp_c: 210.0,
            default_bed_temp_c: 60.0,
            cost_per_kg_usd: 24.0,
        }
    }

    pub fn petg() -> Self {
        Self {
            name: "PETG (Polyethylene Terephthalate Glycol)".to_string(),
            density_g_cm3: 1.27,
            default_nozzle_temp_c: 240.0,
            default_bed_temp_c: 80.0,
            cost_per_kg_usd: 28.0,
        }
    }

    pub fn abs() -> Self {
        Self {
            name: "ABS (Acrylonitrile Butadiene Styrene)".to_string(),
            density_g_cm3: 1.04,
            default_nozzle_temp_c: 250.0,
            default_bed_temp_c: 100.0,
            cost_per_kg_usd: 26.0,
        }
    }
}

/// Slicer operational settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SlicerSettings {
    pub layer_height_mm: f64,
    pub initial_layer_height_mm: f64,
    pub nozzle_diameter_mm: f64,
    pub line_width_mm: f64,
    pub filament_diameter_mm: f64,
    pub wall_perimeters: usize,
    pub infill_density: f64, // 0.0 to 1.0
    pub infill_pattern: InfillPattern,
    pub print_speed_mm_s: f64,
    pub travel_speed_mm_s: f64,
    pub initial_layer_speed_mm_s: f64,
    pub nozzle_temp_c: f64,
    pub bed_temp_c: f64,
    pub retract_distance_mm: f64,
    pub retract_speed_mm_s: f64,
    pub dialect: PrinterDialect,
    pub material: FilamentMaterial,
}

impl Default for SlicerSettings {
    fn default() -> Self {
        Self {
            layer_height_mm: 0.20,
            initial_layer_height_mm: 0.28,
            nozzle_diameter_mm: 0.40,
            line_width_mm: 0.44,
            filament_diameter_mm: 1.75,
            wall_perimeters: 2,
            infill_density: 0.20,
            infill_pattern: InfillPattern::Rectilinear,
            print_speed_mm_s: 60.0,
            travel_speed_mm_s: 150.0,
            initial_layer_speed_mm_s: 25.0,
            nozzle_temp_c: 210.0,
            bed_temp_c: 60.0,
            retract_distance_mm: 1.2,
            retract_speed_mm_s: 40.0,
            dialect: PrinterDialect::Marlin,
            material: FilamentMaterial::pla(),
        }
    }
}
