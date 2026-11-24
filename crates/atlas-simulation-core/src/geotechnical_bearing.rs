//! Geotechnical Foundation Bearing Capacity & Settlement Solver (Terzaghi / Meyerhof / Eurocode 7 EN 1997-1).
//!
//! Evaluates ultimate soil bearing capacity q_ult, Meyerhof capacity factors (Nc, Nq, N_gamma),
//! shape/depth enhancements, allowable bearing capacity with safety factors,
//! and elastic immediate foundation settlement.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

use crate::convergence::ConvergenceMonitor;
use crate::execution::{SimulationResult, SimulationStatus};
use crate::provenance::SimulationProvenance;

/// Soil classification type.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum SoilType {
    DenseGravellySand,
    MediumDenseSand,
    LooseSand,
    StiffClayUndrained,
    FirmSiltyClay,
    SoftClay,
}

impl SoilType {
    /// Nominal soil effective cohesion c' (kPa).
    pub fn default_cohesion_kpa(&self) -> f64 {
        match self {
            Self::DenseGravellySand | Self::MediumDenseSand | Self::LooseSand => 0.0,
            Self::StiffClayUndrained => 90.0,
            Self::FirmSiltyClay => 35.0,
            Self::SoftClay => 15.0,
        }
    }

    /// Nominal internal friction angle phi' (degrees).
    pub fn default_friction_angle_deg(&self) -> f64 {
        match self {
            Self::DenseGravellySand => 38.0,
            Self::MediumDenseSand => 32.0,
            Self::LooseSand => 28.0,
            Self::StiffClayUndrained => 0.0,
            Self::FirmSiltyClay => 22.0,
            Self::SoftClay => 0.0,
        }
    }

    /// Effective unit weight gamma (kN/m³).
    pub fn default_unit_weight_kn_m3(&self) -> f64 {
        match self {
            Self::DenseGravellySand => 20.0,
            Self::MediumDenseSand => 18.5,
            Self::LooseSand => 17.0,
            Self::StiffClayUndrained => 19.5,
            Self::FirmSiltyClay => 18.0,
            Self::SoftClay => 16.5,
        }
    }

    /// Elastic modulus E_s (MPa).
    pub fn default_elastic_modulus_mpa(&self) -> f64 {
        match self {
            Self::DenseGravellySand => 60.0,
            Self::MediumDenseSand => 30.0,
            Self::LooseSand => 15.0,
            Self::StiffClayUndrained => 40.0,
            Self::FirmSiltyClay => 20.0,
            Self::SoftClay => 8.0,
        }
    }
}

/// Geotechnical bearing problem specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeotechnicalBearingProblem {
    /// Problem identifier.
    pub problem_id: String,
    /// Soil classification.
    pub soil_type: SoilType,
    /// Footing width B (m).
    pub footing_width_b_m: f64,
    /// Footing length L (m).
    pub footing_length_l_m: f64,
    /// Embedment depth D_f (m).
    pub embedment_depth_df_m: f64,
    /// Custom soil cohesion c' (kPa, optional, overrides default).
    pub soil_cohesion_kpa: Option<f64>,
    /// Custom friction angle phi' (deg, optional, overrides default).
    pub friction_angle_deg: Option<f64>,
    /// Custom soil unit weight gamma (kN/m³, optional, overrides default).
    pub unit_weight_kn_m3: Option<f64>,
    /// Custom soil Young's modulus E_s (MPa, optional, overrides default).
    pub elastic_modulus_mpa: Option<f64>,
    /// Soil Poisson's ratio nu.
    pub poissons_ratio: f64,
    /// Applied vertical design column load P (kN).
    pub applied_load_kn: f64,
    /// Target factor of safety against shear failure (typically 3.0).
    pub target_factor_of_safety: f64,
    /// Maximum allowable immediate settlement (mm, e.g. 25.0 mm).
    pub max_allowable_settlement_mm: f64,
}

impl Default for GeotechnicalBearingProblem {
    fn default() -> Self {
        Self {
            problem_id: "building_column_pad_footing".to_string(),
            soil_type: SoilType::MediumDenseSand,
            footing_width_b_m: 2.20,
            footing_length_l_m: 2.20, // Square pad
            embedment_depth_df_m: 1.50,
            soil_cohesion_kpa: None,
            friction_angle_deg: Some(32.0),
            unit_weight_kn_m3: Some(18.5),
            elastic_modulus_mpa: Some(30.0),
            poissons_ratio: 0.30,
            applied_load_kn: 1250.0,
            target_factor_of_safety: 3.0,
            max_allowable_settlement_mm: 25.0,
        }
    }
}

/// Comprehensive bearing capacity and settlement results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeotechnicalBearingResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Overburden surcharge pressure q_0 = gamma * D_f (kPa).
    pub surcharge_pressure_q0_kpa: f64,
    /// Bearing factor N_c.
    pub bearing_factor_nc: f64,
    /// Bearing factor N_q.
    pub bearing_factor_nq: f64,
    /// Bearing factor N_gamma.
    pub bearing_factor_ngamma: f64,
    /// Shape factor s_c.
    pub shape_factor_sc: f64,
    /// Shape factor s_q.
    pub shape_factor_sq: f64,
    /// Shape factor s_gamma.
    pub shape_factor_sgamma: f64,
    /// Depth factor d_c.
    pub depth_factor_dc: f64,
    /// Depth factor d_q.
    pub depth_factor_dq: f64,
    /// Depth factor d_gamma.
    pub depth_factor_dgamma: f64,
    /// Ultimate gross bearing capacity q_ult (kPa).
    pub ultimate_bearing_capacity_qult_kpa: f64,
    /// Net ultimate bearing capacity q_net_ult = q_ult - q_0 (kPa).
    pub net_ultimate_bearing_capacity_kpa: f64,
    /// Allowable bearing capacity q_all (kPa).
    pub allowable_bearing_capacity_qall_kpa: f64,
    /// Applied footing contact pressure q_app = P / (B * L) (kPa).
    pub applied_contact_pressure_kpa: f64,
    /// Actual factor of safety against shear failure.
    pub actual_factor_of_safety: f64,
    /// Bearing capacity check pass/fail.
    pub bearing_capacity_adequate: bool,
    /// Estimated immediate elastic settlement S_e (mm).
    pub estimated_settlement_mm: f64,
    /// Settlement check pass/fail (S_e <= S_max).
    pub settlement_acceptable: bool,
}

/// Solves shallow footing ultimate bearing capacity and elastic settlement.
pub fn solve_geotechnical_bearing(prob: &GeotechnicalBearingProblem) -> GeotechnicalBearingResult {
    let b = prob.footing_width_b_m.max(0.2);
    let l = prob.footing_length_l_m.max(b);
    let d_f = prob.embedment_depth_df_m.max(0.0);

    let c = prob
        .soil_cohesion_kpa
        .unwrap_or_else(|| prob.soil_type.default_cohesion_kpa());
    let phi_deg = prob
        .friction_angle_deg
        .unwrap_or_else(|| prob.soil_type.default_friction_angle_deg())
        .clamp(0.0, 48.0);
    let gamma = prob
        .unit_weight_kn_m3
        .unwrap_or_else(|| prob.soil_type.default_unit_weight_kn_m3())
        .max(5.0);
    let e_s_mpa = prob
        .elastic_modulus_mpa
        .unwrap_or_else(|| prob.soil_type.default_elastic_modulus_mpa())
        .max(1.0);
    let nu = prob.poissons_ratio.clamp(0.10, 0.49);

    let phi_rad = phi_deg.to_radians();
    let q_0 = gamma * d_f;

    // Meyerhof classical bearing capacity factors:
    let (n_q, n_c, n_gamma) = if phi_deg < 0.1 {
        // Purely cohesive undrained clay
        (1.0, 5.14, 0.0)
    } else {
        let nq = (std::f64::consts::PI * phi_rad.tan()).exp()
            * (std::f64::consts::FRAC_PI_4 + phi_rad / 2.0).tan().powi(2);
        let nc = (nq - 1.0) / phi_rad.tan();
        let ngamma = 2.0 * (nq + 1.0) * phi_rad.tan();
        (nq, nc, ngamma)
    };

    // Shape factors:
    let kp = (std::f64::consts::FRAC_PI_4 + phi_rad / 2.0).tan().powi(2);
    let s_c = 1.0 + 0.2 * (b / l) * kp;
    let s_q = if phi_deg > 10.0 {
        1.0 + 0.1 * (b / l) * kp
    } else {
        1.0
    };
    let s_gamma = s_q;

    // Depth factors:
    let d_c = 1.0 + 0.2 * (d_f / b).min(1.0) * kp.sqrt();
    let d_q = if phi_deg > 10.0 {
        1.0 + 0.1 * (d_f / b).min(1.0) * kp.sqrt()
    } else {
        1.0
    };
    let d_gamma = d_q;

    // General Bearing Capacity Equation:
    // q_ult = c * Nc * sc * dc + q0 * Nq * sq * dq + 0.5 * gamma * B * N_gamma * s_gamma * d_gamma
    let term_c = c * n_c * s_c * d_c;
    let term_q = q_0 * n_q * s_q * d_q;
    let term_gamma = 0.5 * gamma * b * n_gamma * s_gamma * d_gamma;

    let q_ult = term_c + term_q + term_gamma;
    let q_net_ult = (q_ult - q_0).max(1.0);

    let fs_target = prob.target_factor_of_safety.max(1.5);
    let q_all = (q_net_ult / fs_target) + q_0;

    // Applied footing pressure:
    let area = b * l;
    let q_app = prob.applied_load_kn / area;

    let actual_fs = if (q_app - q_0) > 1e-3 {
        q_net_ult / (q_app - q_0)
    } else {
        100.0
    };

    let bearing_capacity_adequate = q_app <= q_all;

    // Immediate Elastic Settlement:
    // S_e = q_net * B * ((1 - nu^2) / E_s) * I_s
    // Influence factor I_s for rectangular center: ~1.12 for square, ~1.30 for 2:1
    let shape_ratio = l / b;
    let i_s = 1.0 + 0.12 * shape_ratio.ln().max(0.0);
    let q_net_app = (q_app - q_0).max(0.0); // kPa
    // E_s in MPa = E_s * 1000 kPa
    let s_e_m = q_net_app * b * ((1.0 - nu.powi(2)) / (e_s_mpa * 1000.0)) * i_s;
    let s_e_mm = s_e_m * 1000.0;

    let settlement_acceptable = s_e_mm <= prob.max_allowable_settlement_mm;

    GeotechnicalBearingResult {
        problem_id: prob.problem_id.clone(),
        surcharge_pressure_q0_kpa: q_0,
        bearing_factor_nc: n_c,
        bearing_factor_nq: n_q,
        bearing_factor_ngamma: n_gamma,
        shape_factor_sc: s_c,
        shape_factor_sq: s_q,
        shape_factor_sgamma: s_gamma,
        depth_factor_dc: d_c,
        depth_factor_dq: d_q,
        depth_factor_dgamma: d_gamma,
        ultimate_bearing_capacity_qult_kpa: q_ult,
        net_ultimate_bearing_capacity_kpa: q_net_ult,
        allowable_bearing_capacity_qall_kpa: q_all,
        applied_contact_pressure_kpa: q_app,
        actual_factor_of_safety: actual_fs,
        bearing_capacity_adequate,
        estimated_settlement_mm: s_e_mm,
        settlement_acceptable,
    }
}

/// Simulation runner adapter for unified verification.
pub fn solve_geotechnical_bearing_sim(prob: &GeotechnicalBearingProblem) -> SimulationResult {
    let sol = solve_geotechnical_bearing(prob);
    let mut metrics = std::collections::HashMap::new();
    metrics.insert(
        "ultimate_bearing_capacity_qult_kpa".to_string(),
        sol.ultimate_bearing_capacity_qult_kpa,
    );
    metrics.insert(
        "allowable_bearing_capacity_qall_kpa".to_string(),
        sol.allowable_bearing_capacity_qall_kpa,
    );
    metrics.insert(
        "actual_factor_of_safety".to_string(),
        sol.actual_factor_of_safety,
    );
    metrics.insert(
        "estimated_settlement_mm".to_string(),
        sol.estimated_settlement_mm,
    );

    let mut conv = ConvergenceMonitor::new(1e-6);
    conv.record(1, sol.actual_factor_of_safety, 1, 1e-4);

    SimulationResult {
        problem_id: prob.problem_id.clone(),
        engine_id: "atlas.civil.geotechnical_bearing".to_string(),
        status: SimulationStatus::Converged,
        execution_time_ms: 1,
        scalar_metrics: metrics,
        convergence: conv,
        validation: None,
        provenance: SimulationProvenance::new("atlas.civil.geotechnical_bearing", "0.1.0"),
        logs: vec![format!(
            "q_ult: {:.1} kPa, q_all: {:.1} kPa, Actual FoS: {:.2}, Settlement: {:.1} mm",
            sol.ultimate_bearing_capacity_qult_kpa,
            sol.allowable_bearing_capacity_qall_kpa,
            sol.actual_factor_of_safety,
            sol.estimated_settlement_mm
        )],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_building_pad_footing_bearing_capacity() {
        let prob = GeotechnicalBearingProblem::default();
        let sol = solve_geotechnical_bearing(&prob);

        // N_q for 32° is ~23.2, N_c is ~35.5, N_gamma is ~30.2
        assert!(sol.bearing_factor_nq > 20.0 && sol.bearing_factor_nq < 26.0);
        assert!(sol.bearing_factor_nc > 30.0 && sol.bearing_factor_nc < 40.0);
        assert!(sol.bearing_factor_ngamma > 25.0 && sol.bearing_factor_ngamma < 35.0);
        // Surcharge q0 = 18.5 * 1.5 = 27.75 kPa
        assert!((sol.surcharge_pressure_q0_kpa - 27.75).abs() < 1e-2);
        // Ultimate bearing capacity is ~1500 to 2500 kPa
        assert!(
            sol.ultimate_bearing_capacity_qult_kpa > 1500.0
                && sol.ultimate_bearing_capacity_qult_kpa < 3000.0
        );
        // Allowable bearing capacity with FS=3 is ~500 to 900 kPa
        assert!(
            sol.allowable_bearing_capacity_qall_kpa > 500.0
                && sol.allowable_bearing_capacity_qall_kpa < 1000.0
        );
        // Applied contact pressure 1250 kN / (2.2 * 2.2) = 258.3 kPa
        assert!((sol.applied_contact_pressure_kpa - 258.26).abs() < 1.0);
        assert!(sol.actual_factor_of_safety > 3.0);
        assert!(sol.bearing_capacity_adequate);
        // Settlement under ~230 kPa net on 30 MPa sand is ~10 to 18 mm (< 25 mm)
        assert!(sol.estimated_settlement_mm > 5.0 && sol.estimated_settlement_mm < 20.0);
        assert!(sol.settlement_acceptable);
    }
}
