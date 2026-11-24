//! Two-Phase Gas-Liquid Hydrocarbon Pipe Flow Assurance Solver.
//!
//! Implements classical Lockhart-Martinelli two-phase friction multipliers, Chisholm correlation,
//! Beggs-Brill liquid holdup, API RP 14E erosional velocity limits, and multiphase pressure drops
//! for subsea, gathering, and chemical processing pipelines.
//!
//! # Physical Formulations:
//! - **Superficial Velocities ($v_{sl}, v_{sg}$) & Input Liquid Volume Fraction ($\lambda_L$)**:
//!   $$v_{sl} = \frac{Q_L}{A}, \quad v_{sg} = \frac{Q_g}{A}, \quad v_m = v_{sl} + v_{sg}, \quad \lambda_L = \frac{v_{sl}}{v_m}$$
//! - **Lockhart-Martinelli Parameter ($X$)**:
//!   $$X = \left(\frac{\rho_L}{\rho_g}\right)^{0.5} \left(\frac{\mu_L}{\mu_g}\right)^{0.1} \left(\frac{v_{sl}}{v_{sg}}\right)^{0.9}$$
//! - **Chisholm Two-Phase Frictional Pressure Multiplier ($\Phi_L^2$)**:
//!   $$\Phi_L^2 = 1 + \frac{C_{Chisholm}}{X} + \frac{1}{X^2} \quad (C = 20 \text{ for turbulent-turbulent flow})$$
//!   $$\left(\frac{dP}{dL}\right)_{fric, 2\phi} = \Phi_L^2 \cdot \left(\frac{dP}{dL}\right)_L$$
//! - **In-Situ Liquid Holdup ($H_L$) (Beggs & Brill / Rouhani)**:
//!   $$H_L = \frac{\lambda_L}{\left[\lambda_L + (1 - \lambda_L) \cdot \left(\frac{\rho_g}{\rho_L}\right)^{0.33}\right]}$$
//! - **Two-Phase Mixture Density ($\rho_m$)**:
//!   $$\rho_m = H_L \cdot \rho_L + (1 - H_L) \cdot \rho_g \quad [\text{kg/m}^3]$$
//! - **API RP 14E Maximum Allowable Erosional Velocity ($v_e$)**:
//!   $$v_e = \frac{C_{API}}{\sqrt{\rho_m}} \quad [\text{m/s}] \quad (C_{API} \approx 122 \text{ for continuous solids-free service})$$
//! - **Total Pressure Loss ($\Delta P_{total}$)**:
//!   $$\Delta P_{total} = \left(\frac{dP}{dL}\right)_{fric, 2\phi} \cdot L + \rho_m \cdot g \cdot \Delta z \quad [\text{bar}]$$

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

/// Two-phase flow pattern regime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MultiphaseFlowRegime {
    /// Dispersed bubble flow (high liquid, low gas).
    DispersedBubble,
    /// Intermittent / slug flow (alternating liquid slugs and gas pockets).
    IntermittentSlug,
    /// Stratified smooth / wavy flow (low velocity horizontal).
    StratifiedFlow,
    /// Annular mist flow (high velocity gas core with liquid film).
    AnnularMist,
}

/// Input parameters for two-phase gas-liquid pipe flow assurance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiphasePipeFlowProblem {
    /// Problem identifier.
    pub problem_id: String,
    /// Pipeline total length in meters (m).
    pub pipe_length_m: f64,
    /// Pipe internal diameter in millimeters (mm).
    pub pipe_inner_diameter_mm: f64,
    /// Pipeline elevation change (outlet - inlet) in meters (m).
    pub elevation_change_m: f64,
    /// Liquid mass flow rate in kilograms per second (kg/s).
    pub liquid_mass_flow_kg_s: f64,
    /// Gas mass flow rate in kilograms per second (kg/s).
    pub gas_mass_flow_kg_s: f64,
    /// Liquid density (oil/water) in kg/m³.
    pub liquid_density_kg_m3: f64,
    /// Gas density (natural gas/vapor) at operating pressure in kg/m³.
    pub gas_density_kg_m3: f64,
    /// Liquid dynamic viscosity in mPa·s (cP).
    pub liquid_viscosity_mpa_s: f64,
    /// Gas dynamic viscosity in mPa·s (cP).
    pub gas_viscosity_mpa_s: f64,
    /// Pipe absolute wall roughness in millimeters (mm, standard 0.045 mm for commercial steel).
    pub pipe_roughness_mm: f64,
    /// API RP 14E empirical erosion constant C (standard 100 to 125 for solids-free service).
    pub api_erosion_constant_c: f64,
}

impl Default for MultiphasePipeFlowProblem {
    fn default() -> Self {
        Self {
            problem_id: "subsea_tieback_10km".to_string(),
            pipe_length_m: 8500.0,
            pipe_inner_diameter_mm: 254.0, // 10-inch standard pipe
            elevation_change_m: 120.0,     // uphill flow
            liquid_mass_flow_kg_s: 35.0,   // ~ 19,000 bpd oil
            gas_mass_flow_kg_s: 4.50,      // ~ 18 MMSCFD gas
            liquid_density_kg_m3: 840.0,   // 36° API crude oil
            gas_density_kg_m3: 42.0,       // compressed gas @ 60 bar
            liquid_viscosity_mpa_s: 2.80,
            gas_viscosity_mpa_s: 0.016,
            pipe_roughness_mm: 0.045,
            api_erosion_constant_c: 122.0,
        }
    }
}

/// Simulation results for multiphase flow assurance analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiphasePipeFlowResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Liquid superficial velocity in meters per second (m/s).
    pub liquid_superficial_velocity_m_s: f64,
    /// Gas superficial velocity in meters per second (m/s).
    pub gas_superficial_velocity_m_s: f64,
    /// Total mixture velocity in meters per second (m/s).
    pub mixture_velocity_m_s: f64,
    /// No-slip input liquid volume fraction lambda_L.
    pub input_liquid_fraction_lambda_l: f64,
    /// In-situ liquid holdup fraction H_L.
    pub in_situ_liquid_holdup_hl: f64,
    /// Average two-phase mixture density in kg/m³.
    pub mixture_density_kg_m3: f64,
    /// Lockhart-Martinelli parameter X.
    pub lockhart_martinelli_param_x: f64,
    /// Chisholm two-phase frictional pressure drop multiplier Phi_L^2.
    pub chisholm_two_phase_multiplier: f64,
    /// Identified multiphase flow regime.
    pub flow_regime: MultiphaseFlowRegime,
    /// Two-phase frictional pressure gradient in Pascals per meter (Pa/m).
    pub frictional_gradient_pa_per_m: f64,
    /// Total frictional pressure drop over the line in bar.
    pub frictional_pressure_drop_bar: f64,
    /// Hydrostatic / gravitational head pressure change in bar.
    pub hydrostatic_head_loss_bar: f64,
    /// Total pipeline pressure drop in bar.
    pub total_pressure_drop_bar: f64,
    /// API RP 14E maximum allowable erosional velocity limit in m/s.
    pub api_erosional_velocity_limit_m_s: f64,
    /// Erosional velocity ratio (v_mix / v_erosion). Must be <= 1.0.
    pub erosional_velocity_ratio: f64,
    /// Whether mixture velocity is safe from erosion per API RP 14E (ratio <= 1.0).
    pub is_erosion_velocity_acceptable: bool,
    /// Whether pipeline pressure drop is within normal pump/compressor limits (< 35 bar).
    pub is_pressure_drop_acceptable: bool,
    /// Whether flow regime is free from severe liquid slugging.
    pub is_slugging_risk_low: bool,
}

/// Solves two-phase gas-liquid pipe flow assurance, Lockhart-Martinelli pressure drop, and API 14E erosion.
pub fn solve_multiphase_pipe_flow(
    problem: &MultiphasePipeFlowProblem,
) -> Result<MultiphasePipeFlowResult, String> {
    if problem.pipe_length_m <= 0.0 {
        return Err("Pipe length must be positive".to_string());
    }
    if problem.pipe_inner_diameter_mm <= 5.0 {
        return Err("Pipe diameter must be greater than 5 mm".to_string());
    }
    if problem.liquid_mass_flow_kg_s <= 0.0 || problem.gas_mass_flow_kg_s <= 0.0 {
        return Err(
            "Both liquid and gas mass flow rates must be positive for two-phase flow".to_string(),
        );
    }
    if problem.liquid_density_kg_m3 <= 0.0 || problem.gas_density_kg_m3 <= 0.0 {
        return Err("Fluid densities must be positive".to_string());
    }

    let d_m = problem.pipe_inner_diameter_mm / 1000.0;
    let area_m2 = std::f64::consts::PI * (d_m / 2.0).powi(2);

    // 1. Volumetric flow rates and superficial velocities:
    let q_l_m3_s = problem.liquid_mass_flow_kg_s / problem.liquid_density_kg_m3;
    let q_g_m3_s = problem.gas_mass_flow_kg_s / problem.gas_density_kg_m3;

    let v_sl = q_l_m3_s / area_m2;
    let v_sg = q_g_m3_s / area_m2;
    let v_m = v_sl + v_sg;

    let lambda_l = v_sl / v_m;

    // 2. Lockhart-Martinelli parameter X:
    // X = sqrt( (dp/dL)_L / (dp/dL)_G ) ~ (rho_L / rho_G)^0.5 * (mu_L / mu_G)^0.1 * (v_sl / v_sg)^0.9
    let density_ratio = problem.liquid_density_kg_m3 / problem.gas_density_kg_m3;
    let viscosity_ratio = problem.liquid_viscosity_mpa_s / problem.gas_viscosity_mpa_s;
    let velocity_ratio = v_sl / v_sg;
    let x_lm = density_ratio.powf(0.5) * viscosity_ratio.powf(0.1) * velocity_ratio.powf(0.9);

    // 3. Chisholm correlation for two-phase frictional multiplier:
    // For turbulent-turbulent flow (standard in oil/gas pipelines), C = 20
    let c_chisholm = 20.0;
    let phi_l_squared = 1.0 + (c_chisholm / x_lm) + (1.0 / (x_lm * x_lm));

    // 4. Liquid-alone friction gradient (Darcy-Weisbach):
    let mu_l_pa_s = problem.liquid_viscosity_mpa_s / 1000.0;
    let re_l = (problem.liquid_density_kg_m3 * v_sl * d_m) / mu_l_pa_s;
    // Swamee-Jain single-phase friction factor:
    let rel_rough = (problem.pipe_roughness_mm / 1000.0) / d_m;
    let f_l = 0.25 / ((rel_rough / 3.7 + 5.74 / re_l.powf(0.9)).log10()).powi(2);
    let dp_dl_liquid_alone_pa_m = (f_l * problem.liquid_density_kg_m3 * v_sl * v_sl) / (2.0 * d_m);

    // 5. Two-phase frictional pressure gradient:
    let dp_dl_fric_2phase_pa_m = phi_l_squared * dp_dl_liquid_alone_pa_m;
    let dp_fric_bar = (dp_dl_fric_2phase_pa_m * problem.pipe_length_m) / 1e5;

    // 6. In-situ liquid holdup H_L (Rouhani / Beggs-Brill correlation):
    let h_l = lambda_l / (lambda_l + (1.0 - lambda_l) * (1.0 / density_ratio).powf(0.33));
    let h_l_clamped = h_l.clamp(0.01, 0.99);

    // 7. Mixture density:
    let rho_m = h_l_clamped * problem.liquid_density_kg_m3
        + (1.0 - h_l_clamped) * problem.gas_density_kg_m3;

    // 8. Hydrostatic head loss:
    let g = 9.80665;
    let dp_hydro_pa = rho_m * g * problem.elevation_change_m;
    let dp_hydro_bar = dp_hydro_pa / 1e5;

    let dp_total_bar = dp_fric_bar + dp_hydro_bar;

    // 9. API RP 14E Erosional Velocity:
    // v_e = C_api / sqrt(rho_m)
    let v_e = problem.api_erosion_constant_c / rho_m.sqrt();
    let erosion_ratio = v_m / v_e;

    // 10. Flow regime identification:
    let regime = if lambda_l > 0.85 {
        MultiphaseFlowRegime::DispersedBubble
    } else if v_sg > 15.0 {
        MultiphaseFlowRegime::AnnularMist
    } else if v_m < 1.2 && problem.elevation_change_m.abs() < 10.0 {
        MultiphaseFlowRegime::StratifiedFlow
    } else {
        MultiphaseFlowRegime::IntermittentSlug
    };

    let is_erosion_ok = erosion_ratio <= 1.0;
    let is_dp_ok = dp_total_bar <= 35.0; // standard commercial subsea tieback pump differential limit
    let is_slug_low = regime != MultiphaseFlowRegime::IntermittentSlug;

    Ok(MultiphasePipeFlowResult {
        problem_id: problem.problem_id.clone(),
        liquid_superficial_velocity_m_s: v_sl,
        gas_superficial_velocity_m_s: v_sg,
        mixture_velocity_m_s: v_m,
        input_liquid_fraction_lambda_l: lambda_l,
        in_situ_liquid_holdup_hl: h_l_clamped,
        mixture_density_kg_m3: rho_m,
        lockhart_martinelli_param_x: x_lm,
        chisholm_two_phase_multiplier: phi_l_squared,
        flow_regime: regime,
        frictional_gradient_pa_per_m: dp_dl_fric_2phase_pa_m,
        frictional_pressure_drop_bar: dp_fric_bar,
        hydrostatic_head_loss_bar: dp_hydro_bar,
        total_pressure_drop_bar: dp_total_bar,
        api_erosional_velocity_limit_m_s: v_e,
        erosional_velocity_ratio: erosion_ratio,
        is_erosion_velocity_acceptable: is_erosion_ok,
        is_pressure_drop_acceptable: is_dp_ok,
        is_slugging_risk_low: is_slug_low,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subsea_tieback_multiphase_flow() -> Result<(), String> {
        let prob = MultiphasePipeFlowProblem::default();
        let res = solve_multiphase_pipe_flow(&prob)?;

        assert!(
            res.liquid_superficial_velocity_m_s > 0.60
                && res.liquid_superficial_velocity_m_s < 1.20
        );
        assert!(res.gas_superficial_velocity_m_s > 1.50 && res.gas_superficial_velocity_m_s < 3.00);
        assert!(res.mixture_velocity_m_s > 2.0 && res.mixture_velocity_m_s < 4.0);
        assert!(res.in_situ_liquid_holdup_hl > 0.40 && res.in_situ_liquid_holdup_hl < 0.75);
        assert!(res.mixture_density_kg_m3 > 300.0 && res.mixture_density_kg_m3 < 600.0);
        assert!(
            res.chisholm_two_phase_multiplier > 5.0 && res.chisholm_two_phase_multiplier < 30.0
        );
        assert!(res.total_pressure_drop_bar > 5.0 && res.total_pressure_drop_bar < 25.0);
        assert!(
            res.api_erosional_velocity_limit_m_s > 4.5
                && res.api_erosional_velocity_limit_m_s < 8.0
        );
        assert!(res.is_erosion_velocity_acceptable);
        assert!(res.is_pressure_drop_acceptable);
        Ok(())
    }
}
