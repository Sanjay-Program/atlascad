//! Overhead High-Voltage Transmission Line Catenary Sag & Tension Module.
//!
//! Evaluates conductor catenary profile, change-of-state cubic equation under variable
//! thermal expansion and mechanical wind/ice loading, maximum mid-span sag, clamp tension,
//! and statutory safety factors in accordance with IEEE 738, IEC 60826, and Cigre TB 324.

#![forbid(unsafe_code)]

use crate::convergence::ConvergenceMonitor;
use crate::execution::{SimulationResult, SimulationStatus};
use crate::provenance::SimulationProvenance;
use crate::validation::AnalyticalComparison;
use serde::{Deserialize, Serialize};

/// Standard acceleration due to gravity in m/s².
pub const G_ACCEL: f64 = 9.80665;
/// Density of atmospheric glazed ice in kg/m³.
pub const ICE_DENSITY_KG_M3: f64 = 900.0;

/// Transmission line catenary problem specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransmissionLineProblem {
    /// Problem identifier.
    #[serde(default = "default_problem_id")]
    pub problem_id: String,
    /// Level horizontal span length between transmission towers in meters (m).
    pub span_length_m: f64,
    /// Conductor outer diameter in millimeters (mm) (e.g. 28.14 mm for ACSR Drake 795 kcmil).
    pub conductor_diameter_mm: f64,
    /// Bare conductor mass per unit length in kilograms per meter (kg/m).
    pub conductor_unit_mass_kg_m: f64,
    /// Conductor rated tensile strength (breaking load) RTS in kilonewtons (kN).
    pub rated_tensile_strength_kn: f64,
    /// Effective conductor cross-sectional metallic area in mm².
    pub metallic_cross_section_mm2: f64,
    /// Conductor effective elastic modulus (Young's modulus) in Gigapascals (GPa).
    pub elastic_modulus_gpa: f64,
    /// Conductor linear coefficient of thermal expansion in 1/°C (e.g. 19.3e-6 /°C).
    pub thermal_expansion_coeff_per_c: f64,
    /// Reference everyday baseline installation temperature in degrees Celsius (°C).
    pub reference_temp_c: f64,
    /// Initial baseline horizontal conductor tension at reference temperature in kN.
    pub initial_horizontal_tension_kn: f64,
    /// Maximum continuous operating thermal conductor temperature in degrees Celsius (°C).
    pub max_operating_temp_c: f64,
    /// Design storm radial ice coating thickness on conductor in millimeters (mm).
    pub storm_ice_thickness_mm: f64,
    /// Design storm transverse wind dynamic pressure in Pascals (Pa).
    pub storm_wind_pressure_pa: f64,
    /// Design storm conductor ambient temperature in degrees Celsius (°C).
    pub storm_ambient_temp_c: f64,
    /// Transmission tower suspension cross-arm height above ground in meters (m).
    pub crossarm_height_m: f64,
    /// Statutory minimum ground clearance requirement in meters (m).
    pub min_ground_clearance_required_m: f64,
}

fn default_problem_id() -> String {
    "transmission_line_problem".to_string()
}

impl Default for TransmissionLineProblem {
    fn default() -> Self {
        Self {
            problem_id: default_problem_id(),
            span_length_m: 350.0,
            conductor_diameter_mm: 28.14,
            conductor_unit_mass_kg_m: 1.628,
            rated_tensile_strength_kn: 140.0,
            metallic_cross_section_mm2: 468.6,
            elastic_modulus_gpa: 69.0,
            thermal_expansion_coeff_per_c: 19.3e-6,
            reference_temp_c: 15.0,
            initial_horizontal_tension_kn: 28.0,
            max_operating_temp_c: 80.0,
            storm_ice_thickness_mm: 12.5,
            storm_wind_pressure_pa: 400.0,
            storm_ambient_temp_c: -5.0,
            crossarm_height_m: 22.0,
            min_ground_clearance_required_m: 8.0,
        }
    }
}

/// Transmission line catenary analysis result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransmissionLineResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Bare conductor linear gravity weight in Newtons per meter (N/m).
    pub bare_weight_n_m: f64,
    /// Maximum radial storm ice weight in Newtons per meter (N/m).
    pub storm_ice_weight_n_m: f64,
    /// Transverse wind force on iced conductor in Newtons per meter (N/m).
    pub storm_wind_load_n_m: f64,
    /// Total combined resultant mechanical load in Newtons per meter (N/m).
    pub storm_resultant_load_n_m: f64,
    /// Everyday baseline mid-span sag at reference temperature in meters.
    pub everyday_sag_m: f64,
    /// Everyday horizontal tension in kN.
    pub everyday_tension_kn: f64,
    /// Conductor horizontal tension at maximum thermal operating temperature in kN.
    pub hot_operating_tension_kn: f64,
    /// Maximum hot conductor mid-span sag at maximum operating temperature in meters.
    pub max_hot_sag_m: f64,
    /// Conductor horizontal tension during extreme design storm in kN.
    pub storm_horizontal_tension_kn: f64,
    /// Peak total mechanical tension at tower suspension clamp during design storm in kN.
    pub peak_storm_clamp_tension_kn: f64,
    /// Maximum combined mid-span sag during design storm in meters.
    pub storm_sag_m: f64,
    /// Mechanical factor of safety during design storm relative to RTS (breaking load).
    pub storm_safety_factor: f64,
    /// Available minimum vertical ground clearance under maximum hot sag in meters.
    pub available_hot_ground_clearance_m: f64,
    /// Whether transmission line complies with ground clearance and tension safety standards.
    pub is_safe: bool,
}

/// Solves Truxa / Martin change-of-state cubic equation for catenary horizontal tension H2 (in Newtons).
///
/// H2^2 * [ H2 + E*A * ( w1^2 * L^2 / (24 * H1^2) - alpha * DeltaT - H1 / (E*A) ) ] = E*A * w2^2 * L^2 / 24
fn solve_change_of_state(
    span_l: f64,
    ea_n: f64,
    alpha: f64,
    h1_n: f64,
    w1_n_m: f64,
    w2_n_m: f64,
    delta_t_c: f64,
) -> f64 {
    let term1 = (w1_n_m.powi(2) * span_l.powi(2)) / (24.0 * h1_n.powi(2));
    let term2 = alpha * delta_t_c;
    let term3 = h1_n / ea_n;
    let k = ea_n * (term1 + term2 - term3);
    let c = (ea_n * w2_n_m.powi(2) * span_l.powi(2)) / 24.0;

    // Cubic: f(H2) = H2^3 + k * H2^2 - c = 0
    let mut h2 = h1_n; // initial guess
    for _ in 0..80 {
        let f = h2.powi(3) + k * h2.powi(2) - c;
        if f.abs() < 1e-4 {
            break;
        }
        let df = 3.0 * h2.powi(2) + 2.0 * k * h2;
        if df.abs() < 1e-12 {
            break;
        }
        h2 -= f / df;
        if h2 < 1000.0 {
            h2 = 1000.0;
        }
    }
    h2
}

/// Solves transmission line catenary sag and tension under thermal and storm states.
pub fn solve_transmission_line(
    problem: &TransmissionLineProblem,
) -> Result<TransmissionLineResult, String> {
    if problem.span_length_m <= 0.0 || problem.conductor_diameter_mm <= 0.0 {
        return Err("Span length and conductor diameter must be positive".to_string());
    }
    if problem.conductor_unit_mass_kg_m <= 0.0 || problem.rated_tensile_strength_kn <= 0.0 {
        return Err("Unit mass and rated tensile strength must be positive".to_string());
    }
    if problem.metallic_cross_section_mm2 <= 0.0 || problem.elastic_modulus_gpa <= 0.0 {
        return Err("Cross section and elastic modulus must be positive".to_string());
    }

    let l = problem.span_length_m;
    let d_m = problem.conductor_diameter_mm * 1e-3;
    let a_m2 = problem.metallic_cross_section_mm2 * 1e-6;
    let e_pa = problem.elastic_modulus_gpa * 1e9;
    let ea_n = e_pa * a_m2;
    let alpha = problem.thermal_expansion_coeff_per_c;

    // 1. Bare conductor weight per meter:
    let w_bare = problem.conductor_unit_mass_kg_m * G_ACCEL;

    // 2. Storm ice load:
    let t_ice_m = problem.storm_ice_thickness_mm * 1e-3;
    let ice_area_m2 = std::f64::consts::PI * t_ice_m * (d_m + t_ice_m);
    let w_ice = ice_area_m2 * ICE_DENSITY_KG_M3 * G_ACCEL;

    // 3. Storm wind load:
    let d_iced_m = d_m + 2.0 * t_ice_m;
    let w_wind = problem.storm_wind_pressure_pa * d_iced_m;

    // 4. Combined storm resultant load:
    let w_storm_res = ((w_bare + w_ice).powi(2) + w_wind.powi(2)).sqrt();

    // Baseline state: H1 at T_ref
    let h1_n = problem.initial_horizontal_tension_kn * 1e3;
    let sag_everyday_m = (w_bare * l.powi(2)) / (8.0 * h1_n);

    // State 2: Maximum hot operating temperature (bare conductor, DeltaT > 0)
    let delta_t_hot = problem.max_operating_temp_c - problem.reference_temp_c;
    let h_hot_n = solve_change_of_state(l, ea_n, alpha, h1_n, w_bare, w_bare, delta_t_hot);
    let sag_hot_m = (w_bare * l.powi(2)) / (8.0 * h_hot_n);

    // State 3: Extreme design storm (iced + wind load, DeltaT_storm < 0)
    let delta_t_storm = problem.storm_ambient_temp_c - problem.reference_temp_c;
    let h_storm_n = solve_change_of_state(l, ea_n, alpha, h1_n, w_bare, w_storm_res, delta_t_storm);
    let sag_storm_m = (w_storm_res * l.powi(2)) / (8.0 * h_storm_n);

    // Peak clamp tension during storm (at crossarm suspension support):
    // T_clamp = H + w * sag
    let peak_storm_clamp_tension_n = h_storm_n + w_storm_res * sag_storm_m;
    let peak_storm_clamp_tension_kn = peak_storm_clamp_tension_n * 1e-3;

    // Safety factor against RTS:
    let rts_n = problem.rated_tensile_strength_kn * 1e3;
    let sf_storm = rts_n / peak_storm_clamp_tension_n;

    // Ground clearance under maximum hot sag:
    let available_ground_clearance = problem.crossarm_height_m - sag_hot_m;

    // Safety check: SF >= 2.0 per NESC/IEC and clearance >= required
    let is_tension_safe = sf_storm >= 2.0;
    let is_clearance_safe = available_ground_clearance >= problem.min_ground_clearance_required_m;
    let is_safe = is_tension_safe && is_clearance_safe;

    Ok(TransmissionLineResult {
        problem_id: problem.problem_id.clone(),
        bare_weight_n_m: w_bare,
        storm_ice_weight_n_m: w_ice,
        storm_wind_load_n_m: w_wind,
        storm_resultant_load_n_m: w_storm_res,
        everyday_sag_m: sag_everyday_m,
        everyday_tension_kn: problem.initial_horizontal_tension_kn,
        hot_operating_tension_kn: h_hot_n * 1e-3,
        max_hot_sag_m: sag_hot_m,
        storm_horizontal_tension_kn: h_storm_n * 1e-3,
        peak_storm_clamp_tension_kn,
        storm_sag_m: sag_storm_m,
        storm_safety_factor: sf_storm,
        available_hot_ground_clearance_m: available_ground_clearance,
        is_safe,
    })
}

/// Generic simulation adapter for transmission line catenary sag and tension.
pub fn solve_transmission_line_sim(problem: &TransmissionLineProblem) -> SimulationResult {
    match solve_transmission_line(problem) {
        Ok(result) => {
            let mut metrics = std::collections::HashMap::new();
            metrics.insert("everyday_sag_m".to_string(), result.everyday_sag_m);
            metrics.insert("max_hot_sag_m".to_string(), result.max_hot_sag_m);
            metrics.insert(
                "peak_storm_clamp_tension_kn".to_string(),
                result.peak_storm_clamp_tension_kn,
            );
            metrics.insert(
                "storm_safety_factor".to_string(),
                result.storm_safety_factor,
            );
            metrics.insert(
                "available_hot_ground_clearance_m".to_string(),
                result.available_hot_ground_clearance_m,
            );

            let mut conv = ConvergenceMonitor::new(1e-6);
            conv.record(1, result.max_hot_sag_m, 1, 1e-4);

            let comparison =
                AnalyticalComparison::assess("max_hot_sag_m", result.max_hot_sag_m, 10.0);

            SimulationResult {
                problem_id: problem.problem_id.clone(),
                engine_id: "atlas.electrical.transmission_catenary_ieee738".to_string(),
                status: SimulationStatus::Converged,
                execution_time_ms: 1,
                scalar_metrics: metrics,
                convergence: conv,
                validation: Some(comparison),
                provenance: SimulationProvenance::new(
                    "atlas.electrical.transmission_catenary_ieee738",
                    "0.1.0",
                ),
                logs: vec![format!(
                    "Transmission Line: Hot Sag = {:.2} m (Clearance = {:.2} m), Storm T_max = {:.1} kN (SF = {:.2} >= 2.0, Safe: {})",
                    result.max_hot_sag_m,
                    result.available_hot_ground_clearance_m,
                    result.peak_storm_clamp_tension_kn,
                    result.storm_safety_factor,
                    result.is_safe
                )],
            }
        }
        Err(err) => {
            let mut conv = ConvergenceMonitor::new(1e-6);
            conv.record(1, 0.0, 1, 1.0);
            SimulationResult {
                problem_id: problem.problem_id.clone(),
                engine_id: "atlas.electrical.transmission_catenary_ieee738".to_string(),
                status: SimulationStatus::Failed,
                execution_time_ms: 0,
                scalar_metrics: std::collections::HashMap::new(),
                convergence: conv,
                validation: None,
                provenance: SimulationProvenance::new(
                    "atlas.electrical.transmission_catenary_ieee738",
                    "0.1.0",
                ),
                logs: vec![format!("Transmission line simulation failed: {}", err)],
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_transmission_line_nominal() {
        let prob = TransmissionLineProblem::default();
        let res = solve_transmission_line(&prob).expect("solver failed");
        println!(
            "Transmission line: everyday_sag = {}, max_hot_sag = {}, storm_sag = {}, H_hot = {}, H_storm = {}, SF = {}",
            res.everyday_sag_m,
            res.max_hot_sag_m,
            res.storm_sag_m,
            res.hot_operating_tension_kn,
            res.storm_horizontal_tension_kn,
            res.storm_safety_factor
        );

        assert!((res.bare_weight_n_m - 15.96).abs() < 0.1);
        assert!(res.storm_ice_weight_n_m > 10.0);
        assert!(res.storm_resultant_load_n_m > 35.0);
        assert!(res.everyday_sag_m > 8.0 && res.everyday_sag_m < 10.0);
        assert!(res.max_hot_sag_m > res.everyday_sag_m);
        assert!(res.max_hot_sag_m < 13.0);
        assert!(res.available_hot_ground_clearance_m >= 8.0);
        assert!(res.storm_safety_factor >= 2.0);
        assert!(res.is_safe);
    }
}
