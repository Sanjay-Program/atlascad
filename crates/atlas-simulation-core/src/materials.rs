//! Physical and simulation material properties.

use serde::{Deserialize, Serialize};

/// Isotropic physical material properties for structural, thermal, and fluid solvers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SimulationMaterial {
    pub name: String,
    pub elastic_modulus_gpa: f64,
    pub poissons_ratio: f64,
    pub density_kg_m3: f64,
    pub yield_strength_mpa: f64,
    pub thermal_conductivity_w_mk: f64,
    pub dynamic_viscosity_pa_s: Option<f64>,
}

impl SimulationMaterial {
    /// Standard Structural Steel (ASTM A36).
    pub fn structural_steel() -> Self {
        Self {
            name: "Structural Steel".to_string(),
            elastic_modulus_gpa: 200.0,
            poissons_ratio: 0.30,
            density_kg_m3: 7850.0,
            yield_strength_mpa: 250.0,
            thermal_conductivity_w_mk: 50.0,
            dynamic_viscosity_pa_s: None,
        }
    }

    /// Standard Aluminum 6061-T6.
    pub fn aluminum_6061_t6() -> Self {
        Self {
            name: "Aluminum 6061-T6".to_string(),
            elastic_modulus_gpa: 68.9,
            poissons_ratio: 0.33,
            density_kg_m3: 2700.0,
            yield_strength_mpa: 276.0,
            thermal_conductivity_w_mk: 167.0,
            dynamic_viscosity_pa_s: None,
        }
    }

    /// Standard Titanium Ti-6Al-4V Grade 5.
    pub fn titanium_ti6al4v() -> Self {
        Self {
            name: "Titanium Ti-6Al-4V".to_string(),
            elastic_modulus_gpa: 113.8,
            poissons_ratio: 0.342,
            density_kg_m3: 4430.0,
            yield_strength_mpa: 880.0,
            thermal_conductivity_w_mk: 6.7,
            dynamic_viscosity_pa_s: None,
        }
    }

    /// Liquid Water at 20°C.
    pub fn water_20c() -> Self {
        Self {
            name: "Water (20°C)".to_string(),
            elastic_modulus_gpa: 2.2, // Bulk modulus
            poissons_ratio: 0.50,
            density_kg_m3: 998.2,
            yield_strength_mpa: 0.0,
            thermal_conductivity_w_mk: 0.598,
            dynamic_viscosity_pa_s: Some(1.002e-3),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_material_presets() {
        let steel = SimulationMaterial::structural_steel();
        assert_eq!(steel.elastic_modulus_gpa, 200.0);
        assert_eq!(steel.density_kg_m3, 7850.0);

        let alu = SimulationMaterial::aluminum_6061_t6();
        assert_eq!(alu.yield_strength_mpa, 276.0);
    }
}
