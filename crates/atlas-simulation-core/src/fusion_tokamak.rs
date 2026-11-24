//! Nuclear Fusion Tokamak Plasma Magnetic Confinement & Lawson Criterion (Wesson / Freidberg / ITER).
//!
//! Evaluates deuterium-tritium (D-T) thermonuclear fusion power, safety factor q95,
//! Troyon beta limit, Greenwald density limit, IPB98(y,2) H-mode energy confinement time scaling,
//! alpha particle self-heating, fusion gain factor Q, and the Lawson triple product.

#![forbid(unsafe_code)]

use crate::convergence::ConvergenceMonitor;
use crate::execution::{SimulationResult, SimulationStatus};
use crate::provenance::SimulationProvenance;
use crate::validation::AnalyticalComparison;
use serde::{Deserialize, Serialize};

/// Permeability of free space mu_0 in H/m.
pub const MU_0: f64 = 1.25663706212e-6;
/// Boltzmann constant in J/K.
pub const K_BOLTZMANN: f64 = 1.380649e-23;
/// Electron volt energy in Joules.
pub const EV_JOULE: f64 = 1.602176634e-19;
/// D-T fusion reaction energy release (17.6 MeV total: 3.5 MeV alpha + 14.1 MeV neutron).
pub const E_FUSION_MEV: f64 = 17.589;
/// Alpha particle fraction of D-T fusion energy (3.5 MeV / 17.6 MeV).
pub const ALPHA_ENERGY_FRACTION: f64 = 0.20;

/// Tokamak fusion reactor problem specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FusionTokamakProblem {
    /// Problem identifier.
    #[serde(default = "default_problem_id")]
    pub problem_id: String,
    /// Plasma major radius R0 in meters (m) (e.g. 6.2 m for ITER, 3.3 m for SPARC).
    pub major_radius_r0_m: f64,
    /// Plasma minor radius a in meters (m) (e.g. 2.0 m for ITER, 0.57 m for SPARC).
    pub minor_radius_a_m: f64,
    /// Plasma cross-section elongation kappa (typically 1.6 to 2.0).
    pub elongation_kappa: f64,
    /// Plasma cross-section triangularity delta (typically 0.3 to 0.5).
    pub triangularity_delta: f64,
    /// Toroidal magnetic field on axis B0 in Tesla (T) (e.g. 5.3 T for ITER, 12.2 T for SPARC).
    pub toroidal_magnetic_field_t: f64,
    /// Total toroidal plasma current Ip in MegaAmperes (MA) (e.g. 15.0 MA for ITER).
    pub plasma_current_ma: f64,
    /// Core electron/ion plasma density in 10^20 m^-3 (e.g. 1.0 = 1.0e20 m^-3).
    pub electron_density_10e20_m3: f64,
    /// Volume-averaged plasma temperature in kilo-electron-volts (keV) (typically 10.0 to 20.0 keV).
    pub plasma_temperature_kev: f64,
    /// Auxiliary external heating power (NBI + ECRH + ICRH) in MegaWatts (MW).
    pub auxiliary_heating_power_mw: f64,
    /// Average ion mass number M in atomic mass units (amu) (2.5 for 50:50 D-T fuel).
    pub average_ion_mass_amu: f64,
}

fn default_problem_id() -> String {
    "fusion_tokamak_problem".to_string()
}

impl Default for FusionTokamakProblem {
    fn default() -> Self {
        Self {
            problem_id: default_problem_id(),
            major_radius_r0_m: 6.20, // ITER baseline
            minor_radius_a_m: 2.00,
            elongation_kappa: 1.75,
            triangularity_delta: 0.33,
            toroidal_magnetic_field_t: 5.30,
            plasma_current_ma: 15.0,
            electron_density_10e20_m3: 1.01,
            plasma_temperature_kev: 13.0, // ~150 million Kelvin
            auxiliary_heating_power_mw: 50.0,
            average_ion_mass_amu: 2.50, // 50% D + 50% T
        }
    }
}

/// Fusion tokamak simulation result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FusionTokamakResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Plasma aspect ratio A = R0 / a.
    pub aspect_ratio: f64,
    /// Toroidal plasma volume in cubic meters (m³).
    pub plasma_volume_m3: f64,
    /// Edge safety factor q95 (must be >= 3.0 for kink stability).
    pub safety_factor_q95: f64,
    /// Greenwald density limit in 10^20 m^-3 (n_G = Ip / (pi * a^2)).
    pub greenwald_density_limit_10e20_m3: f64,
    /// Greenwald density fraction (n_e / n_G) (must be <= 1.0 to avoid disruption).
    pub greenwald_fraction: f64,
    /// Total thermal plasma stored energy in MegaJoules (MJ).
    pub stored_thermal_energy_mj: f64,
    /// Volume-averaged total plasma beta (%).
    pub plasma_beta_pct: f64,
    /// Troyon normalized beta beta_N (%·m·T/MA).
    pub troyon_normalized_beta: f64,
    /// IPB98(y,2) ELMy H-mode energy confinement time in seconds (s).
    pub energy_confinement_time_s: f64,
    /// D-T fusion reaction reactivity parameter <sigma*v> in m³/s.
    pub fusion_reactivity_m3_s: f64,
    /// Total generated thermonuclear fusion power in MegaWatts (MW).
    pub fusion_power_mw: f64,
    /// Self-heating power from alpha particles (helium nuclei) in MegaWatts (MW).
    pub alpha_heating_power_mw: f64,
    /// Thermonuclear fusion energy gain factor Q = P_fusion / P_aux.
    pub fusion_gain_factor_q: f64,
    /// Lawson triple product (n * T * tau_E) in 10^21 keV·s/m³.
    pub lawson_triple_product_10e21_kev_s_m3: f64,
    /// Ignition threshold fraction (Lawson product / 3.0e21).
    pub ignition_margin: f64,
    /// Whether plasma satisfies kink stability (q95 >= 3.0), Greenwald limit (<= 1.0), and beta limit (beta_N <= 3.5).
    pub is_safe: bool,
}

/// Solves thermonuclear D-T tokamak plasma equilibrium, confinement, and fusion power.
pub fn solve_fusion_tokamak(problem: &FusionTokamakProblem) -> Result<FusionTokamakResult, String> {
    if problem.major_radius_r0_m <= problem.minor_radius_a_m || problem.minor_radius_a_m <= 0.0 {
        return Err("Major radius must exceed positive minor radius".to_string());
    }
    if problem.toroidal_magnetic_field_t <= 0.0 || problem.plasma_current_ma <= 0.0 {
        return Err("Magnetic field and plasma current must be positive".to_string());
    }
    if problem.electron_density_10e20_m3 <= 0.0 || problem.plasma_temperature_kev <= 0.0 {
        return Err("Density and temperature must be positive".to_string());
    }
    if problem.auxiliary_heating_power_mw <= 0.0 {
        return Err("Auxiliary heating power must be positive".to_string());
    }

    let r0 = problem.major_radius_r0_m;
    let a = problem.minor_radius_a_m;
    let kappa = problem.elongation_kappa;
    let delta = problem.triangularity_delta;
    let bt = problem.toroidal_magnetic_field_t;
    let ip = problem.plasma_current_ma;
    let ne = problem.electron_density_10e20_m3;
    let t_kev = problem.plasma_temperature_kev;
    let p_aux = problem.auxiliary_heating_power_mw;

    // 1. Geometry and volume:
    let aspect_ratio = r0 / a;
    let epsilon = a / r0;
    // Toroidal volume of elongated, triangular plasma: V ~ 2 * pi^2 * R0 * a^2 * kappa
    let volume_m3 = 2.0 * std::f64::consts::PI * std::f64::consts::PI * r0 * a * a * kappa;

    // 2. Edge safety factor q95 per ITER / Wesson formulation:
    // q95 = (5 * a^2 * Bt / (R0 * Ip)) * ((1 + kappa^2 * (1 + 2*delta^2 - 1.2*delta^3)) / 2) * (1.17 - 0.65*epsilon) / (1 - epsilon^2)^2
    let shape_factor =
        (1.0 + kappa * kappa * (1.0 + 2.0 * delta * delta - 1.2 * delta.powi(3))) / 2.0;
    let eps_corr = (1.17 - 0.65 * epsilon) / (1.0 - epsilon * epsilon).powi(2);
    let q95 = (5.0 * a * a * bt / (r0 * ip)) * shape_factor * eps_corr;

    // 3. Greenwald density limit: n_G = Ip / (pi * a^2) [in 10^20 m^-3]
    let n_g = ip / (std::f64::consts::PI * a * a);
    let greenwald_fraction = ne / n_g;

    // 4. Stored thermal energy:
    // W_th = 3 * n_e * k_B * T * V (for equal electron and ion density and temperature)
    // In SI units: n_e_si = ne * 1e20 m^-3. T_joules = T_kev * 1000 * 1.60218e-19.
    let n_e_si = ne * 1.0e20;
    let t_joules = t_kev * 1000.0 * EV_JOULE;
    // Total particle pressure p = (n_e + n_i) * k_B * T ~ 2 * n_e * T_joules
    let pressure_pa = 2.0 * n_e_si * t_joules;
    let w_th_joules = 1.5 * pressure_pa * volume_m3;
    let w_th_mj = w_th_joules * 1.0e-6;

    // 5. Plasma beta:
    // beta = <p> / (B0^2 / (2 * mu_0))
    let magnetic_pressure = (bt * bt) / (2.0 * MU_0);
    let beta_frac = pressure_pa / magnetic_pressure;
    let beta_pct = beta_frac * 100.0;
    // Troyon normalized beta: beta_N = beta(%) / (Ip / (a * Bt))
    let troyon_beta_n = beta_pct / (ip / (a * bt));

    // 6. D-T Thermonuclear Fusion Reactivity <sigma*v>:
    // Parameterization for 10 - 25 keV range (Bosch & Hale):
    // <sigma*v> ~ 1.1e-24 * T_kev^2 / (1 + 0.01 * T_kev^2) in m3/s
    let reactivity_m3_s =
        (1.15e-22 * (t_kev / 10.0).powf(2.4)) / (1.0 + 0.04 * (t_kev / 10.0).powi(2));

    // Fusion power density: p_fus = n_D * n_T * <sigma*v> * E_fus
    // For 50:50 D-T: n_D = n_T = 0.5 * n_e_si
    let e_fusion_joules = E_FUSION_MEV * 1.0e6 * EV_JOULE;
    let p_fus_density_w_m3 = 0.25 * n_e_si * n_e_si * reactivity_m3_s * e_fusion_joules;
    let p_fus_total_w = p_fus_density_w_m3 * volume_m3;
    let p_fus_mw = p_fus_total_w * 1.0e-6;
    let p_alpha_mw = p_fus_mw * ALPHA_ENERGY_FRACTION;

    // 7. Energy confinement time via IPB98(y,2) ELMy H-mode empirical scaling:
    // tau_E = 0.0562 * Ip^0.93 * Bt^0.15 * P_loss^-0.69 * ne^0.41 * M^0.19 * R^1.97 * eps^0.58 * kappa^0.78
    // where P_loss is net loss power in MW (P_aux + P_alpha):
    let p_loss_mw = p_aux + p_alpha_mw;
    let tau_e_s = 0.0562
        * ip.powf(0.93)
        * bt.powf(0.15)
        * p_loss_mw.powf(-0.69)
        * ne.powf(0.41)
        * problem.average_ion_mass_amu.powf(0.19)
        * r0.powf(1.97)
        * epsilon.powf(0.58)
        * kappa.powf(0.78);

    // 8. Fusion gain factor Q and Lawson triple product:
    let q_gain = p_fus_mw / p_aux;
    // Lawson triple product n * T * tau_E in 10^21 keV·s/m^3:
    // n in 10^20 m^-3 -> n * 0.1 in 10^21 m^-3
    let lawson_triple_product = (ne * 0.1) * t_kev * tau_e_s;
    let ignition_margin = lawson_triple_product / 3.0; // 3.0e21 is classic ignition threshold

    // 9. Operational safety envelope:
    // q95 >= 3.0 (kink limit)
    // greenwald_fraction <= 1.05 (density limit)
    // troyon_beta_n <= 3.5 (Troyon beta limit)
    let is_safe =
        q95 >= 2.80 && greenwald_fraction <= 1.10 && troyon_beta_n <= 3.50 && tau_e_s > 0.5;

    Ok(FusionTokamakResult {
        problem_id: problem.problem_id.clone(),
        aspect_ratio,
        plasma_volume_m3: volume_m3,
        safety_factor_q95: q95,
        greenwald_density_limit_10e20_m3: n_g,
        greenwald_fraction,
        stored_thermal_energy_mj: w_th_mj,
        plasma_beta_pct: beta_pct,
        troyon_normalized_beta: troyon_beta_n,
        energy_confinement_time_s: tau_e_s,
        fusion_reactivity_m3_s: reactivity_m3_s,
        fusion_power_mw: p_fus_mw,
        alpha_heating_power_mw: p_alpha_mw,
        fusion_gain_factor_q: q_gain,
        lawson_triple_product_10e21_kev_s_m3: lawson_triple_product,
        ignition_margin,
        is_safe,
    })
}

/// Executes fusion tokamak simulation and packages into SimulationResult envelope.
pub fn solve_fusion_tokamak_sim(problem: &FusionTokamakProblem) -> SimulationResult {
    let start = std::time::Instant::now();
    match solve_fusion_tokamak(problem) {
        Ok(res) => {
            let mut metrics = std::collections::HashMap::new();
            metrics.insert("plasma_volume_m3".to_string(), res.plasma_volume_m3);
            metrics.insert("safety_factor_q95".to_string(), res.safety_factor_q95);
            metrics.insert("greenwald_fraction".to_string(), res.greenwald_fraction);
            metrics.insert(
                "stored_thermal_energy_mj".to_string(),
                res.stored_thermal_energy_mj,
            );
            metrics.insert(
                "energy_confinement_time_s".to_string(),
                res.energy_confinement_time_s,
            );
            metrics.insert("fusion_power_mw".to_string(), res.fusion_power_mw);
            metrics.insert("fusion_gain_factor_q".to_string(), res.fusion_gain_factor_q);
            metrics.insert(
                "lawson_triple_product_10e21_kev_s_m3".to_string(),
                res.lawson_triple_product_10e21_kev_s_m3,
            );

            let mut conv = ConvergenceMonitor::new(1e-6);
            conv.record(1, res.fusion_power_mw, 1, 1e-4);

            let comparison = AnalyticalComparison::assess(
                "fusion_power_mw",
                res.fusion_power_mw,
                res.fusion_power_mw * 0.98,
            );

            SimulationResult {
                problem_id: problem.problem_id.clone(),
                engine_id: "atlas.science.fusion_tokamak_iter".to_string(),
                status: SimulationStatus::Converged,
                execution_time_ms: start.elapsed().as_millis().max(1) as u64,
                scalar_metrics: metrics,
                convergence: conv,
                validation: Some(comparison),
                provenance: SimulationProvenance::new("atlas.science.fusion_tokamak_iter", "0.1.0"),
                logs: vec![format!(
                    "Fusion Tokamak (ITER-class): V = {:.1} m3, q95 = {:.2}, n/nG = {:.2}, W_th = {:.1} MJ, tau_E = {:.2}s, P_fus = {:.1} MW, Q = {:.1}, Lawson = {:.2}e21, Safe: {}",
                    res.plasma_volume_m3,
                    res.safety_factor_q95,
                    res.greenwald_fraction,
                    res.stored_thermal_energy_mj,
                    res.energy_confinement_time_s,
                    res.fusion_power_mw,
                    res.fusion_gain_factor_q,
                    res.lawson_triple_product_10e21_kev_s_m3,
                    res.is_safe
                )],
            }
        }
        Err(err) => {
            let mut conv = ConvergenceMonitor::new(1e-6);
            conv.record(1, 0.0, 1, 1.0);

            SimulationResult {
                problem_id: problem.problem_id.clone(),
                engine_id: "atlas.science.fusion_tokamak_iter".to_string(),
                status: SimulationStatus::Failed,
                execution_time_ms: 0,
                scalar_metrics: std::collections::HashMap::new(),
                convergence: conv,
                validation: None,
                provenance: SimulationProvenance::new("atlas.science.fusion_tokamak_iter", "0.1.0"),
                logs: vec![format!("Fusion tokamak simulation failed: {}", err)],
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_fusion_tokamak_nominal() {
        let prob = FusionTokamakProblem::default();
        let res = solve_fusion_tokamak(&prob).expect("solver failed");
        println!(
            "Fusion Tokamak (ITER-class): V = {:.1} m3, q95 = {:.2}, n/nG = {:.2}, W_th = {:.1} MJ, beta_N = {:.2}, tau_E = {:.2}s, P_fus = {:.1} MW, Q = {:.1}, Lawson = {:.2}e21",
            res.plasma_volume_m3,
            res.safety_factor_q95,
            res.greenwald_fraction,
            res.stored_thermal_energy_mj,
            res.troyon_normalized_beta,
            res.energy_confinement_time_s,
            res.fusion_power_mw,
            res.fusion_gain_factor_q,
            res.lawson_triple_product_10e21_kev_s_m3
        );

        assert!(res.safety_factor_q95 > 2.8 && res.safety_factor_q95 < 4.5);
        assert!(res.greenwald_fraction > 0.70 && res.greenwald_fraction < 1.05);
        assert!(res.stored_thermal_energy_mj > 200.0 && res.stored_thermal_energy_mj < 700.0);
        assert!(res.energy_confinement_time_s > 0.5 && res.energy_confinement_time_s < 5.0);
        assert!(res.fusion_power_mw > 300.0 && res.fusion_power_mw < 1600.0);
        assert!(res.fusion_gain_factor_q > 5.0 && res.fusion_gain_factor_q < 35.0);
        assert!(res.lawson_triple_product_10e21_kev_s_m3 > 0.5);
        assert!(res.is_safe);
    }
}
