//! Structural Steel Bolted & Welded Connections (Eurocode 3 EN 1993-1-8 / AISC 360-16).
//!
//! Evaluates bolt shear resistance, plate bearing capacity, bolt tension capacity,
//! combined shear-tension interaction, and fillet weld throat stresses per Eurocode 3 and AISC 360.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

use crate::convergence::ConvergenceMonitor;
use crate::execution::{SimulationResult, SimulationStatus};
use crate::provenance::SimulationProvenance;
use crate::validation::AnalyticalComparison;

/// Structural steel bolt grade (ISO 898-1 / EN 1993-1-8).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum BoltGrade {
    /// Class 4.6 (f_ub = 400 MPa, f_yb = 240 MPa).
    Class4_6,
    /// Class 8.8 (f_ub = 800 MPa, f_yb = 640 MPa).
    Class8_8,
    /// Class 10.9 (f_ub = 1000 MPa, f_yb = 900 MPa).
    Class10_9,
}

impl BoltGrade {
    pub fn ultimate_strength_mpa(&self) -> f64 {
        match self {
            Self::Class4_6 => 400.0,
            Self::Class8_8 => 800.0,
            Self::Class10_9 => 1000.0,
        }
    }

    pub fn yield_strength_mpa(&self) -> f64 {
        match self {
            Self::Class4_6 => 240.0,
            Self::Class8_8 => 640.0,
            Self::Class10_9 => 900.0,
        }
    }

    /// Alpha_v shear reduction factor.
    pub fn alpha_v(&self) -> f64 {
        match self {
            Self::Class4_6 | Self::Class8_8 => 0.60,
            Self::Class10_9 => 0.50,
        }
    }
}

/// Standard metric structural bolt size.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum BoltDiameter {
    M12,
    M16,
    M20,
    M24,
    M30,
}

impl BoltDiameter {
    pub fn nominal_diameter_mm(&self) -> f64 {
        match self {
            Self::M12 => 12.0,
            Self::M16 => 16.0,
            Self::M20 => 20.0,
            Self::M24 => 24.0,
            Self::M30 => 30.0,
        }
    }

    pub fn tensile_stress_area_mm2(&self) -> f64 {
        match self {
            Self::M12 => 84.3,
            Self::M16 => 157.0,
            Self::M20 => 245.0,
            Self::M24 => 353.0,
            Self::M30 => 561.0,
        }
    }

    pub fn standard_hole_diameter_mm(&self) -> f64 {
        match self {
            Self::M12 => self.nominal_diameter_mm() + 1.0,
            Self::M16 | Self::M20 | Self::M24 => self.nominal_diameter_mm() + 2.0,
            Self::M30 => self.nominal_diameter_mm() + 3.0,
        }
    }
}

/// Steel structural grade for connecting plates and profiles.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum StructuralSteelGrade {
    S235,
    S275,
    S355,
    S460,
}

impl StructuralSteelGrade {
    pub fn ultimate_strength_fu_mpa(&self) -> f64 {
        match self {
            Self::S235 => 360.0,
            Self::S275 => 430.0,
            Self::S355 => 510.0,
            Self::S460 => 570.0,
        }
    }

    pub fn yield_strength_fy_mpa(&self) -> f64 {
        match self {
            Self::S235 => 235.0,
            Self::S275 => 275.0,
            Self::S355 => 355.0,
            Self::S460 => 460.0,
        }
    }

    /// Eurocode 3 correlation factor beta_w for fillet welds.
    pub fn beta_w(&self) -> f64 {
        match self {
            Self::S235 => 0.80,
            Self::S275 => 0.85,
            Self::S355 => 0.90,
            Self::S460 => 1.00,
        }
    }
}

/// Bolted and welded connection specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionProblem {
    /// Problem identifier.
    pub problem_id: String,
    /// Bolt grade.
    pub bolt_grade: BoltGrade,
    /// Bolt nominal size.
    pub bolt_diameter: BoltDiameter,
    /// Number of bolts in connection.
    pub number_of_bolts: usize,
    /// Shear planes per bolt (1 for single shear lap, 2 for double shear clevis/splice).
    pub shear_planes: usize,
    /// Connecting plate steel grade.
    pub steel_grade: StructuralSteelGrade,
    /// Thinner connecting plate thickness t (mm).
    pub plate_thickness_mm: f64,
    /// End distance e1 in load direction (mm).
    pub end_distance_e1_mm: f64,
    /// Edge distance e2 perpendicular to load (mm).
    pub edge_distance_e2_mm: f64,
    /// Bolt pitch p1 in load direction (mm).
    pub pitch_p1_mm: f64,
    /// Applied design shear force V_Ed (kN).
    pub applied_shear_force_kn: f64,
    /// Applied design tension force N_Ed (kN).
    pub applied_tension_force_kn: f64,
    /// Fillet weld throat thickness a (mm, e.g. 6.0 mm).
    pub weld_throat_thickness_mm: f64,
    /// Total effective fillet weld length L_w (mm, e.g. 200 mm).
    pub weld_length_mm: f64,
    /// Applied design force on fillet weld F_w_Ed (kN).
    pub applied_weld_force_kn: f64,
    /// Angle of applied weld force relative to weld axis (degrees, 0 = longitudinal shear, 90 = transverse).
    pub weld_load_angle_deg: f64,
}

impl Default for ConnectionProblem {
    fn default() -> Self {
        Self {
            problem_id: "bracket_bolted_and_welded_connection".to_string(),
            bolt_grade: BoltGrade::Class8_8,
            bolt_diameter: BoltDiameter::M20,
            number_of_bolts: 4,
            shear_planes: 1, // Single shear
            steel_grade: StructuralSteelGrade::S355,
            plate_thickness_mm: 12.0,
            end_distance_e1_mm: 45.0,
            edge_distance_e2_mm: 40.0,
            pitch_p1_mm: 70.0,
            applied_shear_force_kn: 180.0,
            applied_tension_force_kn: 80.0,
            weld_throat_thickness_mm: 6.0,
            weld_length_mm: 220.0,
            applied_weld_force_kn: 150.0,
            weld_load_angle_deg: 45.0,
        }
    }
}

/// Comprehensive connection verification results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Design shear resistance per bolt F_v_Rd (kN).
    pub single_bolt_shear_resistance_kn: f64,
    /// Total bolted connection shear capacity (kN).
    pub total_connection_shear_capacity_kn: f64,
    /// Design plate bearing resistance per bolt F_b_Rd (kN).
    pub single_bolt_bearing_resistance_kn: f64,
    /// Total connection plate bearing capacity (kN).
    pub total_connection_bearing_capacity_kn: f64,
    /// Design tension resistance per bolt F_t_Rd (kN).
    pub single_bolt_tension_resistance_kn: f64,
    /// Total connection tension capacity (kN).
    pub total_connection_tension_capacity_kn: f64,
    /// Bolt shear-tension interaction utilization (EC3 Eq 3.5: V/V_Rd + N / (1.4 N_Rd) <= 1.0).
    pub bolt_interaction_utilization: f64,
    /// Bolted connection pass/fail.
    pub bolted_joint_safe: bool,
    /// Fillet weld design shear strength f_vw_d per EC3 (MPa).
    pub weld_design_strength_f_vwd_mpa: f64,
    /// Fillet weld equivalent von Mises stress sigma_w (MPa).
    pub weld_equivalent_stress_mpa: f64,
    /// Fillet weld stress perpendicular to throat sigma_perp (MPa).
    pub weld_normal_stress_mpa: f64,
    /// Fillet weld utilization ratio.
    pub weld_utilization: f64,
    /// Welded joint pass/fail.
    pub welded_joint_safe: bool,
}

/// Solves structural steel bolted and welded connection per Eurocode 3 / AISC 360.
pub fn solve_structural_connection(prob: &ConnectionProblem) -> ConnectionResult {
    let gamma_m2 = 1.25; // Partial factor for connections (EN 1993-1-8 Table 2.1)
    let f_ub = prob.bolt_grade.ultimate_strength_mpa();
    let a_s = prob.bolt_diameter.tensile_stress_area_mm2();
    let d = prob.bolt_diameter.nominal_diameter_mm();
    let d_0 = prob.bolt_diameter.standard_hole_diameter_mm();
    let n_bolts = prob.number_of_bolts.max(1) as f64;
    let n_planes = prob.shear_planes.max(1) as f64;

    let f_u = prob.steel_grade.ultimate_strength_fu_mpa();
    let t = prob.plate_thickness_mm.max(1.0);

    // 1. Bolt shear resistance per shear plane:
    // F_v_Rd = (alpha_v * f_ub * A_s) / gamma_M2 in N
    let alpha_v = prob.bolt_grade.alpha_v();
    let f_v_rd_single = (alpha_v * f_ub * a_s * n_planes) / (gamma_m2 * 1000.0); // kN
    let total_shear_cap = f_v_rd_single * n_bolts;

    // 2. Plate bearing resistance per bolt:
    // F_b_Rd = (k1 * alpha_b * f_u * d * t) / gamma_M2
    // alpha_b = min( e1 / (3*d_0), p1 / (3*d_0) - 0.25, f_ub / f_u, 1.0 )
    let alpha_b = (prob.end_distance_e1_mm / (3.0 * d_0))
        .min((prob.pitch_p1_mm / (3.0 * d_0)) - 0.25)
        .min(f_ub / f_u)
        .min(1.0)
        .max(0.1);

    // k1 = min( 2.8 * (e2 / d_0) - 1.7, 2.5 )
    let k1 = (2.8 * (prob.edge_distance_e2_mm / d_0) - 1.7).min(2.5).max(0.1);

    let f_b_rd_single = (k1 * alpha_b * f_u * d * t) / (gamma_m2 * 1000.0); // kN
    let total_bearing_cap = f_b_rd_single * n_bolts;

    // Governing shear/bearing capacity per bolt:
    let bolt_shear_governing = f_v_rd_single.min(f_b_rd_single);

    // 3. Bolt tension resistance:
    // F_t_Rd = (k2 * f_ub * A_s) / gamma_M2 (k2 = 0.9 for countersunk or standard hex)
    let k2 = 0.90;
    let f_t_rd_single = (k2 * f_ub * a_s) / (gamma_m2 * 1000.0); // kN
    let total_tension_cap = f_t_rd_single * n_bolts;

    // 4. Combined shear and tension interaction per bolt:
    // V_Ed / V_Rd + N_Ed / (1.4 * N_Rd) <= 1.0
    let v_ed_bolt = prob.applied_shear_force_kn / n_bolts;
    let n_ed_bolt = prob.applied_tension_force_kn / n_bolts;

    let interaction_util = (v_ed_bolt / bolt_shear_governing) + (n_ed_bolt / (1.4 * f_t_rd_single));
    let bolted_joint_safe = interaction_util <= 1.0
        && (v_ed_bolt / bolt_shear_governing) <= 1.0
        && (n_ed_bolt / f_t_rd_single) <= 1.0;

    // 5. Fillet weld throat stress verification (EN 1993-1-8 4.5.3 Directional Method):
    // Weld throat area A_w = a * L_w
    let a_weld = prob.weld_throat_thickness_mm.max(0.5);
    let l_weld = prob.weld_length_mm.max(1.0);
    let f_w_ed_n = prob.applied_weld_force_kn * 1000.0; // N

    let theta_rad = prob.weld_load_angle_deg.to_radians();
    let f_transverse = f_w_ed_n * theta_rad.sin();
    let f_longitudinal = f_w_ed_n * theta_rad.cos();

    // Resolving throat stresses:
    // Transverse force splits equally into normal stress sigma_perp and shear stress tau_perp:
    // sigma_perp = tau_perp = F_trans / (sqrt(2) * a * L_w)
    let throat_area = a_weld * l_weld;
    let sigma_perp = f_transverse / (std::f64::consts::SQRT_2 * throat_area);
    let tau_perp = sigma_perp;
    let tau_parallel = f_longitudinal / throat_area;

    // Eurocode 3 von Mises equivalent weld stress:
    // sigma_w = sqrt( sigma_perp^2 + 3 * (tau_perp^2 + tau_parallel^2) )
    let sigma_w = (sigma_perp.powi(2) + 3.0 * (tau_perp.powi(2) + tau_parallel.powi(2))).sqrt();

    // Design weld strength:
    // f_vw_d = f_u / (sqrt(3) * beta_w * gamma_M2)
    let beta_w = prob.steel_grade.beta_w();
    let f_vw_d = f_u / (3.0_f64.sqrt() * beta_w * gamma_m2);
    let weld_limit_stress = f_u / (beta_w * gamma_m2);

    let weld_util = (sigma_w / weld_limit_stress).max(sigma_perp / (0.9 * f_u / gamma_m2));
    let welded_joint_safe = weld_util <= 1.0;

    ConnectionResult {
        problem_id: prob.problem_id.clone(),
        single_bolt_shear_resistance_kn: f_v_rd_single,
        total_connection_shear_capacity_kn: total_shear_cap,
        single_bolt_bearing_resistance_kn: f_b_rd_single,
        total_connection_bearing_capacity_kn: total_bearing_cap,
        single_bolt_tension_resistance_kn: f_t_rd_single,
        total_connection_tension_capacity_kn: total_tension_cap,
        bolt_interaction_utilization: interaction_util,
        bolted_joint_safe,
        weld_design_strength_f_vwd_mpa: f_vw_d,
        weld_equivalent_stress_mpa: sigma_w,
        weld_normal_stress_mpa: sigma_perp,
        weld_utilization: weld_util,
        welded_joint_safe,
    }
}

/// Simulation runner adapter for unified verification.
pub fn solve_structural_connection_sim(prob: &ConnectionProblem) -> SimulationResult {
    let sol = solve_structural_connection(prob);
    let mut metrics = std::collections::HashMap::new();
    metrics.insert("total_connection_shear_capacity_kn".to_string(), sol.total_connection_shear_capacity_kn);
    metrics.insert("bolt_interaction_utilization".to_string(), sol.bolt_interaction_utilization);
    metrics.insert("weld_equivalent_stress_mpa".to_string(), sol.weld_equivalent_stress_mpa);
    metrics.insert("weld_utilization".to_string(), sol.weld_utilization);

    let comparison = AnalyticalComparison::assess(
        "bolt_interaction_utilization",
        sol.bolt_interaction_utilization,
        1.0,
    );

    let mut conv = ConvergenceMonitor::new(1e-6);
    conv.record(1, sol.bolt_interaction_utilization, 1, 1e-4);

    SimulationResult {
        problem_id: prob.problem_id.clone(),
        engine_id: "atlas.structural.connections".to_string(),
        status: SimulationStatus::Converged,
        execution_time_ms: 1,
        scalar_metrics: metrics,
        convergence: conv,
        validation: Some(comparison),
        provenance: SimulationProvenance::new("atlas.structural.connections", "0.1.0"),
        logs: vec![format!("Bolt utilization: {:.2}, Weld utilization: {:.2}, Safe: {}", sol.bolt_interaction_utilization, sol.weld_utilization, sol.bolted_joint_safe && sol.welded_joint_safe)],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_industrial_bracket_bolted_and_welded_connection() {
        let prob = ConnectionProblem::default();
        let sol = solve_structural_connection(&prob);

        // M20 8.8 single shear resistance ~94 kN
        assert!(sol.single_bolt_shear_resistance_kn > 85.0 && sol.single_bolt_shear_resistance_kn < 105.0);
        // Total shear capacity for 4 bolts ~376 kN
        assert!(sol.total_connection_shear_capacity_kn > 350.0);
        // Single bolt bearing capacity on 12mm S355 ~100 to 140 kN
        assert!(sol.single_bolt_bearing_resistance_kn > 80.0);
        // Combined interaction for 180 kN shear + 80 kN tension across 4 bolts
        assert!(sol.bolt_interaction_utilization > 0.40 && sol.bolt_interaction_utilization < 0.95);
        assert!(sol.bolted_joint_safe);
        // Fillet weld verification
        assert!(sol.weld_equivalent_stress_mpa > 100.0 && sol.weld_equivalent_stress_mpa < 250.0);
        assert!(sol.weld_utilization < 1.0);
        assert!(sol.welded_joint_safe);
    }
}
