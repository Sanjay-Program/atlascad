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
    /// No declared parts and no priced procurement.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            items: Vec::new(),
            total_parts_count: 0,
            total_mass_kg: 0.0,
            total_estimated_cost_usd: 0.0,
        }
    }
}
