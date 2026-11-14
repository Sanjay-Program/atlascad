//! Project-level configuration, unit standards, and tolerances.

use serde::{Deserialize, Serialize};

/// Standard unit systems for engineering projects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnitSystem {
    /// Millimeters, Kilograms, Seconds, Celsius (MMKS).
    MetricMm,
    /// Meters, Kilograms, Seconds, Kelvin (MKS).
    MetricM,
    /// Inches, Pounds, Seconds, Fahrenheit (IPS).
    ImperialIn,
    /// Feet, Pounds, Seconds, Fahrenheit (FPS).
    ImperialFt,
}

/// Project modeling settings and geometric tolerances.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectSettings {
    pub units: UnitSystem,
    pub linear_tolerance_mm: f64,
    pub angular_tolerance_deg: f64,
    pub active_workspace: String,
    pub autosave_enabled: bool,
}

impl Default for ProjectSettings {
    fn default() -> Self {
        Self {
            units: UnitSystem::MetricMm,
            linear_tolerance_mm: 1e-4,
            angular_tolerance_deg: 1e-3,
            active_workspace: "mechanical".to_string(),
            autosave_enabled: true,
        }
    }
}
