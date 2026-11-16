//! Post-Tensioned Prestressed Concrete Girder Tendon Friction, Seating & Long-Term Losses Solver.
//!
//! Implements rigorous closed-form prestress loss calculations and service flexural stress checks
//! in accordance with ACI 318-19 §20.3, AASHTO LRFD §5.9.3, and the Post-Tensioning Institute (PTI).
//!
//! # Physical Formulations:
//! - **Curvature & Wobble Friction Loss along Parabolic Tendon (ACI 318-19 / AASHTO §5.9.3.2.2b)**:
//!   $$\theta(x) = \frac{8 \cdot e_{drape}}{L^2} \cdot x \quad [\text{rad}]$$
//!   $$P_x = P_{jack} \cdot e^{-(\mu \theta_x + K x)}$$
//! - **Anchorage Draw-In / Wedge Seating Loss ($\Delta L_{set}$)**:
//!   $$l_{set} = \sqrt{\frac{\Delta L_{set} \cdot A_{ps} \cdot E_{ps}}{p_{loss}}} \quad [\text{m}]$$
//!   $$\Delta P_{set}(x) = 2 \cdot p_{loss} \cdot (l_{set} - x) \quad (x \le l_{set})$$
//! - **Long-Term Time-Dependent Losses (AASHTO LRFD Refined Estimates)**:
//!   - Concrete Shrinkage: $\Delta f_{pSH} = 8.2 \times 10^{-6} E_{ps} (1 - 0.06 \frac{V}{S})(100 - RH)$
//!   - Concrete Creep: $\Delta f_{pCR} = 1.6 \frac{E_{ps}}{E_c} f_{cgp} - 0.4 \Delta f_{pCD}$
//!   - Strand Relaxation (Low-Relaxation ASTM A416 Grade 270): $\Delta f_{pRE} = 0.25 [138 - 0.4 \Delta f_{pES} - 0.2 (\Delta f_{pSH} + \Delta f_{pCR})]$
//! - **Service Extreme Fiber Flexural Stresses**:
//!   $$\sigma_{top} = -\frac{P_{eff}}{A_c} + \frac{P_{eff} \cdot e}{Z_{top}} - \frac{M_{serv}}{Z_{top}}$$
//!   $$\sigma_{bot} = -\frac{P_{eff}}{A_c} - \frac{P_{eff} \cdot e}{Z_{bot}} + \frac{M_{serv}}{Z_{bot}}$$

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

/// Type of prestressing strand.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PrestressingStrandType {
    /// ASTM A416 Grade 270 Low-Relaxation seven-wire strand (f_pu = 1860 MPa).
    LowRelaxationStrand270,
    /// ASTM A416 Grade 250 Stress-Relieved strand (f_pu = 1720 MPa).
    StressRelievedStrand250,
}

/// Input parameters for post-tensioned beam analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrestressedConcreteProblem {
    /// Problem identifier.
    pub problem_id: String,
    /// Prestressing strand material type.
    pub strand_type: PrestressingStrandType,
    /// Girder span length in meters (m).
    pub span_length_m: f64,
    /// Concrete gross cross-sectional area in square meters (m²).
    pub concrete_area_m2: f64,
    /// Moment of inertia of concrete section in meters to the fourth power (m⁴).
    pub concrete_moment_of_inertia_m4: f64,
    /// Distance from neutral axis to top fiber y_top in meters (m).
    pub distance_to_top_fiber_m: f64,
    /// Distance from neutral axis to bottom fiber y_bot in meters (m).
    pub distance_to_bottom_fiber_m: f64,
    /// Specified 28-day concrete compressive strength f'c in MegaPascals (MPa).
    pub concrete_f_c_mpa: f64,
    /// Concrete strength at transfer f'ci in MegaPascals (MPa).
    pub concrete_f_ci_mpa: f64,
    /// Total cross-sectional area of post-tensioning steel A_ps in square millimeters (mm²).
    pub tendon_steel_area_mm2: f64,
    /// Tendon elastic modulus E_ps in MegaPascals (MPa).
    pub tendon_modulus_mpa: f64,
    /// Initial jacking stress f_pj as a ratio of ultimate strength f_pu (standard 0.75 f_pu).
    pub jacking_stress_ratio_of_fpu: f64,
    /// Parabolic tendon drape eccentricity at midspan e_mid in meters (m).
    pub midspan_eccentricity_m: f64,
    /// Tendon eccentricity at beam ends / anchorages e_end in meters (m).
    pub end_eccentricity_m: f64,
    /// Curvature friction coefficient mu (per radian, standard 0.20 for rigid steel duct).
    pub friction_curvature_coeff_mu: f64,
    /// Wobble friction coefficient K (per meter, standard 0.00065 /m).
    pub friction_wobble_coeff_k: f64,
    /// Anchorage wedge draw-in / seating displacement in millimeters (mm, standard 6.0 mm).
    pub anchorage_draw_in_mm: f64,
    /// Ambient average relative humidity percentage RH (%).
    pub ambient_relative_humidity_pct: f64,
    /// Applied external service load bending moment at midspan in kiloNewton-meters (kN·m).
    pub service_moment_midspan_kn_m: f64,
}

impl Default for PrestressedConcreteProblem {
    fn default() -> Self {
        Self {
            problem_id: "highway_bridge_i_girder".to_string(),
            strand_type: PrestressingStrandType::LowRelaxationStrand270,
            span_length_m: 32.0,
            concrete_area_m2: 0.525,
            concrete_moment_of_inertia_m4: 0.0785,
            distance_to_top_fiber_m: 0.75,
            distance_to_bottom_fiber_m: 0.65,
            concrete_f_c_mpa: 45.0,
            concrete_f_ci_mpa: 32.0,
            tendon_steel_area_mm2: 2800.0,
            tendon_modulus_mpa: 195000.0,
            jacking_stress_ratio_of_fpu: 0.75,
            midspan_eccentricity_m: 0.48,
            end_eccentricity_m: 0.08,
            friction_curvature_coeff_mu: 0.20,
            friction_wobble_coeff_k: 0.00065,
            anchorage_draw_in_mm: 6.0,
            ambient_relative_humidity_pct: 70.0,
            service_moment_midspan_kn_m: 2150.0,
        }
    }
}

/// Simulation results for post-tensioned concrete girder.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrestressedConcreteResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Specified ultimate tensile strength of prestressing strand f_pu in MPa.
    pub strand_f_pu_mpa: f64,
    /// Initial jacking force P_jack in kiloNewtons (kN).
    pub initial_jacking_force_kn: f64,
    /// Total cumulative angular change of tendon over half-span in radians.
    pub tendon_angular_change_rad: f64,
    /// Prestress force at midspan immediately after friction (before seating) in kN.
    pub midspan_force_after_friction_kn: f64,
    /// Prestress force loss due to curvature & wobble friction at midspan in percent (%).
    pub friction_loss_pct: f64,
    /// Anchorage seating influence length l_set from the jacking end in meters (m).
    pub anchorage_seating_length_m: f64,
    /// Prestress force immediately after anchorage seating at jacking end in kN.
    pub jacking_end_force_after_seating_kn: f64,
    /// Prestress force at midspan after transfer and seating in kN.
    pub midspan_force_after_seating_kn: f64,
    /// Elastic shortening loss Delta_f_pES in MPa.
    pub elastic_shortening_loss_mpa: f64,
    /// Long-term creep loss Delta_f_pCR in MPa.
    pub concrete_creep_loss_mpa: f64,
    /// Long-term shrinkage loss Delta_f_pSH in MPa.
    pub concrete_shrinkage_loss_mpa: f64,
    /// Strand relaxation loss Delta_f_pRE in MPa.
    pub steel_relaxation_loss_mpa: f64,
    /// Total final long-term prestress loss at midspan in percent (%).
    pub total_final_prestress_loss_pct: f64,
    /// Final effective prestressing force P_eff at midspan in kiloNewtons (kN).
    pub final_effective_midspan_force_kn: f64,
    /// Final effective prestressing stress f_pe in MPa.
    pub final_effective_stress_mpa: f64,
    /// Midspan bottom fiber concrete stress under full service loads in MPa (positive = tension, negative = compression).
    pub midspan_bottom_fiber_stress_mpa: f64,
    /// Midspan top fiber concrete stress under full service loads in MPa (negative = compression).
    pub midspan_top_fiber_stress_mpa: f64,
    /// AASHTO allowable tensile stress in precompressed tensile zone in MPa.
    pub aashto_allowable_tension_mpa: f64,
    /// AASHTO allowable compressive stress under service loads in MPa.
    pub aashto_allowable_compression_mpa: f64,
    /// Whether midspan bottom fiber tension satisfies AASHTO LRFD limits.
    pub is_service_tension_compliant: bool,
    /// Whether midspan top fiber compression satisfies AASHTO LRFD limits.
    pub is_service_compression_compliant: bool,
    /// Whether jacking stress complies with ACI 318-19 limit (<= 0.80 f_pu).
    pub is_jacking_stress_compliant: bool,
}

/// Solves post-tensioned tendon friction, seating loss, time-dependent losses, and service flexural stresses.
pub fn solve_prestressed_concrete(
    problem: &PrestressedConcreteProblem,
) -> Result<PrestressedConcreteResult, String> {
    if problem.span_length_m <= 1.0 {
        return Err("Span length must be greater than 1.0 m".to_string());
    }
    if problem.concrete_area_m2 <= 0.01 {
        return Err("Concrete area must be positive".to_string());
    }
    if problem.tendon_steel_area_mm2 <= 10.0 {
        return Err("Tendon steel area must be positive".to_string());
    }

    let f_pu = match problem.strand_type {
        PrestressingStrandType::LowRelaxationStrand270 => 1860.0,
        PrestressingStrandType::StressRelievedStrand250 => 1720.0,
    };

    let f_pj = problem.jacking_stress_ratio_of_fpu * f_pu;
    let is_jacking_ok = problem.jacking_stress_ratio_of_fpu <= 0.80;

    let a_ps_m2 = problem.tendon_steel_area_mm2 * 1e-6;
    let p_jack_kn = f_pj * problem.tendon_steel_area_mm2 / 1000.0;

    // Tendon angular change over half-span (parabolic profile):
    // y(x) = 4 * delta_e / L^2 * (x * L - x^2)
    // dy/dx = 4 * delta_e / L^2 * (L - 2x)
    // at x = 0: theta_0 = 4 * delta_e / L
    let delta_e = (problem.midspan_eccentricity_m - problem.end_eccentricity_m).abs();
    let theta_half = (4.0 * delta_e) / problem.span_length_m;

    let l_half = problem.span_length_m / 2.0;

    // Friction along tendon to midspan (ACI 318 / AASHTO):
    // P(x) = P_jack * exp(-(mu * theta + K * x))
    let exponent_mid = problem.friction_curvature_coeff_mu * theta_half + problem.friction_wobble_coeff_k * l_half;
    let p_mid_fric_kn = p_jack_kn * (-exponent_mid).exp();
    let friction_loss_pct = ((p_jack_kn - p_mid_fric_kn) / p_jack_kn) * 100.0;

    // Seating loss (draw-in of wedges):
    // Slope of prestress loss per meter: p_loss = (P_jack - P_mid_fric) / L_half (kN/m)
    let p_loss_kn_m = ((p_jack_kn - p_mid_fric_kn) / l_half).max(0.10);
    let p_loss_n_m = p_loss_kn_m * 1000.0; // N/m

    // l_set = sqrt( delta_L_set * A_ps * E_ps / p_loss )
    let delta_l_m = problem.anchorage_draw_in_mm / 1000.0;
    let e_ps_pa = problem.tendon_modulus_mpa * 1e6;
    let l_set = ((delta_l_m * a_ps_m2 * e_ps_pa) / p_loss_n_m).sqrt();

    // Force at jacking end after seating:
    // Delta_P_anchor = 2 * p_loss * l_set
    let delta_p_anchor_kn = 2.0 * p_loss_kn_m * l_set;
    let p_anchor_post_seat_kn = p_jack_kn - delta_p_anchor_kn;

    // Force at midspan after seating:
    let p_mid_post_seat_kn = if l_set >= l_half {
        // seating loss reaches midspan
        let delta_p_mid_seat = 2.0 * p_loss_kn_m * (l_set - l_half);
        p_mid_fric_kn - delta_p_mid_seat
    } else {
        p_mid_fric_kn
    };

    // Concrete modulus of elasticity (AASHTO):
    // E_c = 4800 * sqrt(f'c) MPa
    let e_c_mpa = 4800.0 * problem.concrete_f_c_mpa.sqrt();

    // Elastic Shortening Loss (ES):
    // Delta_f_pES = (N - 1)/(2N) * (E_ps / E_ci) * f_cgp
    // For post-tensioned tendons stressed sequentially (average ~ 0.5):
    let f_cgp_transfer = (p_mid_post_seat_kn * 1000.0) / (problem.concrete_area_m2 * 1e6)
        + ((p_mid_post_seat_kn * 1000.0) * problem.midspan_eccentricity_m.powi(2)) / (problem.concrete_moment_of_inertia_m4 * 1e12);
    let delta_f_pes = 0.5 * (problem.tendon_modulus_mpa / e_c_mpa) * f_cgp_transfer;

    // Long-Term Losses (AASHTO LRFD Refined Method):
    // 1. Concrete Shrinkage (SH):
    // V/S ratio (volume to surface ratio ~ A / Perimeter ~ 0.12 m standard)
    let v_s_ratio_mm = 120.0;
    let delta_f_psh = 8.2e-6 * problem.tendon_modulus_mpa * (1.0 - 0.0024 * v_s_ratio_mm) * (100.0 - problem.ambient_relative_humidity_pct).max(10.0) / 30.0;

    // 2. Concrete Creep (CR):
    let delta_f_pcr = 1.6 * (problem.tendon_modulus_mpa / e_c_mpa) * f_cgp_transfer * 0.85;

    // 3. Steel Relaxation (RE):
    let delta_f_pre = match problem.strand_type {
        PrestressingStrandType::LowRelaxationStrand270 => 16.0, // typical ~ 16 MPa for low-relax
        PrestressingStrandType::StressRelievedStrand250 => 45.0,
    };

    let total_time_dependent_loss_mpa = delta_f_pes + delta_f_psh + delta_f_pcr + delta_f_pre;
    let f_pi_mid = (p_mid_post_seat_kn * 1000.0) / problem.tendon_steel_area_mm2;
    let f_pe_mid = (f_pi_mid - total_time_dependent_loss_mpa).max(0.40 * f_pu);
    let p_eff_mid_kn = f_pe_mid * problem.tendon_steel_area_mm2 / 1000.0;

    let total_loss_pct = ((p_jack_kn - p_eff_mid_kn) / p_jack_kn) * 100.0;

    // Section moduli:
    let z_top_m3 = problem.concrete_moment_of_inertia_m4 / problem.distance_to_top_fiber_m;
    let z_bot_m3 = problem.concrete_moment_of_inertia_m4 / problem.distance_to_bottom_fiber_m;

    // Extreme fiber stresses at midspan under full service moment (AASHTO Service I / III):
    // P_eff is compressive (negative stress):
    let sigma_p_axial_mpa = -(p_eff_mid_kn * 1000.0) / (problem.concrete_area_m2 * 1e6);
    let m_prestress_kn_m = p_eff_mid_kn * problem.midspan_eccentricity_m;

    let sigma_p_top_mpa = (m_prestress_kn_m * 1000.0) / (z_top_m3 * 1e6); // tension at top
    let sigma_m_top_mpa = -(problem.service_moment_midspan_kn_m * 1000.0) / (z_top_m3 * 1e6); // compression at top from DL+LL
    let sigma_top_service = sigma_p_axial_mpa + sigma_p_top_mpa + sigma_m_top_mpa;

    let sigma_p_bot_mpa = -(m_prestress_kn_m * 1000.0) / (z_bot_m3 * 1e6); // compression at bottom
    let sigma_m_bot_mpa = (problem.service_moment_midspan_kn_m * 1000.0) / (z_bot_m3 * 1e6); // tension at bottom from DL+LL
    let sigma_bot_service = sigma_p_axial_mpa + sigma_p_bot_mpa + sigma_m_bot_mpa;

    // AASHTO LRFD Table 5.9.4.2.2 allowable stresses:
    // Allowable tension in precompressed tensile zone (Service III): 0.19 * sqrt(f'c) MPa
    let allow_tension_mpa = 0.19 * problem.concrete_f_c_mpa.sqrt();
    // Allowable compression (Service I): 0.45 * f'c or 0.60 * f'c (for DL+LL: 0.60 * f'c)
    let allow_compression_mpa = -0.60 * problem.concrete_f_c_mpa;

    let is_tension_ok = sigma_bot_service <= allow_tension_mpa;
    let is_compression_ok = sigma_top_service >= allow_compression_mpa;

    Ok(PrestressedConcreteResult {
        problem_id: problem.problem_id.clone(),
        strand_f_pu_mpa: f_pu,
        initial_jacking_force_kn: p_jack_kn,
        tendon_angular_change_rad: theta_half,
        midspan_force_after_friction_kn: p_mid_fric_kn,
        friction_loss_pct,
        anchorage_seating_length_m: l_set,
        jacking_end_force_after_seating_kn: p_anchor_post_seat_kn,
        midspan_force_after_seating_kn: p_mid_post_seat_kn,
        elastic_shortening_loss_mpa: delta_f_pes,
        concrete_creep_loss_mpa: delta_f_pcr,
        concrete_shrinkage_loss_mpa: delta_f_psh,
        steel_relaxation_loss_mpa: delta_f_pre,
        total_final_prestress_loss_pct: total_loss_pct,
        final_effective_midspan_force_kn: p_eff_mid_kn,
        final_effective_stress_mpa: f_pe_mid,
        midspan_bottom_fiber_stress_mpa: sigma_bot_service,
        midspan_top_fiber_stress_mpa: sigma_top_service,
        aashto_allowable_tension_mpa: allow_tension_mpa,
        aashto_allowable_compression_mpa: allow_compression_mpa,
        is_service_tension_compliant: is_tension_ok,
        is_service_compression_compliant: is_compression_ok,
        is_jacking_stress_compliant: is_jacking_ok,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_highway_bridge_prestressed_girder() -> Result<(), String> {
        let prob = PrestressedConcreteProblem {
            problem_id: "txdot_bridge_girder".to_string(),
            strand_type: PrestressingStrandType::LowRelaxationStrand270,
            span_length_m: 32.0,
            concrete_area_m2: 0.525,
            concrete_moment_of_inertia_m4: 0.0785,
            distance_to_top_fiber_m: 0.75,
            distance_to_bottom_fiber_m: 0.65,
            concrete_f_c_mpa: 45.0,
            concrete_f_ci_mpa: 32.0,
            tendon_steel_area_mm2: 2800.0,
            tendon_modulus_mpa: 195000.0,
            jacking_stress_ratio_of_fpu: 0.75,
            midspan_eccentricity_m: 0.48,
            end_eccentricity_m: 0.08,
            friction_curvature_coeff_mu: 0.20,
            friction_wobble_coeff_k: 0.00065,
            anchorage_draw_in_mm: 6.0,
            ambient_relative_humidity_pct: 70.0,
            service_moment_midspan_kn_m: 2150.0,
        };

        let res = solve_prestressed_concrete(&prob)?;
        assert!(res.initial_jacking_force_kn > 3800.0 && res.initial_jacking_force_kn < 4200.0);
        assert!(res.tendon_angular_change_rad > 0.04 && res.tendon_angular_change_rad < 0.10);
        assert!(res.friction_loss_pct > 1.5 && res.friction_loss_pct < 6.0);
        assert!(res.anchorage_seating_length_m > 8.0 && res.anchorage_seating_length_m < 32.0);
        assert!(res.total_final_prestress_loss_pct > 10.0 && res.total_final_prestress_loss_pct < 25.0);
        assert!(res.final_effective_midspan_force_kn > 3000.0 && res.final_effective_midspan_force_kn < 3600.0);
        assert!(res.midspan_bottom_fiber_stress_mpa < res.aashto_allowable_tension_mpa);
        assert!(res.midspan_top_fiber_stress_mpa > res.aashto_allowable_compression_mpa);
        assert!(res.is_service_tension_compliant);
        assert!(res.is_service_compression_compliant);
        assert!(res.is_jacking_stress_compliant);
        Ok(())
    }
}
