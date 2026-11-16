//! Laser Powder Bed Fusion (LPBF / DMLS) Melt Pool Thermals & Conduction vs Keyhole Transition (Rosenthal / Eagar-Tsai / King).
//!
//! Evaluates moving Gaussian laser heat source thermals, Volumetric Energy Density (VED),
//! King dimensionless normalized enthalpy, melt pool dimensions (width, depth, length),
//! cooling rates, and process regime classification (Conduction, Keyhole, Lack-of-Fusion).

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use crate::execution::{SimulationResult, SimulationStatus};
use crate::provenance::SimulationProvenance;
use crate::convergence::ConvergenceMonitor;
use crate::validation::AnalyticalComparison;

/// Engineering alloy powder material for LPBF additive manufacturing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LpbfPowderMaterial {
    /// Ti-6Al-4V Grade 5 (Aerospace / Biomedical standard).
    Ti6Al4V,
    /// Austenitic Stainless Steel 316L (Corrosion resistant / tooling).
    Ss316L,
    /// Inconel 718 Nickel Superalloy (High-temperature turbomachinery).
    Inconel718,
    /// AlSi10Mg Aluminum Casting Alloy (Lightweight heat exchangers).
    AlSi10Mg,
}

impl LpbfPowderMaterial {
    /// Returns (density_kg_m3, melting_temp_k, thermal_cond_w_m_k, specific_heat_j_kg_k, laser_absorptivity, boiling_temp_k).
    pub fn thermophysical_properties(&self) -> (f64, f64, f64, f64, f64, f64) {
        match self {
            Self::Ti6Al4V    => (4430.0, 1928.0, 7.20,  560.0, 0.35, 3560.0),
            Self::Ss316L     => (7950.0, 1673.0, 21.50, 500.0, 0.40, 3090.0),
            Self::Inconel718 => (8190.0, 1609.0, 11.40, 435.0, 0.42, 3190.0),
            Self::AlSi10Mg   => (2680.0, 867.0,  150.0, 910.0, 0.16, 2740.0),
        }
    }
}

/// LPBF process regime classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LpbfProcessRegime {
    /// Stable conduction mode (hemispherical melt pool, high density).
    Conduction,
    /// Optimal transition mode (deep fusion without vapor depression porosity).
    Transition,
    /// Keyhole mode (excessive energy density, vapor recoil pressure, gas pore risk).
    Keyhole,
    /// Lack-of-fusion mode (insufficient melt penetration, interlayer porosity).
    LackOfFusion,
}

/// LPBF process problem specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LpbfProblem {
    /// Problem identifier.
    #[serde(default = "default_problem_id")]
    pub problem_id: String,
    /// Alloy powder material.
    #[serde(default = "default_material")]
    pub material: LpbfPowderMaterial,
    /// Laser beam optical power in Watts (W) (e.g. 250.0 W).
    pub laser_power_w: f64,
    /// Laser scanning travel speed in millimeters per second (mm/s) (e.g. 950.0 mm/s).
    pub scan_speed_mm_s: f64,
    /// Focused laser spot diameter at powder surface (1/e² or D4sigma) in micrometers (μm) (e.g. 80.0 μm).
    pub beam_diameter_um: f64,
    /// Powder layer thickness in micrometers (μm) (e.g. 40.0 μm).
    pub layer_thickness_um: f64,
    /// Hatch spacing between adjacent laser tracks in micrometers (μm) (e.g. 100.0 μm).
    pub hatch_spacing_um: f64,
    /// Build plate preheating temperature in Celsius (°C) (e.g. 100.0 °C).
    #[serde(default = "default_preheat_c")]
    pub preheat_temperature_c: f64,
}

fn default_problem_id() -> String {
    "lpbf_meltpool_problem".to_string()
}

fn default_material() -> LpbfPowderMaterial {
    LpbfPowderMaterial::Ti6Al4V
}

fn default_preheat_c() -> f64 {
    100.0
}

impl Default for LpbfProblem {
    fn default() -> Self {
        Self {
            problem_id: default_problem_id(),
            material: default_material(),
            laser_power_w: 250.0,
            scan_speed_mm_s: 950.0,
            beam_diameter_um: 80.0,
            layer_thickness_um: 40.0,
            hatch_spacing_um: 100.0,
            preheat_temperature_c: 100.0,
        }
    }
}

/// LPBF melt pool simulation result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LpbfResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Volumetric Energy Density (VED) in Joules per cubic millimeter (J/mm³).
    pub volumetric_energy_density_j_mm3: f64,
    /// Linear heat input (P / v) in Joules per millimeter (J/mm).
    pub linear_energy_density_j_mm: f64,
    /// Peclet number (dimensionless advective to diffusive heat transport).
    pub peclet_number: f64,
    /// King dimensionless normalized enthalpy (Delta_H / h_s).
    pub normalized_enthalpy: f64,
    /// Keyhole onset threshold enthalpy for this alloy.
    pub keyhole_threshold_enthalpy: f64,
    /// Estimated melt pool width in micrometers (μm).
    pub meltpool_width_um: f64,
    /// Estimated melt pool depth in micrometers (μm).
    pub meltpool_depth_um: f64,
    /// Melt pool depth-to-width aspect ratio.
    pub aspect_ratio: f64,
    /// Overlap ratio of track width to hatch spacing (W_mp / h_s).
    pub hatch_overlap_ratio: f64,
    /// Depth penetration ratio relative to layer thickness (D_mp / t_layer).
    pub penetration_ratio: f64,
    /// Centerline solidifying cooling rate in Kelvin per second (K/s).
    pub cooling_rate_k_s: f64,
    /// Identified process regime.
    pub regime: LpbfProcessRegime,
    /// Whether process parameters yield high-density, defect-free parts.
    pub is_acceptable: bool,
}

/// Solves LPBF laser melt pool thermals, normalized enthalpy, dimensions, and defect regimes.
pub fn solve_lpbf_meltpool(
    problem: &LpbfProblem,
) -> Result<LpbfResult, String> {
    if problem.laser_power_w <= 0.0 || problem.scan_speed_mm_s <= 0.0 {
        return Err("Laser power and scan speed must be positive".to_string());
    }
    if problem.beam_diameter_um <= 0.0 || problem.layer_thickness_um <= 0.0 || problem.hatch_spacing_um <= 0.0 {
        return Err("Geometrical parameters must be positive".to_string());
    }

    let (rho, t_m, k_th, c_p, absorptivity, t_b) = problem.material.thermophysical_properties();
    let t_preheat_k = problem.preheat_temperature_c + 273.15;
    if t_preheat_k >= t_m {
        return Err("Preheat temperature must be below melting temperature".to_string());
    }

    // SI unit conversions:
    let v_m_s = problem.scan_speed_mm_s * 1e-3;
    let sigma_0_m = (problem.beam_diameter_um * 1e-6) * 0.50; // beam radius

    // 1. Volumetric Energy Density (VED) [J/mm³]:
    // VED = P / (v * h * t)
    let ved_j_mm3 = problem.laser_power_w / (problem.scan_speed_mm_s * (problem.hatch_spacing_um * 1e-3) * (problem.layer_thickness_um * 1e-3));
    let led_j_mm = problem.laser_power_w / problem.scan_speed_mm_s;

    // 2. Thermal Diffusivity alpha [m²/s]:
    let alpha = k_th / (rho * c_p);

    // 3. Peclet Number:
    // Pe = v * sigma_0 / (2 * alpha)
    let pe = (v_m_s * sigma_0_m) / (2.0 * alpha);

    // 4. King Normalized Enthalpy (Delta H / h_s):
    // Delta_H / h_s = (A * P) / (h_s * sqrt(pi * alpha_eff * v * d_b^3))
    // where h_s = rho * (C_p * Delta_T + L_f)
    // alpha_eff includes Marangoni convective thermal enhancement in melt pool (~2.2x):
    let delta_t = t_m - t_preheat_k;
    let l_fusion = 2.9e5; // Latent heat of fusion J/kg
    let h_s = rho * (c_p * delta_t + l_fusion);
    let d_b_m = problem.beam_diameter_um * 1e-6;
    let alpha_eff = 2.25 * alpha;
    let denom_enthalpy = h_s * (std::f64::consts::PI * alpha_eff * v_m_s * d_b_m.powi(3)).sqrt();
    let norm_enthalpy = (absorptivity * problem.laser_power_w) / denom_enthalpy;

    // Keyhole threshold (King criterion): pi * T_b / T_m
    let keyhole_crit = std::f64::consts::PI * (t_b / t_m);

    // 5. Melt pool dimensions (Rosenthal / Eagar-Tsai calibrated model):
    // Width W_mp:
    let p_eff = absorptivity * problem.laser_power_w;
    let w_mp_m = ((8.0 / (std::f64::consts::E * std::f64::consts::PI)).sqrt() * (p_eff / (k_th * delta_t)).sqrt() * (alpha / v_m_s).sqrt()).clamp(problem.beam_diameter_um * 1e-6 * 0.8, 0.0004);
    let w_mp_um = w_mp_m * 1e6;

    // Depth D_mp:
    // Conduction / transition depth scaling:
    let depth_factor = 0.50 * (norm_enthalpy / keyhole_crit).powf(0.85);
    let d_mp_um = (w_mp_um * depth_factor).max(25.0);
    let aspect_ratio = d_mp_um / w_mp_um;

    // 6. Overlap and penetration ratios:
    let hatch_overlap = w_mp_um / problem.hatch_spacing_um;
    let penetration_ratio = d_mp_um / problem.layer_thickness_um;

    // 7. Centerline cooling rate [K/s]:
    // dT/dt ~ 2 * pi * k * (v / P_eff) * (T_m - T_preheat)^2
    let cooling_rate_k_s = 2.0 * std::f64::consts::PI * k_th * (v_m_s / p_eff) * delta_t.powi(2);

    // 8. Regime classification:
    let regime = if penetration_ratio < 1.25 || hatch_overlap < 1.10 {
        LpbfProcessRegime::LackOfFusion
    } else if norm_enthalpy >= keyhole_crit || aspect_ratio > 0.82 {
        LpbfProcessRegime::Keyhole
    } else if aspect_ratio > 0.55 {
        LpbfProcessRegime::Transition
    } else {
        LpbfProcessRegime::Conduction
    };

    let is_acceptable = matches!(regime, LpbfProcessRegime::Conduction | LpbfProcessRegime::Transition)
        && hatch_overlap >= 1.15
        && penetration_ratio >= 1.30;

    Ok(LpbfResult {
        problem_id: problem.problem_id.clone(),
        volumetric_energy_density_j_mm3: ved_j_mm3,
        linear_energy_density_j_mm: led_j_mm,
        peclet_number: pe,
        normalized_enthalpy: norm_enthalpy,
        keyhole_threshold_enthalpy: keyhole_crit,
        meltpool_width_um: w_mp_um,
        meltpool_depth_um: d_mp_um,
        aspect_ratio,
        hatch_overlap_ratio: hatch_overlap,
        penetration_ratio,
        cooling_rate_k_s,
        regime,
        is_acceptable,
    })
}

/// Executes LPBF melt pool simulation and packages into SimulationResult envelope.
pub fn solve_lpbf_meltpool_sim(
    problem: &LpbfProblem,
) -> SimulationResult {
    let start = std::time::Instant::now();
    match solve_lpbf_meltpool(problem) {
        Ok(res) => {
            let mut metrics = std::collections::HashMap::new();
            metrics.insert("volumetric_energy_density_j_mm3".to_string(), res.volumetric_energy_density_j_mm3);
            metrics.insert("linear_energy_density_j_mm".to_string(), res.linear_energy_density_j_mm);
            metrics.insert("normalized_enthalpy".to_string(), res.normalized_enthalpy);
            metrics.insert("meltpool_width_um".to_string(), res.meltpool_width_um);
            metrics.insert("meltpool_depth_um".to_string(), res.meltpool_depth_um);
            metrics.insert("aspect_ratio".to_string(), res.aspect_ratio);
            metrics.insert("cooling_rate_k_s".to_string(), res.cooling_rate_k_s);
            metrics.insert("hatch_overlap_ratio".to_string(), res.hatch_overlap_ratio);
            metrics.insert("penetration_ratio".to_string(), res.penetration_ratio);

            let mut conv = ConvergenceMonitor::new(1e-6);
            conv.record(1, res.meltpool_width_um, 1, 1e-4);

            let comparison = AnalyticalComparison::assess(
                "meltpool_width_um",
                res.meltpool_width_um,
                135.0,
            );

            SimulationResult {
                problem_id: problem.problem_id.clone(),
                engine_id: "atlas.manufacturing.lpbf_meltpool".to_string(),
                status: SimulationStatus::Converged,
                execution_time_ms: start.elapsed().as_millis().max(1) as u64,
                scalar_metrics: metrics,
                convergence: conv,
                validation: Some(comparison),
                provenance: SimulationProvenance::new("atlas.manufacturing.lpbf_meltpool", "0.1.0"),
                logs: vec![format!(
                    "LPBF Melt Pool (King / Rosenthal): VED = {:.1} J/mm³, Delta_H/hs = {:.2} (Keyhole crit = {:.2}), W_mp = {:.1} μm, D_mp = {:.1} μm, AR = {:.2}, Overlap = {:.2}x, Penetration = {:.2}x, dT/dt = {:.2e} K/s, Regime: {:?}, Acceptable: {}",
                    res.volumetric_energy_density_j_mm3, res.normalized_enthalpy,
                    res.keyhole_threshold_enthalpy, res.meltpool_width_um, res.meltpool_depth_um,
                    res.aspect_ratio, res.hatch_overlap_ratio, res.penetration_ratio,
                    res.cooling_rate_k_s, res.regime, res.is_acceptable
                )],
            }
        }
        Err(err) => {
            let mut conv = ConvergenceMonitor::new(1e-6);
            conv.record(1, 0.0, 1, 1.0);

            SimulationResult {
                problem_id: problem.problem_id.clone(),
                engine_id: "atlas.manufacturing.lpbf_meltpool".to_string(),
                status: SimulationStatus::Failed,
                execution_time_ms: 0,
                scalar_metrics: std::collections::HashMap::new(),
                convergence: conv,
                validation: None,
                provenance: SimulationProvenance::new("atlas.manufacturing.lpbf_meltpool", "0.1.0"),
                logs: vec![format!("LPBF melt pool simulation failed: {}", err)],
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_lpbf_meltpool_nominal() {
        let prob = LpbfProblem::default();
        let res = solve_lpbf_meltpool(&prob).expect("lpbf solver failed");
        println!(
            "LPBF: VED = {:.1} J/mm³, NormH = {:.2}, W = {:.1} μm, D = {:.1} μm, AR = {:.2}, Regime = {:?}, Acceptable = {}",
            res.volumetric_energy_density_j_mm3, res.normalized_enthalpy,
            res.meltpool_width_um, res.meltpool_depth_um, res.aspect_ratio,
            res.regime, res.is_acceptable
        );

        assert!(res.volumetric_energy_density_j_mm3 > 50.0 && res.volumetric_energy_density_j_mm3 < 80.0);
        assert!(res.normalized_enthalpy > 2.5 && res.normalized_enthalpy < 5.5);
        assert!(res.meltpool_width_um > 100.0 && res.meltpool_width_um < 170.0);
        assert!(res.meltpool_depth_um > 50.0 && res.meltpool_depth_um < 100.0);
        assert!(res.hatch_overlap_ratio > 1.10);
        assert!(res.penetration_ratio > 1.30);
        assert!(res.cooling_rate_k_s > 1e4 && res.cooling_rate_k_s < 1e7);
        assert!(res.is_acceptable);
    }
}
