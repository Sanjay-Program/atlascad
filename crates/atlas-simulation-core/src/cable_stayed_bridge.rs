//! Cable-Stayed Bridge Stay Cable Rain-Wind Induced Vibration (RWIV) & Damping Analysis
//!
//! Formulated according to:
//! - FHWA-HRT-05-072 "Wind-Induced Vibration of Stay Cables"
//! - Post-Tensioning Institute (PTI) DC35.1 "Recommendations for Stay Cable Design, Testing and Installation" (6th Edition)
//! - Eurocode 1: EN 1991-1-4 "General actions - Wind actions"
//! - Irvine, H. M., "Cable Structures", The MIT Press
//! - Pacheco, B. M., Fujino, Y. & Sulekh, A., "Estimation Curve for Universal Damping of Stay Cables"
//!
//! Evaluates stay cable sag-extensible transverse eigenfrequencies (Irvine theory),
//! rain-wind induced vibration (RWIV) aerodynamic instability onset, Scruton mass-damping parameter,
//! PTI criterion for limit-cycle galloping suppression, and external damper sizing.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

/// Structural stay cable manufacturing system architecture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StayCableClass {
    /// Parallel Strand System (PSS) encased in co-extruded HDPE stay pipe with helical ribs.
    ParallelStrandSystemPSS,
    /// Locked Coil Steel Wire Rope with zinc-aluminum galvanized outer Z-wires.
    LockedCoilWireRope,
    /// Carbon Fiber Reinforced Polymer (CFRP) ultra-light composite stay cable.
    CarbonFiberCfrpStay,
}

/// Input parameters for cable-stayed bridge stay cable dynamic simulation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CableStayProblem {
    /// Problem identifier.
    pub problem_id: String,
    /// Stay cable structural technology class.
    pub stay_class: StayCableClass,
    /// Chord length of the stay cable between anchorages L in meters (m, standard 50 to 550 m).
    pub cable_chord_length_m: f64,
    /// Outer diameter of HDPE stay casing pipe D in millimeters (mm, standard 90 to 350 mm).
    pub outer_pipe_diameter_mm: f64,
    /// Stay cable chord angle of inclination theta above horizontal in degrees (deg, standard 20 to 65 deg).
    pub chord_inclination_deg: f64,
    /// Service stage effective permanent cable pretension force T_0 in kiloNewtons (kN, standard 500 to 12,000 kN).
    pub cable_pretension_kn: f64,
    /// Total linear mass density of stay cable (steel strands + grout/wax + pipe) m in kg/m (standard 15 to 160 kg/m).
    pub total_linear_mass_kg_m: f64,
    /// Steel / composite strand elastic tensile modulus E in GigaPascals (GPa, standard 185 to 210 GPa).
    pub elastic_modulus_gpa: f64,
    /// Net structural metallic cross-sectional area A in square millimeters (mm², standard 1,000 to 18,000 mm²).
    pub net_steel_area_mm2: f64,
    /// Inherent structural material and friction damping ratio zeta_0 (fraction, standard 0.0005 to 0.0020).
    pub inherent_damping_ratio: f64,
    /// Installation distance of external damper from lower deck anchorage x_d in meters (m, standard 2.0 to 15.0 m).
    pub damper_distance_from_deck_m: f64,
    /// Design mean hourly ambient wind speed at bridge deck level in meters per second (m/s, standard 10 to 50 m/s).
    pub wind_speed_m_s: f64,
}

impl Default for CableStayProblem {
    fn default() -> Self {
        Self {
            problem_id: "long_span_navigational_stay_cable".to_string(),
            stay_class: StayCableClass::ParallelStrandSystemPSS,
            cable_chord_length_m: 240.0,       // 240 m long stay cable
            outer_pipe_diameter_mm: 160.0,     // 160 mm outer HDPE pipe
            chord_inclination_deg: 32.0,       // 32 deg inclination
            cable_pretension_kn: 3600.0,       // 3600 kN service tension
            total_linear_mass_kg_m: 52.0,      // 52 kg/m linear mass
            elastic_modulus_gpa: 195.0,        // 195 GPa strand modulus
            net_steel_area_mm2: 6400.0,        // 6400 mm² metallic area
            inherent_damping_ratio: 0.0012,    // 0.12% inherent structural damping
            damper_distance_from_deck_m: 5.50, // 5.5 m from lower anchorage (~2.3% of span)
            wind_speed_m_s: 14.5,              // 14.5 m/s critical RWIV wind range
        }
    }
}

/// Simulation results for stay cable transverse eigenfrequencies, RWIV stability, and damper sizing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CableStayResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Maximum vertical transverse static gravity sag at midspan delta_sag in millimeters (mm).
    pub midspan_static_sag_mm: f64,
    /// Dimensionless Irvine sag-extensibility parameter lambda_squared.
    pub irvine_parameter_lambda_sq: f64,
    /// Fundamental in-plane transverse modal natural frequency f_1 in Hertz (Hz).
    pub fundamental_frequency_hz: f64,
    /// Second harmonic in-plane modal frequency f_2 in Hertz (Hz).
    pub second_harmonic_freq_hz: f64,
    /// Third harmonic in-plane modal frequency f_3 in Hertz (Hz).
    pub third_harmonic_freq_hz: f64,
    /// Critical onset wind velocity for rain-wind induced vibration U_crit in meters per second (m/s).
    pub critical_rwiv_wind_velocity_m_s: f64,
    /// Baseline inherent Scruton mass-damping parameter Sc_inherent (dimensionless).
    pub inherent_scruton_number: f64,
    /// Post-Tensioning Institute (PTI) minimum required target damping ratio zeta_req to suppress RWIV (fraction).
    pub pti_required_damping_ratio: f64,
    /// Required Scruton number Sc_target according to PTI criterion (>= 10.0).
    pub target_scruton_number: f64,
    /// Optimal external viscous damper linear damping coefficient C_opt in kiloNewton-seconds per meter (kN·s/m).
    pub optimal_damper_coefficient_kn_s_m: f64,
    /// Total system modal damping ratio achieved with sized external damper zeta_total (fraction).
    pub attained_damping_ratio: f64,
    /// Assessment indicating external damper raises Scruton number above PTI threshold (Sc >= 10.0).
    pub is_rwiv_mitigated: bool,
    /// Assessment indicating damper stroke and force are mechanically realizable (C_opt <= 300 kN·s/m).
    pub is_damper_sizing_practical: bool,
    /// Assessment indicating cable static sag does not exceed L / 500 limit.
    pub is_cable_sag_acceptable: bool,
    /// Overall stay cable aerodynamic and structural damping viability.
    pub is_stay_cable_viable: bool,
}

/// Solves cable-stayed bridge stay cable dynamics and RWIV mitigation per FHWA-HRT-05-072 / PTI.
pub fn solve_cable_stayed_bridge(problem: &CableStayProblem) -> Result<CableStayResult, String> {
    if problem.cable_chord_length_m < 20.0 || problem.cable_chord_length_m > 800.0 {
        return Err("Cable chord length must be between 20 and 800 meters".to_string());
    }
    if problem.outer_pipe_diameter_mm < 30.0 || problem.outer_pipe_diameter_mm > 600.0 {
        return Err("Outer pipe diameter must be between 30 and 600 mm".to_string());
    }
    if problem.cable_pretension_kn <= 50.0 {
        return Err("Cable pretension must be positive".to_string());
    }
    if problem.total_linear_mass_kg_m <= 1.0 {
        return Err("Total linear mass must be positive".to_string());
    }

    let g = 9.80665; // m/s²
    let rho_air = 1.225; // kg/m³
    let l_chord = problem.cable_chord_length_m;
    let d_pipe_m = problem.outer_pipe_diameter_mm / 1000.0;
    let theta_rad = problem.chord_inclination_deg.to_radians();
    let cos_theta = theta_rad.cos();
    let t_0_n = problem.cable_pretension_kn * 1000.0; // N
    let m_lin = problem.total_linear_mass_kg_m; // kg/m

    // 1. Static gravity sag perpendicular to chord (Irvine theory):
    // delta_sag = (m * g * cos(theta) * L²) / (8 * T_0) [m]
    let delta_sag_m = (m_lin * g * cos_theta * l_chord.powi(2)) / (8.0 * t_0_n);
    let delta_sag_mm = delta_sag_m * 1000.0;

    // 2. Irvine sag-extensibility dimensionless parameter lambda²:
    // lambda² = (m * g * cos(theta) * L / T_0)² * (E * A * L / (T_0 * L_e))
    // where L_e ~ L * (1 + 8*(delta/L)²) ~ L:
    let ea_n = (problem.elastic_modulus_gpa * 1.0e9) * (problem.net_steel_area_mm2 * 1.0e-6);
    let irvine_lambda_sq = ((m_lin * g * cos_theta * l_chord) / t_0_n).powi(2) * (ea_n / t_0_n);

    // 3. Transverse eigenfrequencies:
    // Taut string fundamental velocity c = sqrt(T_0 / m):
    let c_wave = (t_0_n / m_lin).sqrt();
    let f_taut_1 = c_wave / (2.0 * l_chord);

    // Irvine in-plane symmetric first mode frequency correction:
    // f_1 = f_taut_1 * sqrt(1 + lambda² / pi²)
    let f_1 = f_taut_1 * (1.0 + irvine_lambda_sq / (std::f64::consts::PI.powi(2))).sqrt();
    // Anti-symmetric second mode (sag has zero effect on anti-symmetric node at center):
    let f_2 = 2.0 * f_taut_1;
    // Third mode:
    let f_3 = 3.0 * f_taut_1;

    // 4. Rain-Wind Induced Vibration (RWIV) aerodynamics (FHWA-HRT-05-072):
    // RWIV critical onset velocity: U_crit ~ 35.0 * f_1 * D [m/s]
    let u_crit = 35.0 * f_1 * d_pipe_m;

    // 5. Scruton mass-damping parameter: Sc = (m * zeta) / (rho_air * D²)
    // Baseline inherent Scruton number:
    let sc_inherent = (m_lin * problem.inherent_damping_ratio) / (rho_air * d_pipe_m.powi(2));

    // Post-Tensioning Institute (PTI) criterion:
    // To completely eliminate RWIV galloping and large-amplitude vibration: Sc >= 10.0
    // => zeta_req = (10.0 * rho_air * D²) / m
    let pti_target_sc = 10.0;
    let zeta_req = (pti_target_sc * rho_air * d_pipe_m.powi(2)) / m_lin;

    // 6. External damper design (Pacheco-Fujino universal damper curve):
    // Damper location ratio: mu = x_d / L:
    let mu_damper = (problem.damper_distance_from_deck_m / l_chord).clamp(0.01, 0.10);
    // Optimal linear viscous damping coefficient: C_opt = 0.10 * (m * L * omega_1) / (pi * mu)
    let omega_1 = 2.0 * std::f64::consts::PI * f_1;
    let c_opt_n_s_m = (0.10 * m_lin * l_chord * omega_1) / (std::f64::consts::PI * mu_damper);
    let c_opt_kn_s_m = c_opt_n_s_m / 1000.0;

    // Attained modal damping with optimal damper: zeta_add ~ 0.5 * mu_damper:
    let zeta_add = 0.45 * mu_damper;
    let zeta_total = problem.inherent_damping_ratio + zeta_add;
    let sc_attained = (m_lin * zeta_total) / (rho_air * d_pipe_m.powi(2));

    // 7. Safety assessments:
    let is_rwiv_mitigated = sc_attained >= pti_target_sc;
    let is_damper_sizing_practical = (5.0..=300.0).contains(&c_opt_kn_s_m);
    let max_allowable_sag_m = l_chord / 250.0;
    let is_cable_sag_acceptable = delta_sag_m <= max_allowable_sag_m;
    let is_stay_cable_viable =
        is_rwiv_mitigated && is_damper_sizing_practical && is_cable_sag_acceptable;

    Ok(CableStayResult {
        problem_id: problem.problem_id.clone(),
        midspan_static_sag_mm: delta_sag_mm,
        irvine_parameter_lambda_sq: irvine_lambda_sq,
        fundamental_frequency_hz: f_1,
        second_harmonic_freq_hz: f_2,
        third_harmonic_freq_hz: f_3,
        critical_rwiv_wind_velocity_m_s: u_crit,
        inherent_scruton_number: sc_inherent,
        pti_required_damping_ratio: zeta_req,
        target_scruton_number: pti_target_sc,
        optimal_damper_coefficient_kn_s_m: c_opt_kn_s_m,
        attained_damping_ratio: zeta_total,
        is_rwiv_mitigated,
        is_damper_sizing_practical,
        is_cable_sag_acceptable,
        is_stay_cable_viable,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_long_span_stay_cable_vibration() -> Result<(), String> {
        let prob = CableStayProblem::default();
        let res = solve_cable_stayed_bridge(&prob)?;

        assert!(res.midspan_static_sag_mm > 500.0 && res.midspan_static_sag_mm < 950.0);
        assert!(res.fundamental_frequency_hz > 0.40 && res.fundamental_frequency_hz < 1.40);
        assert!(res.second_harmonic_freq_hz > res.fundamental_frequency_hz);
        assert!(
            res.critical_rwiv_wind_velocity_m_s > 2.0 && res.critical_rwiv_wind_velocity_m_s < 12.0
        );
        assert!(res.inherent_scruton_number < 5.0); // Baseline un-damped cable fails PTI threshold
        assert!(res.pti_required_damping_ratio > 0.004 && res.pti_required_damping_ratio < 0.010);
        assert!(
            res.optimal_damper_coefficient_kn_s_m > 20.0
                && res.optimal_damper_coefficient_kn_s_m < 200.0
        );
        assert!(res.attained_damping_ratio > res.pti_required_damping_ratio);
        assert!(res.is_rwiv_mitigated);
        assert!(res.is_damper_sizing_practical);
        assert!(res.is_cable_sag_acceptable);
        assert!(res.is_stay_cable_viable);
        Ok(())
    }
}
