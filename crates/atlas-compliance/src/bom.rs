//! Bill of Materials (BOM) summary and procurement cost rolls.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BomLineItem {
    pub part_number: String,
    pub description: String,
    pub material: String,
    pub quantity: u32,
    pub unit_mass_kg: f64,
    pub extended_mass_kg: f64,
    pub unit_cost_usd: f64,
    pub extended_cost_usd: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BomSummary {
    pub items: Vec<BomLineItem>,
    pub total_parts_count: u32,
    pub total_mass_kg: f64,
    pub total_estimated_cost_usd: f64,
}

impl BomSummary {
    pub fn standard_single_part(mass_kg: f64, material: &str) -> Self {
        let line = BomLineItem {
            part_number: "ATL-PRT-001".to_string(),
            description: "Machined Structural Bracket".to_string(),
            material: material.to_string(),
            quantity: 1,
            unit_mass_kg: mass_kg,
            extended_mass_kg: mass_kg,
            unit_cost_usd: 48.50,
            extended_cost_usd: 48.50,
        };

        Self {
            items: vec![line],
            total_parts_count: 1,
            total_mass_kg: mass_kg,
            total_estimated_cost_usd: 48.50,
        }
    }
}
