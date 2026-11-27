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
    /// False when mass, centre of gravity and principal moments were not computed. Those fields
    /// are then zeros and must not be read as measurements.
    pub assessed: bool,
}

impl MassPropertiesSummary {
    /// Nothing measured: no mesh, no material density.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            volume_mm3: 0.0,
            surface_area_mm2: 0.0,
            mass_kg: 0.0,
            material_name: "Unassigned".to_string(),
            density_kg_m3: 0.0,
            center_of_gravity_mm: [0.0; 3],
            principal_moments_g_mm2: [0.0; 3],
            bounding_box_min_mm: [0.0; 3],
            bounding_box_max_mm: [0.0; 3],
            assessed: false,
        }
    }
}
