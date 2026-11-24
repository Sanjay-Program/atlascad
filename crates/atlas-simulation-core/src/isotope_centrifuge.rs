//! Gas Centrifuge Isotopic Separation & Separative Work (Cohen-Onsager / Dirac-Peierls / ISO 9001)
//!
//! Provides first-principles closed-form analytical solutions for:
//! 1. High-speed spinning cylinder centrifugal acceleration (g-force) and composite rotor hoop stress.
//! 2. Elementary radial equilibrium separation factor alpha_0 for UF6 gas (Delta M = 0.003 kg/mol).
//! 3. Counter-current axial circulation flow multiplication and overall stage separation factor alpha.
//! 4. Dirac-Peierls isotopic value function V(x) and stage separative power delta_U (kg SWU/year).
//! 5. Cohen theoretical maximum separative capacity delta_U_max and centrifuge cascade stage yield.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

/// High-strength rotor cylinder tube material for gas centrifuges.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CentrifugeRotorMaterial {
    /// High-Modulus Carbon Fiber Reinforced Polymer (CFRP): Ultimate strength ~ 3,500 MPa, speed up to 800 m/s.
    CarbonFiberCfrp,
    /// Maraging Steel (Grade 350): Ultimate tensile strength ~ 2,400 MPa, speed up to 500 m/s.
    MaragingSteel350,
    /// High-Strength Aluminum Alloy (AA 7075-T6): Tensile strength ~ 570 MPa, speed up to 400 m/s.
    Aluminum7075,
}

/// Input parameters for uranium hexafluoride (UF6) gas centrifuge analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IsotopeCentrifugeProblem {
    /// Problem identifier.
    pub problem_id: String,
    /// Rotor structural material.
    pub rotor_material: CentrifugeRotorMaterial,
    /// Rotor cylinder outer radius r_0 in meters (m, standard 0.08 to 0.25 m).
    pub rotor_radius_m: f64,
    /// Active rotor tube axial length L in meters (m, standard 1.0 to 6.0 m).
    pub rotor_length_m: f64,
    /// Rotor rotational shaft speed in revolutions per minute (RPM, standard 40,000 to 110,000 RPM).
    pub rotational_speed_rpm: f64,
    /// UF6 process gas operating temperature in degrees Celsius (°C, standard 40 to 80 °C).
    pub operating_temperature_c: f64,
    /// Total stage feed mass flow rate F in kilograms of UF6 per hour (kg/h, standard 0.05 to 5.0 kg/h).
    pub feed_mass_flow_kg_h: f64,
    /// Feed stream U-235 isotopic atom fraction x_f (natural uranium = 0.00711 = 0.711%).
    pub feed_assay_u235_fraction: f64,
    /// Stage cut theta = P / F (product mass fraction, standard 0.40 to 0.60).
    pub stage_cut_theta: f64,
    /// Counter-current axial circulation flow efficiency factor E (standard 0.60 to 0.85).
    pub circulation_efficiency: f64,
}

impl Default for IsotopeCentrifugeProblem {
    fn default() -> Self {
        Self {
            problem_id: "commercial_cfrp_uf6_gas_centrifuge".to_string(),
            rotor_material: CentrifugeRotorMaterial::CarbonFiberCfrp,
            rotor_radius_m: 0.10,              // Ø200 mm rotor
            rotor_length_m: 3.50,              // 3.5 m active length
            rotational_speed_rpm: 65000.0,     // 65,000 RPM (v_tip ~ 680 m/s)
            operating_temperature_c: 60.0,     // 60 °C (333.15 K)
            feed_mass_flow_kg_h: 0.45,         // 0.45 kg/h UF6 feed
            feed_assay_u235_fraction: 0.00711, // 0.711% U-235 natural feed
            stage_cut_theta: 0.50,             // Symmetric stage cut (theta = 0.50)
            circulation_efficiency: 0.75,      // 75% Cohen axial circulation efficiency
        }
    }
}

/// Simulation results for isotope separation, separative work (SWU), and mechanical safety.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IsotopeCentrifugeResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Rotor peripheral linear surface speed v_tip = omega * r_0 in meters per second (m/s).
    pub rotor_peripheral_speed_m_s: f64,
    /// Centrifugal acceleration at rotor wall in g-forces (g = 9.80665 m/s²).
    pub centrifugal_acceleration_g: f64,
    /// Centrifugal hoop stress sigma_hoop in MegaPascals (MPa).
    pub rotor_hoop_stress_mpa: f64,
    /// Rotor burst strength structural safety factor (sigma_ult / sigma_hoop).
    pub rotor_burst_safety_factor: f64,
    /// Elementary radial equilibrium separation factor alpha_0.
    pub elementary_separation_factor_alpha0: f64,
    /// Overall counter-current stage separation factor alpha.
    pub overall_stage_separation_factor_alpha: f64,
    /// Enriched product stream U-235 assay fraction x_p.
    pub product_assay_u235_fraction: f64,
    /// Depleted waste tails stream U-235 assay fraction x_w.
    pub tails_assay_u235_fraction: f64,
    /// Dirac-Peierls value function for feed stream V(x_f).
    pub feed_value_function: f64,
    /// Annual stage separative work delta_U in kilograms of Separative Work Units per year (kg SWU/yr).
    pub separative_power_swu_per_year: f64,
    /// Cohen theoretical maximum thermodynamic separative power limit in kg SWU/year.
    pub cohen_maximum_separative_power_swu_yr: f64,
    /// Stage separative work efficiency (delta_U / delta_U_max) in percentage (%).
    pub separative_efficiency_pct: f64,
    /// Whether rotor peripheral speed is within material safe stress limit (FS >= 1.30).
    pub is_rotor_stress_safe: bool,
    /// Whether stage isotopic separation factor exceeds unity (alpha > 1.05).
    pub is_separation_effective: bool,
    /// Whether annual separative capacity satisfies commercial centrifuge benchmarks (>= 10 kg SWU/yr).
    pub is_swu_capacity_commercial: bool,
    /// Overall gas centrifuge isotopic enrichment stage feasibility status.
    pub is_centrifuge_stage_viable: bool,
}

/// Helper function to compute the Dirac-Peierls value function: V(x) = (2x - 1) * ln(x / (1 - x)).
fn dirac_peierls_value(x: f64) -> f64 {
    let x_safe = x.clamp(1.0e-6, 1.0 - 1.0e-6);
    (2.0 * x_safe - 1.0) * (x_safe / (1.0 - x_safe)).ln()
}

/// Solves uranium hexafluoride gas centrifuge separation, Cohen-Onsager dynamics, and SWU capacity.
pub fn solve_isotope_centrifuge(
    problem: &IsotopeCentrifugeProblem,
) -> Result<IsotopeCentrifugeResult, String> {
    if problem.rotor_radius_m <= 0.02 || problem.rotor_radius_m > 0.50 {
        return Err("Rotor radius must be between 0.02 and 0.50 m".to_string());
    }
    if problem.rotor_length_m <= 0.20 || problem.rotor_length_m > 12.0 {
        return Err("Rotor length must be between 0.20 and 12.0 m".to_string());
    }
    if problem.rotational_speed_rpm <= 1000.0 {
        return Err("Rotational speed must exceed 1000 RPM".to_string());
    }
    if problem.feed_mass_flow_kg_h <= 0.001 {
        return Err("Feed flow rate must be positive".to_string());
    }
    if problem.feed_assay_u235_fraction <= 0.0005 || problem.feed_assay_u235_fraction >= 0.95 {
        return Err("Feed assay must be between 0.05% and 95% U-235".to_string());
    }
    if problem.stage_cut_theta <= 0.10 || problem.stage_cut_theta >= 0.90 {
        return Err("Stage cut must be between 0.10 and 0.90".to_string());
    }

    // Rotor material properties: (density_kg_m3, sigma_ult_mpa)
    let (rho_mat, sigma_ult_mpa) = match problem.rotor_material {
        CentrifugeRotorMaterial::CarbonFiberCfrp => (1550.0, 3600.0), // Toray T1000 carbon fiber
        CentrifugeRotorMaterial::MaragingSteel350 => (8050.0, 2450.0), // Vascomax 350
        CentrifugeRotorMaterial::Aluminum7075 => (2810.0, 570.0),     // Al 7075-T6
    };

    let omega = (2.0 * std::f64::consts::PI * problem.rotational_speed_rpm) / 60.0; // rad/s
    let v_tip = omega * problem.rotor_radius_m; // m/s

    // Centrifugal acceleration at rotor wall:
    let g_accel = (v_tip * v_tip) / problem.rotor_radius_m;
    let g_force = g_accel / 9.80665;

    // Centrifugal hoop stress: sigma = rho * v^2 [Pa] -> [MPa]:
    let sigma_hoop_mpa = (rho_mat * v_tip * v_tip) / 1.0e6;
    let fs_burst = sigma_ult_mpa / sigma_hoop_mpa.max(1.0);

    // UF6 molecular masses: 235-UF6 = 349.03 g/mol, 238-UF6 = 352.04 g/mol
    // Delta M = 3.01 g/mol = 0.00301 kg/mol
    let delta_m = 0.00301; // kg/mol
    let r_gas = 8.31446; // J/(mol*K)
    let t_kelvin = problem.operating_temperature_c + 273.15;

    // Elementary radial equilibrium separation factor:
    // alpha_0 = exp((Delta_M * v_tip^2) / (2 * R * T))
    let exp_radial = (delta_m * v_tip * v_tip) / (2.0 * r_gas * t_kelvin);
    let alpha_0 = exp_radial.exp();

    // Cohen-Onsager counter-current axial circulation multiplication:
    // alpha = alpha_0 ^ [E * L / (2 * r_0)]
    let length_radius_ratio = problem.rotor_length_m / (2.0 * problem.rotor_radius_m);
    let axial_exponent = problem.circulation_efficiency * length_radius_ratio;
    // Clamped to avoid numerical overflow while capturing real centrifuge physics:
    let stage_alpha = alpha_0.powf(axial_exponent.min(1.5));

    // Product and tails assays:
    // With stage separation factor alpha and symmetric cut theta:
    // (x_p / (1 - x_p)) = alpha * (x_w / (1 - x_w))
    // Overall mass balance: x_f = theta * x_p + (1 - theta) * x_w
    let x_f = problem.feed_assay_u235_fraction;
    let theta = problem.stage_cut_theta;
    let x_p = (stage_alpha * x_f) / (1.0 + (stage_alpha - 1.0) * x_f);
    let x_w = (x_f - theta * x_p) / (1.0 - theta);
    let x_w_safe = x_w.max(0.0001);

    // Annual mass flows (kg UF6 / year):
    let hours_per_year = 8760.0;
    let f_year = problem.feed_mass_flow_kg_h * hours_per_year;
    let p_year = theta * f_year;
    let w_year = (1.0 - theta) * f_year;

    // Dirac-Peierls value functions:
    let v_f = dirac_peierls_value(x_f);
    let v_p = dirac_peierls_value(x_p);
    let v_w = dirac_peierls_value(x_w_safe);

    // Separative Work: delta_U = P * V(x_p) + W * V(x_w) - F * V(x_f) [kg SWU/yr]:
    let delta_u_swu_yr = (p_year * v_p + w_year * v_w - f_year * v_f).max(0.1);

    // Cohen's theoretical maximum separative power limit (diffusion-controlled limit):
    // delta_U_max = rho * D * [(Delta_M * v^2) / (2 * R * T)]^2 * (pi * L / 2) [kg/s -> kg/year]
    // For UF6 gas: rho * D ~ 2.35e-5 kg/(m*s) at 60 °C
    let rho_d = 2.35e-5; // kg/(m*s)
    let max_cohen_w_rate =
        rho_d * exp_radial.powi(2) * (std::f64::consts::PI * problem.rotor_length_m / 2.0);
    let delta_u_max_swu_yr = max_cohen_w_rate * 3600.0 * hours_per_year;

    let swu_eff_pct = if delta_u_max_swu_yr > 0.0 {
        (delta_u_swu_yr / delta_u_max_swu_yr) * 100.0
    } else {
        0.0
    };

    // Verification checks:
    let is_stress_ok = fs_burst >= 1.30;
    let is_sep_ok = stage_alpha > 1.05;
    let is_swu_ok = delta_u_swu_yr >= 10.0; // Commercial SWU benchmark >= 10 kg SWU/yr
    let is_overall_ok = is_stress_ok && is_sep_ok && is_swu_ok;

    Ok(IsotopeCentrifugeResult {
        problem_id: problem.problem_id.clone(),
        rotor_peripheral_speed_m_s: v_tip,
        centrifugal_acceleration_g: g_force,
        rotor_hoop_stress_mpa: sigma_hoop_mpa,
        rotor_burst_safety_factor: fs_burst,
        elementary_separation_factor_alpha0: alpha_0,
        overall_stage_separation_factor_alpha: stage_alpha,
        product_assay_u235_fraction: x_p,
        tails_assay_u235_fraction: x_w_safe,
        feed_value_function: v_f,
        separative_power_swu_per_year: delta_u_swu_yr,
        cohen_maximum_separative_power_swu_yr: delta_u_max_swu_yr,
        separative_efficiency_pct: swu_eff_pct,
        is_rotor_stress_safe: is_stress_ok,
        is_separation_effective: is_sep_ok,
        is_swu_capacity_commercial: is_swu_ok,
        is_centrifuge_stage_viable: is_overall_ok,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_commercial_cfrp_gas_centrifuge() -> Result<(), String> {
        let prob = IsotopeCentrifugeProblem::default();
        let res = solve_isotope_centrifuge(&prob)?;

        assert!(res.rotor_peripheral_speed_m_s > 600.0 && res.rotor_peripheral_speed_m_s < 750.0);
        assert!(res.centrifugal_acceleration_g > 350000.0);
        assert!(
            res.rotor_burst_safety_factor >= 1.30,
            "CFRP rotor should have safety factor >= 1.30"
        );
        assert!(res.elementary_separation_factor_alpha0 > 1.05);
        assert!(res.overall_stage_separation_factor_alpha > 1.10);
        assert!(res.product_assay_u235_fraction > prob.feed_assay_u235_fraction);
        assert!(res.tails_assay_u235_fraction < prob.feed_assay_u235_fraction);
        assert!(res.separative_power_swu_per_year >= 10.0);
        assert!(res.is_rotor_stress_safe);
        assert!(res.is_separation_effective);
        assert!(res.is_swu_capacity_commercial);
        assert!(res.is_centrifuge_stage_viable);
        Ok(())
    }
}
