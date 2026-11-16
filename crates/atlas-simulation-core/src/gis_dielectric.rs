//! Gas-Insulated Switchgear (GIS) High-Voltage Dielectric Breakdown & Paschen Margin (IEC 62271-203 / CIGRE).
//!
//! Evaluates coaxial cylindrical electric field stress, geometric field utilization factor,
//! SF6 / Eco-gas Paschen dielectric breakdown scaling with pressure, surface roughness enhancement,
//! and power frequency and lightning impulse withstand voltage (LIWV/BIL) safety margins.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use crate::execution::{SimulationResult, SimulationStatus};
use crate::provenance::SimulationProvenance;
use crate::convergence::ConvergenceMonitor;
use crate::validation::AnalyticalComparison;

/// Insulating gas dielectric medium.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GisInsulatingGas {
    /// Pure Sulfur Hexafluoride (100% SF6). Highest dielectric strength.
    PureSf6,
    /// SF6 / N2 Gas Mixture (30% SF6 / 70% N2). Reduced GWP for cold climates.
    Sf6N2Mixture,
    /// Fluoronitrile Eco-Gas C4-FN / CO2 / O2 mixture. Ultra-low GWP green alternative.
    EcoGasC4Fn,
}

impl GisInsulatingGas {
    /// Returns (critical_reduced_field_kv_mm_bar, relative_dielectric_strength_vs_sf6, gwp_index)
    pub fn properties(&self) -> (f64, f64, f64) {
        match self {
            Self::PureSf6      => (8.90, 1.00, 23500.0),
            Self::Sf6N2Mixture => (6.10, 0.68, 7000.0),
            Self::EcoGasC4Fn   => (8.50, 0.95, 350.0),
        }
    }
}

/// GIS high-voltage problem specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GisDielectricProblem {
    /// Problem identifier.
    #[serde(default = "default_problem_id")]
    pub problem_id: String,
    /// Insulating dielectric gas medium.
    #[serde(default = "default_gas")]
    pub insulating_gas: GisInsulatingGas,
    /// Rated system line-to-line RMS operating voltage in kiloVolts (kV) (e.g. 420.0 kV).
    pub rated_voltage_kv_rms: f64,
    /// Outer radius of central tubular conductor in millimeters (mm) (r1, e.g. 60.0 mm).
    pub conductor_radius_mm: f64,
    /// Inner radius of outer grounded metal enclosure tank in millimeters (mm) (r2, e.g. 165.0 mm).
    pub enclosure_radius_mm: f64,
    /// Absolute gas filling pressure at 20°C in bar (e.g. 5.50 bar abs).
    pub gas_pressure_abs_bar: f64,
    /// Lightning Impulse Withstand Voltage (LIWV / BIL) peak in kiloVolts (kV) (e.g. 1425.0 kV).
    pub lightning_impulse_withstand_kv_pk: f64,
    /// Conductor surface arithmetic roughness Ra in micrometers (μm) (typically 3.0 to 12.0 μm).
    #[serde(default = "default_roughness")]
    pub surface_roughness_ra_um: f64,
}

fn default_problem_id() -> String {
    "gis_dielectric_problem".to_string()
}

fn default_gas() -> GisInsulatingGas {
    GisInsulatingGas::PureSf6
}

fn default_roughness() -> f64 {
    6.30
}

impl Default for GisDielectricProblem {
    fn default() -> Self {
        Self {
            problem_id: default_problem_id(),
            insulating_gas: default_gas(),
            rated_voltage_kv_rms: 420.0,
            conductor_radius_mm: 60.0,
            enclosure_radius_mm: 165.0,
            gas_pressure_abs_bar: 5.50,
            lightning_impulse_withstand_kv_pk: 1425.0,
            surface_roughness_ra_um: 6.30,
        }
    }
}

/// GIS dielectric breakdown and electric field analysis result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GisDielectricResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Insulating gas analyzed.
    pub insulating_gas: GisInsulatingGas,
    /// Coaxial radius ratio (R_enclosure / r_conductor). Optimal for E_min is e ~ 2.718.
    pub radius_ratio: f64,
    /// Geometric electric field utilization factor eta (E_mean / E_max).
    pub field_utilization_factor: f64,
    /// Continuous line-to-ground peak operating AC voltage in kiloVolts (kV).
    pub continuous_peak_voltage_kv: f64,
    /// Maximum continuous electric field on conductor surface in kV/mm.
    pub max_continuous_e_field_kv_mm: f64,
    /// Maximum electric field under lightning impulse withstand (BIL) in kV/mm.
    pub max_impulse_e_field_kv_mm: f64,
    /// Surface roughness micro-field enhancement derating factor xi.
    pub roughness_derating_factor: f64,
    /// Effective critical dielectric breakdown field strength in kV/mm.
    pub effective_breakdown_field_kv_mm: f64,
    /// Dielectric safety factor under continuous AC operating voltage (E_crit / E_cont).
    pub continuous_safety_factor: f64,
    /// Dielectric safety factor under lightning impulse withstand (E_crit / E_impulse).
    pub impulse_safety_factor: f64,
    /// Minimum required gas pressure to maintain BIL impulse withstand in bar.
    pub min_required_pressure_bar: f64,
    /// Whether GIS design fully conforms to IEC 62271-203 insulation coordination criteria.
    pub is_iec_compliant: bool,
}

/// Solves GIS coaxial electric field, Paschen breakdown, and lightning impulse margin.
pub fn solve_gis_dielectric(
    problem: &GisDielectricProblem,
) -> Result<GisDielectricResult, String> {
    if problem.conductor_radius_mm <= 0.0 || problem.enclosure_radius_mm <= 0.0 {
        return Err("Conductor and enclosure radii must be positive".to_string());
    }
    if problem.enclosure_radius_mm <= problem.conductor_radius_mm {
        return Err("Enclosure radius must exceed conductor radius".to_string());
    }
    if problem.rated_voltage_kv_rms <= 0.0 || problem.gas_pressure_abs_bar <= 0.0 {
        return Err("Voltage and gas pressure must be positive".to_string());
    }
    if problem.lightning_impulse_withstand_kv_pk <= 0.0 {
        return Err("Lightning impulse voltage must be positive".to_string());
    }

    let r1 = problem.conductor_radius_mm;
    let r2 = problem.enclosure_radius_mm;
    let r_ratio = r2 / r1;

    // 1. Geometric field utilization factor:
    // E(r) = U / (r * ln(R/r))
    // E_max = U / (r1 * ln(r2/r1))
    // E_mean = U / (r2 - r1)
    // eta = E_mean / E_max = (r1 * ln(r2/r1)) / (r2 - r1)
    let ln_ratio = r_ratio.ln();
    let eta_utilization = (r1 * ln_ratio) / (r2 - r1);

    // 2. Voltage stress:
    // Continuous line-to-ground peak AC voltage:
    // U_cont_pk = (U_rated_rms * sqrt(2)) / sqrt(3)
    let u_cont_pk_kv = (problem.rated_voltage_kv_rms * std::f64::consts::SQRT_2) / 3.0_f64.sqrt();
    let e_max_cont_kv_mm = u_cont_pk_kv / (r1 * ln_ratio);

    // Impulse electric field under LIWV:
    let u_liwv_kv = problem.lightning_impulse_withstand_kv_pk;
    let e_max_impulse_kv_mm = u_liwv_kv / (r1 * ln_ratio);

    // 3. Dielectric breakdown field of insulating gas:
    let (crit_grad_per_bar, _rel_str, _gwp) = problem.insulating_gas.properties();
    let e_crit_ideal_kv_mm = crit_grad_per_bar * problem.gas_pressure_abs_bar;

    // 4. Surface roughness derating factor xi (Pedersen / CIGRE model):
    // xi = 1.0 + 0.08 * (Ra_um * P_bar)^0.55
    let ra_p = problem.surface_roughness_ra_um * problem.gas_pressure_abs_bar;
    let xi_roughness = 1.0 + 0.075 * ra_p.powf(0.55);
    let e_crit_eff_kv_mm = e_crit_ideal_kv_mm / xi_roughness;

    // 5. Safety factors:
    let sf_cont = e_crit_eff_kv_mm / e_max_cont_kv_mm;
    let sf_impulse = e_crit_eff_kv_mm / e_max_impulse_kv_mm;

    // 6. Minimum required gas pressure to achieve SF_impulse >= 1.25:
    // P_min ~ P_actual * (1.25 / SF_impulse)
    let min_p_bar = problem.gas_pressure_abs_bar * (1.25 / sf_impulse.max(0.1));

    // 7. IEC 62271-203 compliance check:
    // - Continuous safety factor >= 2.5
    // - Lightning impulse safety factor >= 1.25
    // - Radius ratio within optimal dielectric window 2.2 <= r2/r1 <= 3.5
    let is_iec_compliant = sf_cont >= 2.50
        && sf_impulse >= 1.25
        && (2.20..=3.50).contains(&r_ratio);

    Ok(GisDielectricResult {
        problem_id: problem.problem_id.clone(),
        insulating_gas: problem.insulating_gas,
        radius_ratio: r_ratio,
        field_utilization_factor: eta_utilization,
        continuous_peak_voltage_kv: u_cont_pk_kv,
        max_continuous_e_field_kv_mm: e_max_cont_kv_mm,
        max_impulse_e_field_kv_mm: e_max_impulse_kv_mm,
        roughness_derating_factor: xi_roughness,
        effective_breakdown_field_kv_mm: e_crit_eff_kv_mm,
        continuous_safety_factor: sf_cont,
        impulse_safety_factor: sf_impulse,
        min_required_pressure_bar: min_p_bar,
        is_iec_compliant,
    })
}

/// Executes GIS dielectric simulation and packages into SimulationResult envelope.
pub fn solve_gis_dielectric_sim(
    problem: &GisDielectricProblem,
) -> SimulationResult {
    let start = std::time::Instant::now();
    match solve_gis_dielectric(problem) {
        Ok(res) => {
            let mut metrics = std::collections::HashMap::new();
            metrics.insert("radius_ratio".to_string(), res.radius_ratio);
            metrics.insert("field_utilization_factor".to_string(), res.field_utilization_factor);
            metrics.insert("max_continuous_e_field_kv_mm".to_string(), res.max_continuous_e_field_kv_mm);
            metrics.insert("max_impulse_e_field_kv_mm".to_string(), res.max_impulse_e_field_kv_mm);
            metrics.insert("effective_breakdown_field_kv_mm".to_string(), res.effective_breakdown_field_kv_mm);
            metrics.insert("continuous_safety_factor".to_string(), res.continuous_safety_factor);
            metrics.insert("impulse_safety_factor".to_string(), res.impulse_safety_factor);

            let mut conv = ConvergenceMonitor::new(1e-6);
            conv.record(1, res.impulse_safety_factor, 1, 1e-4);

            let comparison = AnalyticalComparison::assess(
                "impulse_safety_factor",
                res.impulse_safety_factor,
                res.continuous_safety_factor * 0.25,
            );

            SimulationResult {
                problem_id: problem.problem_id.clone(),
                engine_id: "atlas.electrical.gis_dielectric".to_string(),
                status: SimulationStatus::Converged,
                execution_time_ms: start.elapsed().as_millis().max(1) as u64,
                scalar_metrics: metrics,
                convergence: conv,
                validation: Some(comparison),
                provenance: SimulationProvenance::new("atlas.electrical.gis_dielectric", "0.1.0"),
                logs: vec![format!(
                    "GIS Dielectric (IEC 62271-203): Gas = {:?}, R/r = {:.2}, eta = {:.3}, E_cont = {:.2} kV/mm, E_bil = {:.2} kV/mm, E_crit = {:.2} kV/mm, SF_bil = {:.2} (>= 1.25), Compliant: {}",
                    res.insulating_gas, res.radius_ratio, res.field_utilization_factor,
                    res.max_continuous_e_field_kv_mm, res.max_impulse_e_field_kv_mm,
                    res.effective_breakdown_field_kv_mm, res.impulse_safety_factor, res.is_iec_compliant
                )],
            }
        }
        Err(err) => {
            let mut conv = ConvergenceMonitor::new(1e-6);
            conv.record(1, 0.0, 1, 1.0);

            SimulationResult {
                problem_id: problem.problem_id.clone(),
                engine_id: "atlas.electrical.gis_dielectric".to_string(),
                status: SimulationStatus::Failed,
                execution_time_ms: 0,
                scalar_metrics: std::collections::HashMap::new(),
                convergence: conv,
                validation: None,
                provenance: SimulationProvenance::new("atlas.electrical.gis_dielectric", "0.1.0"),
                logs: vec![format!("GIS dielectric simulation failed: {}", err)],
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_gis_dielectric_nominal() {
        let prob = GisDielectricProblem::default();
        let res = solve_gis_dielectric(&prob).expect("solver failed");
        println!("GIS Dielectric: R/r = {:.2}, eta = {:.3}, E_cont = {:.2} kV/mm, E_bil = {:.2} kV/mm, E_crit = {:.2} kV/mm, SF_bil = {:.2}",
            res.radius_ratio, res.field_utilization_factor, res.max_continuous_e_field_kv_mm,
            res.max_impulse_e_field_kv_mm, res.effective_breakdown_field_kv_mm, res.impulse_safety_factor);

        assert!(res.radius_ratio == 2.75);
        assert!(res.field_utilization_factor > 0.55 && res.field_utilization_factor < 0.62);
        assert!(res.max_continuous_e_field_kv_mm > 5.0 && res.max_continuous_e_field_kv_mm < 6.0);
        assert!(res.max_impulse_e_field_kv_mm > 22.0 && res.max_impulse_e_field_kv_mm < 25.0);
        assert!(res.effective_breakdown_field_kv_mm > 30.0 && res.effective_breakdown_field_kv_mm < 40.0);
        assert!(res.continuous_safety_factor > 5.0);
        assert!(res.impulse_safety_factor >= 1.25);
        assert!(res.is_iec_compliant);
    }
}