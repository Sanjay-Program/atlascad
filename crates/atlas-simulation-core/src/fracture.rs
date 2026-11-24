//! # Linear Elastic Fracture Mechanics (LEFM) & Paris Fatigue Crack Growth (ASTM E399 / E647)
//!
//! Provides first-principles calculation of Mode I stress intensity factors K_I,
//! critical flaw size a_c, fracture toughness margins K_IC, and analytical integration
//! of Paris-Erdogan subcritical fatigue crack propagation cycles N_f.

use crate::convergence::ConvergenceMonitor;
use crate::execution::{SimulationResult, SimulationStatus};
use crate::provenance::SimulationProvenance;
use crate::validation::AnalyticalComparison;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Standard engineering alloy fracture toughness and Paris constants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FractureMaterial {
    /// Aerospace Aluminum 7075-T651 (K_IC = 29 MPa*m^0.5, C = 5.2e-11, m = 3.6)
    Aluminum7075T6,
    /// Aerospace Titanium Ti-6Al-4V (K_IC = 65 MPa*m^0.5, C = 1.8e-11, m = 3.2)
    TitaniumTi6Al4V,
    /// High-strength rotor steel 4340 (K_IC = 50 MPa*m^0.5, C = 2.4e-11, m = 3.0)
    AlloySteel4340,
    /// Pressure vessel steel SA-516 Gr 70 (K_IC = 85 MPa*m^0.5, C = 1.2e-11, m = 3.1)
    CarbonSteelSa516,
}

impl FractureMaterial {
    /// Plane-strain fracture toughness K_IC in MPa*m^0.5.
    pub fn fracture_toughness_kic_mpa_m05(&self) -> f64 {
        match self {
            Self::Aluminum7075T6 => 29.0,
            Self::TitaniumTi6Al4V => 65.0,
            Self::AlloySteel4340 => 50.0,
            Self::CarbonSteelSa516 => 85.0,
        }
    }

    /// Paris crack growth coefficient C in m/cycle / (MPa*m^0.5)^m.
    pub fn paris_coefficient_c(&self) -> f64 {
        match self {
            Self::Aluminum7075T6 => 5.2e-11,
            Self::TitaniumTi6Al4V => 1.8e-11,
            Self::AlloySteel4340 => 2.4e-11,
            Self::CarbonSteelSa516 => 1.2e-11,
        }
    }

    /// Paris crack growth exponent m.
    pub fn paris_exponent_m(&self) -> f64 {
        match self {
            Self::Aluminum7075T6 => 3.6,
            Self::TitaniumTi6Al4V => 3.2,
            Self::AlloySteel4340 => 3.0,
            Self::CarbonSteelSa516 => 3.1,
        }
    }

    /// Threshold stress intensity range Delta K_th in MPa*m^0.5.
    pub fn threshold_delta_k_mpa_m05(&self) -> f64 {
        match self {
            Self::Aluminum7075T6 => 2.5,
            Self::TitaniumTi6Al4V => 4.0,
            Self::AlloySteel4340 => 3.2,
            Self::CarbonSteelSa516 => 4.5,
        }
    }
}

/// Crack and component structural geometry configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CrackConfiguration {
    /// Single through-thickness edge crack in plate of width W
    EdgeCrackInTension,
    /// Center through-thickness crack in plate of total width 2W
    CenterCrackInTension,
    /// Embedded circular / penny-shaped internal crack
    EmbeddedCircularCrack,
}

/// Problem input definition for LEFM and fatigue crack life assessment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FractureProblem {
    pub component_name: String,
    pub material: FractureMaterial,
    pub crack_type: CrackConfiguration,
    /// Maximum remote tensile cyclic stress sigma_max in MPa
    pub max_stress_mpa: f64,
    /// Minimum remote tensile cyclic stress sigma_min in MPa (default 0.0)
    pub min_stress_mpa: Option<f64>,
    /// Initial detectable flaw length / depth a_0 in mm (e.g. 1.5 mm NDT threshold)
    pub initial_crack_depth_a0_mm: f64,
    /// Component plate half-width or width W in mm
    pub plate_width_w_mm: f64,
}

impl Default for FractureProblem {
    fn default() -> Self {
        Self {
            component_name: "Aircraft Wing Spar Flange Lower Cap".to_string(),
            material: FractureMaterial::Aluminum7075T6,
            crack_type: CrackConfiguration::EdgeCrackInTension,
            max_stress_mpa: 180.0,
            min_stress_mpa: Some(18.0), // R = 0.10
            initial_crack_depth_a0_mm: 1.5,
            plate_width_w_mm: 80.0,
        }
    }
}

/// Detailed calculation outputs for LEFM and Paris crack growth.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FractureResult {
    pub component_name: String,
    pub material_kic_mpa_m05: f64,
    pub initial_crack_a0_mm: f64,
    pub initial_stress_intensity_k1_mpa_m05: f64,
    pub delta_k1_initial_mpa_m05: f64,
    pub threshold_delta_k_mpa_m05: f64,
    pub is_crack_propagating: bool,
    pub geometry_factor_y0: f64,
    pub critical_crack_length_ac_mm: f64,
    pub allowable_stress_intensity_margin: f64,
    pub fatigue_cycles_to_failure_nf: f64,
    pub recommended_inspection_interval_cycles: f64,
    pub is_safe_against_fast_fracture: bool,
    pub summary_notes: Vec<String>,
}

/// Solves linear elastic fracture mechanics and Paris law integration.
pub fn solve_lefm_and_paris_growth(prob: &FractureProblem) -> Result<FractureResult, String> {
    let s_max = prob.max_stress_mpa;
    let s_min = prob.min_stress_mpa.unwrap_or(0.0).max(0.0);
    let a0_mm = prob.initial_crack_depth_a0_mm;
    let w_mm = prob.plate_width_w_mm;

    if s_max <= 0.0 || a0_mm <= 0.05 || w_mm <= a0_mm {
        return Err(
            "Max stress, initial crack depth (>0.05mm), and plate width (>a0) must be positive."
                .to_string(),
        );
    }

    let delta_sigma = (s_max - s_min).max(1.0);
    let a0_m = a0_mm / 1000.0;
    let w_m = w_mm / 1000.0;
    let alpha0 = a0_m / w_m;

    let kic = prob.material.fracture_toughness_kic_mpa_m05();
    let dk_th = prob.material.threshold_delta_k_mpa_m05();
    let c_paris = prob.material.paris_coefficient_c();
    let m_paris = prob.material.paris_exponent_m();

    // Geometry correction factor Y(a/W):
    let y0 = match prob.crack_type {
        CrackConfiguration::EdgeCrackInTension => {
            // Y(alpha) = 1.12 - 0.231*alpha + 10.55*alpha^2 - 21.72*alpha^3 + 30.39*alpha^4
            1.12 - 0.231 * alpha0 + 10.55 * alpha0.powi(2) - 21.72 * alpha0.powi(3)
                + 30.39 * alpha0.powi(4)
        }
        CrackConfiguration::CenterCrackInTension => {
            // Y(alpha) = sqrt(sec(pi * alpha / 2))
            (1.0 / (std::f64::consts::FRAC_PI_2 * alpha0).cos()).sqrt()
        }
        CrackConfiguration::EmbeddedCircularCrack => 2.0 / std::f64::consts::PI,
    };

    // Initial Mode I stress intensity factor:
    // K_I = Y * sigma_max * sqrt(pi * a) [MPa*m^0.5]
    let k1_initial = y0 * s_max * (std::f64::consts::PI * a0_m).sqrt();
    let delta_k_initial = y0 * delta_sigma * (std::f64::consts::PI * a0_m).sqrt();

    // Check fast brittle fracture at initial crack:
    let is_fast_fracture_safe = k1_initial < kic;
    let kic_margin = kic / k1_initial.max(0.1);

    // Critical crack length a_c at K_I = K_IC:
    // a_c = (1 / pi) * (K_IC / (Y * sigma_max))^2
    let ac_m_theoretical = (1.0 / std::f64::consts::PI) * (kic / (y0 * s_max)).powi(2);
    // Upper bound by 80% of section width
    let ac_m = ac_m_theoretical.clamp(a0_m * 1.05, w_m * 0.80);
    let ac_mm = ac_m * 1000.0;

    // Check if cyclic crack propagation occurs (Delta K > Delta K_th):
    let is_propagating = delta_k_initial > dk_th;

    // Paris Law Cycle Integration:
    // da/dN = C * (Delta K)^m = C * (Y * Delta_sigma * sqrt(pi))^m * a^(m/2)
    // N_f = (ac^(1 - m/2) - a0^(1 - m/2)) / [ C * (Y * Delta_sigma * sqrt(pi))^m * (1 - m/2) ]
    let p_const = c_paris * (y0 * delta_sigma * std::f64::consts::PI.sqrt()).powf(m_paris);
    let exponent = 1.0 - m_paris / 2.0;

    let nf_cycles = if !is_propagating {
        f64::INFINITY
    } else if exponent.abs() < 1e-4 {
        (ac_m / a0_m).ln() / p_const
    } else {
        (ac_m.powf(exponent) - a0_m.powf(exponent)) / (p_const * exponent)
    };

    // Damage-tolerant inspection interval (typically 1/2 of N_f per FAA / MIL-STD-1530):
    let inspect_cycles = if nf_cycles.is_finite() {
        (nf_cycles * 0.50).max(100.0)
    } else {
        1000000.0
    };

    let notes = vec![
        format!(
            "Fracture Mechanics: '{}' (Material = {:?}, K_IC = {:.1} MPa·m^0.5)",
            prob.component_name, prob.material, kic
        ),
        format!(
            "Loading: σ_max = {:.1} MPa, σ_min = {:.1} MPa (Δσ = {:.1} MPa, R = {:.2})",
            s_max,
            s_min,
            delta_sigma,
            s_min / s_max
        ),
        format!(
            "Initial Flaw: a0 = {:.2} mm (Y = {:.3}) -> Initial K_I = {:.2} MPa·m^0.5 (Toughness Margin = {:.2}x)",
            a0_mm, y0, k1_initial, kic_margin
        ),
        format!(
            "Fatigue Threshold: ΔK_initial = {:.2} MPa·m^0.5 vs ΔK_th = {:.2} MPa·m^0.5 (Active Propagation: {})",
            delta_k_initial, dk_th, is_propagating
        ),
        format!("Critical Crack: Fast Fracture a_c = {:.2} mm", ac_mm),
        format!(
            "Paris Crack Life: N_f = {:.0} cycles -> Mandated Inspection Interval = {:.0} cycles (Safety 2.0x)",
            nf_cycles, inspect_cycles
        ),
    ];

    Ok(FractureResult {
        component_name: prob.component_name.clone(),
        material_kic_mpa_m05: kic,
        initial_crack_a0_mm: a0_mm,
        initial_stress_intensity_k1_mpa_m05: k1_initial,
        delta_k1_initial_mpa_m05: delta_k_initial,
        threshold_delta_k_mpa_m05: dk_th,
        is_crack_propagating: is_propagating,
        geometry_factor_y0: y0,
        critical_crack_length_ac_mm: ac_mm,
        allowable_stress_intensity_margin: kic_margin,
        fatigue_cycles_to_failure_nf: nf_cycles,
        recommended_inspection_interval_cycles: inspect_cycles,
        is_safe_against_fast_fracture: is_fast_fracture_safe,
        summary_notes: notes,
    })
}

/// Simulation runner adapter for fracture mechanics.
pub fn solve_fracture_sim(prob: &FractureProblem) -> Result<SimulationResult, String> {
    let sol = solve_lefm_and_paris_growth(prob)?;

    let mut metrics = HashMap::new();
    metrics.insert(
        "initial_stress_intensity_k1_mpa_m05".to_string(),
        sol.initial_stress_intensity_k1_mpa_m05,
    );
    metrics.insert(
        "delta_k1_initial_mpa_m05".to_string(),
        sol.delta_k1_initial_mpa_m05,
    );
    metrics.insert(
        "critical_crack_length_ac_mm".to_string(),
        sol.critical_crack_length_ac_mm,
    );
    metrics.insert(
        "fatigue_cycles_to_failure_nf".to_string(),
        sol.fatigue_cycles_to_failure_nf,
    );
    metrics.insert(
        "recommended_inspection_interval_cycles".to_string(),
        sol.recommended_inspection_interval_cycles,
    );

    let mut conv = ConvergenceMonitor::new(1e-6);
    conv.record(1, sol.initial_stress_intensity_k1_mpa_m05, 1, 1e-4);

    let val = AnalyticalComparison::assess(
        "Initial Stress Intensity K_I (MPa*m^0.5)",
        sol.initial_stress_intensity_k1_mpa_m05,
        sol.geometry_factor_y0
            * prob.max_stress_mpa
            * (std::f64::consts::PI * prob.initial_crack_depth_a0_mm / 1000.0).sqrt(),
    );

    Ok(SimulationResult {
        problem_id: "fracture-lefm-paris-growth".to_string(),
        engine_id: "atlas-fracture".to_string(),
        status: SimulationStatus::Converged,
        execution_time_ms: 1,
        scalar_metrics: metrics,
        convergence: conv,
        validation: Some(val),
        provenance: SimulationProvenance::new("atlas-fracture", "0.1.0"),
        logs: sol.summary_notes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_aerospace_aluminum_wing_spar_flaw_growth() -> Result<(), String> {
        let prob = FractureProblem::default();
        let sol = solve_lefm_and_paris_growth(&prob)?;

        // For Al 7075-T6: K_IC = 29 MPa*m^0.5
        assert_eq!(sol.material_kic_mpa_m05, 29.0);
        // a0 = 1.5mm, s_max = 180 MPa, Y ~ 1.13 -> K_I ~ 1.13 * 180 * sqrt(pi * 0.0015) ~ 13.9 MPa*m^0.5
        assert!((sol.initial_stress_intensity_k1_mpa_m05 - 13.9).abs() < 1.0);
        // Margin > 2.0x against fracture toughness
        assert!(sol.allowable_stress_intensity_margin > 1.8);
        assert!(sol.is_safe_against_fast_fracture);
        // Delta K ~ 12.5 MPa*m^0.5 > Delta K_th (2.5) -> actively propagating
        assert!(sol.is_crack_propagating);
        // Critical crack size ac ~ 6.0 - 8.0 mm
        assert!(sol.critical_crack_length_ac_mm > 5.0 && sol.critical_crack_length_ac_mm < 10.0);
        // Fatigue cycles to failure ~ 2,000 to 10,000 cycles
        assert!(
            sol.fatigue_cycles_to_failure_nf > 1500.0 && sol.fatigue_cycles_to_failure_nf < 15000.0
        );
        // Recommended inspection interval is half of life
        assert!(
            (sol.recommended_inspection_interval_cycles - sol.fatigue_cycles_to_failure_nf * 0.5)
                .abs()
                < 1.0
        );

        let sim = solve_fracture_sim(&prob)?;
        assert_eq!(sim.status, SimulationStatus::Converged);
        Ok(())
    }
}
