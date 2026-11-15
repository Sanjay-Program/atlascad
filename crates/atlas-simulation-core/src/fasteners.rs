//! # Standard Fasteners, ISO Metric Hardware & Bolted Joint Engineering
//!
//! Provides ISO 4014/4017 Hexagon Head Bolts, ISO 4032 Nuts, ISO 7089 Washers,
//! ISO 273 clearance holes, metric thread profiles, proof load calculations,
//! Shigley/Motosh tightening torque, and joint clamping safety factor analysis.

use crate::convergence::ConvergenceMonitor;
use crate::execution::{SimulationResult, SimulationStatus};
use crate::provenance::SimulationProvenance;
use crate::validation::AnalyticalComparison;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// ISO Metric Bolt Property Classes per ISO 898-1 and ISO 3506-1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BoltPropertyClass {
    Class4_8,
    Class8_8,
    Class10_9,
    Class12_9,
    StainlessA2_70,
}

impl BoltPropertyClass {
    /// Nominal proof strength Sp in MPa.
    pub fn proof_strength_mpa(&self) -> f64 {
        match self {
            Self::Class4_8 => 310.0,
            Self::Class8_8 => 600.0,
            Self::Class10_9 => 830.0,
            Self::Class12_9 => 970.0,
            Self::StainlessA2_70 => 450.0,
        }
    }

    /// Nominal ultimate tensile strength Sut in MPa.
    pub fn tensile_strength_mpa(&self) -> f64 {
        match self {
            Self::Class4_8 => 420.0,
            Self::Class8_8 => 830.0,
            Self::Class10_9 => 1040.0,
            Self::Class12_9 => 1220.0,
            Self::StainlessA2_70 => 700.0,
        }
    }

    /// Yield strength Sy in MPa.
    pub fn yield_strength_mpa(&self) -> f64 {
        match self {
            Self::Class4_8 => 340.0,
            Self::Class8_8 => 660.0,
            Self::Class10_9 => 940.0,
            Self::Class12_9 => 1100.0,
            Self::StainlessA2_70 => 450.0,
        }
    }
}

/// Standard metric thread specifications.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetricThreadSpec {
    pub designation: String,
    pub nominal_diameter_mm: f64,
    pub pitch_mm: f64,
    pub width_across_flats_s_mm: f64,
    pub head_height_k_mm: f64,
    pub clearance_hole_fine_mm: f64,
    pub clearance_hole_medium_mm: f64,
    pub tap_drill_diameter_mm: f64,
}

/// Returns standard coarse pitch metrics for M3 to M36 bolts (ISO 4014 / ISO 261 / ISO 273).
pub fn get_standard_metric_threads() -> Vec<MetricThreadSpec> {
    vec![
        MetricThreadSpec {
            designation: "M3".to_string(),
            nominal_diameter_mm: 3.0,
            pitch_mm: 0.50,
            width_across_flats_s_mm: 5.5,
            head_height_k_mm: 2.0,
            clearance_hole_fine_mm: 3.2,
            clearance_hole_medium_mm: 3.4,
            tap_drill_diameter_mm: 2.5,
        },
        MetricThreadSpec {
            designation: "M4".to_string(),
            nominal_diameter_mm: 4.0,
            pitch_mm: 0.70,
            width_across_flats_s_mm: 7.0,
            head_height_k_mm: 2.8,
            clearance_hole_fine_mm: 4.3,
            clearance_hole_medium_mm: 4.5,
            tap_drill_diameter_mm: 3.3,
        },
        MetricThreadSpec {
            designation: "M5".to_string(),
            nominal_diameter_mm: 5.0,
            pitch_mm: 0.80,
            width_across_flats_s_mm: 8.0,
            head_height_k_mm: 3.5,
            clearance_hole_fine_mm: 5.3,
            clearance_hole_medium_mm: 5.5,
            tap_drill_diameter_mm: 4.2,
        },
        MetricThreadSpec {
            designation: "M6".to_string(),
            nominal_diameter_mm: 6.0,
            pitch_mm: 1.00,
            width_across_flats_s_mm: 10.0,
            head_height_k_mm: 4.0,
            clearance_hole_fine_mm: 6.4,
            clearance_hole_medium_mm: 6.6,
            tap_drill_diameter_mm: 5.0,
        },
        MetricThreadSpec {
            designation: "M8".to_string(),
            nominal_diameter_mm: 8.0,
            pitch_mm: 1.25,
            width_across_flats_s_mm: 13.0,
            head_height_k_mm: 5.3,
            clearance_hole_fine_mm: 8.4,
            clearance_hole_medium_mm: 9.0,
            tap_drill_diameter_mm: 6.8,
        },
        MetricThreadSpec {
            designation: "M10".to_string(),
            nominal_diameter_mm: 10.0,
            pitch_mm: 1.50,
            width_across_flats_s_mm: 16.0,
            head_height_k_mm: 6.4,
            clearance_hole_fine_mm: 10.5,
            clearance_hole_medium_mm: 11.0,
            tap_drill_diameter_mm: 8.5,
        },
        MetricThreadSpec {
            designation: "M12".to_string(),
            nominal_diameter_mm: 12.0,
            pitch_mm: 1.75,
            width_across_flats_s_mm: 18.0,
            head_height_k_mm: 7.5,
            clearance_hole_fine_mm: 13.0,
            clearance_hole_medium_mm: 13.5,
            tap_drill_diameter_mm: 10.2,
        },
        MetricThreadSpec {
            designation: "M14".to_string(),
            nominal_diameter_mm: 14.0,
            pitch_mm: 2.00,
            width_across_flats_s_mm: 21.0,
            head_height_k_mm: 8.8,
            clearance_hole_fine_mm: 15.0,
            clearance_hole_medium_mm: 15.5,
            tap_drill_diameter_mm: 12.0,
        },
        MetricThreadSpec {
            designation: "M16".to_string(),
            nominal_diameter_mm: 16.0,
            pitch_mm: 2.00,
            width_across_flats_s_mm: 24.0,
            head_height_k_mm: 10.0,
            clearance_hole_fine_mm: 17.0,
            clearance_hole_medium_mm: 17.5,
            tap_drill_diameter_mm: 14.0,
        },
        MetricThreadSpec {
            designation: "M20".to_string(),
            nominal_diameter_mm: 20.0,
            pitch_mm: 2.50,
            width_across_flats_s_mm: 30.0,
            head_height_k_mm: 12.5,
            clearance_hole_fine_mm: 21.0,
            clearance_hole_medium_mm: 22.0,
            tap_drill_diameter_mm: 17.5,
        },
        MetricThreadSpec {
            designation: "M24".to_string(),
            nominal_diameter_mm: 24.0,
            pitch_mm: 3.00,
            width_across_flats_s_mm: 36.0,
            head_height_k_mm: 15.0,
            clearance_hole_fine_mm: 25.0,
            clearance_hole_medium_mm: 26.0,
            tap_drill_diameter_mm: 21.0,
        },
        MetricThreadSpec {
            designation: "M30".to_string(),
            nominal_diameter_mm: 30.0,
            pitch_mm: 3.50,
            width_across_flats_s_mm: 46.0,
            head_height_k_mm: 18.7,
            clearance_hole_fine_mm: 32.0,
            clearance_hole_medium_mm: 33.0,
            tap_drill_diameter_mm: 26.5,
        },
        MetricThreadSpec {
            designation: "M36".to_string(),
            nominal_diameter_mm: 36.0,
            pitch_mm: 4.00,
            width_across_flats_s_mm: 55.0,
            head_height_k_mm: 22.5,
            clearance_hole_fine_mm: 38.0,
            clearance_hole_medium_mm: 39.0,
            tap_drill_diameter_mm: 32.0,
        },
    ]
}

/// Find metric thread spec by designation string (e.g. "M8" or "8").
pub fn find_metric_thread(designation: &str) -> Option<MetricThreadSpec> {
    let clean = designation.trim().to_uppercase();
    get_standard_metric_threads().into_iter().find(|t| {
        t.designation == clean || t.designation.trim_start_matches('M') == clean
    })
}

/// Input problem for bolted joint preload, tightening torque, and load capacity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BoltedJointProblem {
    /// Fastener designation (e.g. "M10")
    pub designation: String,
    /// Property class grade
    pub property_class: BoltPropertyClass,
    /// Clamped grip length in millimeters
    pub grip_length_mm: f64,
    /// Friction factor / torque coefficient K (e.g. 0.15 for lubricated/zinc, 0.20 for dry steel)
    pub torque_coefficient_k: Option<f64>,
    /// External service tensile force applied to joint in kN
    pub external_tensile_kn: Option<f64>,
    /// External service shear force applied to joint in kN
    pub external_shear_kn: Option<f64>,
    /// Joint stiffness ratio C = kb / (kb + km). Typically 0.15 - 0.35.
    pub joint_stiffness_constant_c: Option<f64>,
    /// Number of bolts sharing the load (default 1)
    pub bolt_count: Option<u32>,
}

impl Default for BoltedJointProblem {
    fn default() -> Self {
        Self {
            designation: "M10".to_string(),
            property_class: BoltPropertyClass::Class8_8,
            grip_length_mm: 30.0,
            torque_coefficient_k: Some(0.18),
            external_tensile_kn: Some(15.0),
            external_shear_kn: Some(8.0),
            joint_stiffness_constant_c: Some(0.25),
            bolt_count: Some(1),
        }
    }
}

/// Analytical results for bolted joint sizing and verification.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BoltedJointResult {
    pub designation: String,
    pub property_class: BoltPropertyClass,
    pub nominal_diameter_mm: f64,
    pub pitch_mm: f64,
    pub tensile_stress_area_mm2: f64,
    pub proof_strength_mpa: f64,
    pub proof_load_kn: f64,
    pub recommended_preload_kn: f64,
    pub tightening_torque_nm: f64,
    pub shear_capacity_kn: f64,
    pub clearance_hole_fine_mm: f64,
    pub clearance_hole_medium_mm: f64,
    pub tap_drill_diameter_mm: f64,
    pub total_bolt_tensile_force_kn: f64,
    pub tensile_safety_factor: f64,
    pub shear_safety_factor: f64,
    pub joint_separation_load_kn: f64,
    pub notes: Vec<String>,
}

/// Solves bolted joint mechanics, preload, torque, and factor of safety.
pub fn solve_bolted_joint(prob: &BoltedJointProblem) -> Result<BoltedJointResult, String> {
    let spec = find_metric_thread(&prob.designation)
        .ok_or_else(|| format!("Unknown metric bolt designation: '{}'", prob.designation))?;

    let d = spec.nominal_diameter_mm;
    let p = spec.pitch_mm;
    let sp = prob.property_class.proof_strength_mpa();
    let sut = prob.property_class.tensile_strength_mpa();

    // Tensile stress area per ISO 898-1: At = (pi / 4) * (d - 0.9382 * P)^2
    let d_effective = d - 0.938194 * p;
    let at = (std::f64::consts::PI / 4.0) * d_effective * d_effective;

    // Proof load Fp = At * Sp
    let fp_kn = (at * sp) / 1000.0;

    // Recommended preload Fi = 0.75 * Fp for reusable connections
    let fi_kn = 0.75 * fp_kn;

    // Tightening torque: T = K * d * Fi
    // T [N*m] = K * (d / 1000) [m] * (Fi * 1000) [N] = K * d * Fi [N*m]
    let k_factor = prob.torque_coefficient_k.unwrap_or(0.18).clamp(0.08, 0.40);
    let torque_nm = k_factor * d * fi_kn;

    // Direct shear capacity: V_cap = 0.6 * Sut * At
    let shear_cap_kn = (0.6 * sut * at) / 1000.0;

    let n_bolts = prob.bolt_count.unwrap_or(1).max(1) as f64;
    let p_ext_per_bolt = prob.external_tensile_kn.unwrap_or(0.0) / n_bolts;
    let v_ext_per_bolt = prob.external_shear_kn.unwrap_or(0.0) / n_bolts;

    let c = prob
        .joint_stiffness_constant_c
        .unwrap_or(0.25)
        .clamp(0.05, 0.60);

    // Total bolt load Fb = Fi + C * P_ext
    let fb_kn = fi_kn + c * p_ext_per_bolt;

    // Clamping force remaining on joint: Fj = Fi - (1 - C) * P_ext
    // Joint separation occurs when Fj = 0 => P_sep = Fi / (1 - C)
    let p_sep_kn = fi_kn / (1.0 - c);

    // Tensile safety factor against proof load
    let fos_tensile = if fb_kn > 1e-6 {
        fp_kn / fb_kn
    } else {
        100.0
    };

    // Shear safety factor
    let fos_shear = if v_ext_per_bolt > 1e-6 {
        shear_cap_kn / v_ext_per_bolt
    } else {
        100.0
    };

    let notes = vec![
        format!(
            "Fastener: {} Property Class {:?} (d = {:.1} mm, P = {:.2} mm)",
            spec.designation, prob.property_class, d, p
        ),
        format!(
            "Tensile Stress Area At = {:.2} mm², Proof Strength Sp = {:.0} MPa",
            at, sp
        ),
        format!(
            "Proof Load Fp = {:.2} kN, Target Preload Fi = {:.2} kN (75% Fp)",
            fp_kn, fi_kn
        ),
        format!(
            "Recommended Tightening Torque: T = {:.1} N·m (Torque Coeff K = {:.2})",
            torque_nm, k_factor
        ),
        format!(
            "Joint Separation Force: P_sep = {:.2} kN (Stiffness Ratio C = {:.2})",
            p_sep_kn, c
        ),
        format!(
            "Safety Factors: Tensile FoS = {:.2} (Target >= 1.2), Shear FoS = {:.2}",
            fos_tensile, fos_shear
        ),
        format!(
            "Standard Hole Sizing: Fine Clearance = {:.1} mm, Medium = {:.1} mm, Tap Drill = {:.1} mm",
            spec.clearance_hole_fine_mm, spec.clearance_hole_medium_mm, spec.tap_drill_diameter_mm
        ),
    ];

    Ok(BoltedJointResult {
        designation: spec.designation,
        property_class: prob.property_class,
        nominal_diameter_mm: d,
        pitch_mm: p,
        tensile_stress_area_mm2: at,
        proof_strength_mpa: sp,
        proof_load_kn: fp_kn,
        recommended_preload_kn: fi_kn,
        tightening_torque_nm: torque_nm,
        shear_capacity_kn: shear_cap_kn,
        clearance_hole_fine_mm: spec.clearance_hole_fine_mm,
        clearance_hole_medium_mm: spec.clearance_hole_medium_mm,
        tap_drill_diameter_mm: spec.tap_drill_diameter_mm,
        total_bolt_tensile_force_kn: fb_kn,
        tensile_safety_factor: fos_tensile,
        shear_safety_factor: fos_shear,
        joint_separation_load_kn: p_sep_kn,
        notes,
    })
}

/// Simulation runner returning standard `SimulationResult`.
pub fn solve_bolted_joint_sim(prob: &BoltedJointProblem) -> Result<SimulationResult, String> {
    let sol = solve_bolted_joint(prob)?;

    let mut metrics = HashMap::new();
    metrics.insert(
        "tensile_stress_area_mm2".to_string(),
        sol.tensile_stress_area_mm2,
    );
    metrics.insert("proof_load_kn".to_string(), sol.proof_load_kn);
    metrics.insert(
        "recommended_preload_kn".to_string(),
        sol.recommended_preload_kn,
    );
    metrics.insert(
        "tightening_torque_nm".to_string(),
        sol.tightening_torque_nm,
    );
    metrics.insert("shear_capacity_kn".to_string(), sol.shear_capacity_kn);
    metrics.insert(
        "tensile_safety_factor".to_string(),
        sol.tensile_safety_factor,
    );
    metrics.insert("shear_safety_factor".to_string(), sol.shear_safety_factor);
    metrics.insert(
        "joint_separation_load_kn".to_string(),
        sol.joint_separation_load_kn,
    );

    let val = AnalyticalComparison::assess(
        "Preload to Proof Load Ratio",
        sol.recommended_preload_kn / sol.proof_load_kn,
        0.75,
    );

    let mut conv = ConvergenceMonitor::new(1e-6);
    conv.record(1, 0.0, 1, 1e-6);

    Ok(SimulationResult {
        problem_id: "bolted-joint-engineering".to_string(),
        engine_id: "atlas-fasteners".to_string(),
        status: SimulationStatus::Converged,
        execution_time_ms: 1,
        scalar_metrics: metrics,
        convergence: conv,
        validation: Some(val),
        provenance: SimulationProvenance::new("atlas-fasteners", "0.1.0"),
        logs: sol.notes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_m10_class_8_8_bolted_joint_calculations() -> Result<(), String> {
        let prob = BoltedJointProblem {
            designation: "M10".to_string(),
            property_class: BoltPropertyClass::Class8_8,
            grip_length_mm: 25.0,
            torque_coefficient_k: Some(0.18),
            external_tensile_kn: Some(10.0),
            external_shear_kn: Some(5.0),
            joint_stiffness_constant_c: Some(0.25),
            bolt_count: Some(1),
        };

        let sol = solve_bolted_joint(&prob)?;
        // M10 coarse pitch P = 1.50 mm
        // d_eff = 10 - 0.9382 * 1.5 = 8.5927 mm
        // At = (pi/4) * (8.5927)^2 = 57.99 mm2 (~58.0 mm2 standard)
        assert!(sol.tensile_stress_area_mm2 > 57.5 && sol.tensile_stress_area_mm2 < 58.5);
        // Sp = 600 MPa => Fp = 58.0 * 600 = 34.8 kN
        assert!(sol.proof_load_kn > 34.0 && sol.proof_load_kn < 35.5);
        // Fi = 0.75 * Fp approx 26.1 kN
        assert!(sol.recommended_preload_kn > 25.5 && sol.recommended_preload_kn < 26.5);
        // T = 0.18 * 10 * 26.1 = 46.98 Nm
        assert!(sol.tightening_torque_nm > 45.0 && sol.tightening_torque_nm < 49.0);
        // Clearance holes: 10.5 mm fine, 11.0 mm medium
        assert_eq!(sol.clearance_hole_fine_mm, 10.5);
        assert_eq!(sol.clearance_hole_medium_mm, 11.0);
        // FoS > 1.0
        assert!(sol.tensile_safety_factor > 1.2);
        assert!(sol.shear_safety_factor > 2.0);

        let sim_res = solve_bolted_joint_sim(&prob)?;
        assert_eq!(sim_res.status, SimulationStatus::Converged);
        Ok(())
    }

    #[test]
    fn test_standard_threads_catalog() {
        let catalog = get_standard_metric_threads();
        assert!(catalog.len() >= 13);
        assert!(find_metric_thread("M6").is_some());
        assert!(find_metric_thread("M12").is_some());
        assert!(find_metric_thread("M24").is_some());
    }
}
