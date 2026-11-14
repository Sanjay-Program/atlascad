//! Analytic mass properties and inertial moments.

use serde::{Deserialize, Serialize};

/// Comprehensive geometric and mass properties for engineering certification.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MassPropertiesSummary {
    pub volume_mm3: f64,
    pub surface_area_mm2: f64,
    pub mass_kg: f64,
    pub material_name: String,
    pub density_kg_m3: f64,
    pub center_of_gravity_mm: [f64; 3],
    pub principal_moments_g_mm2: [f64; 3],
    pub bounding_box_min_mm: [f64; 3],
    pub bounding_box_max_mm: [f64; 3],
}

impl MassPropertiesSummary {
    pub fn standard_bracket() -> Self {
        Self {
            volume_mm3: 500.0,
            surface_area_mm2: 400.0,
            mass_kg: 0.00135,
            material_name: "Aluminium 6061-T6".to_string(),
            density_kg_m3: 2700.0,
            center_of_gravity_mm: [5.0, 5.0, 2.5],
            principal_moments_g_mm2: [14.06, 14.06, 22.50],
            bounding_box_min_mm: [0.0, 0.0, 0.0],
            bounding_box_max_mm: [10.0, 10.0, 5.0],
        }
    }
}
