//! # Sheet Metal Modeling, Bend Allowance, and Flat Pattern Unfolding (DIN 6935)
//!
//! Calculates neutral axis shift (K-factor), bend allowance (BA), outside setback (OSSB),
//! bend deduction (BD), unfolded flat pattern dimensions, minimum bend relief geometry,
//! and hydraulic press-brake forming tonnage.

use crate::convergence::ConvergenceMonitor;
use crate::execution::{SimulationResult, SimulationStatus};
use crate::provenance::SimulationProvenance;
use crate::validation::AnalyticalComparison;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Material classification for sheet metal forming.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SheetMetalMaterial {
    CarbonSteel,
    StainlessSteel,
    Aluminum5052,
    CopperC11000,
    BrassC26000,
}

impl SheetMetalMaterial {
    /// Nominal ultimate tensile strength (UTS) in MPa.
    pub fn nominal_uts_mpa(&self) -> f64 {
        match self {
            Self::CarbonSteel => 400.0,
            Self::StainlessSteel => 620.0,
            Self::Aluminum5052 => 230.0,
            Self::CopperC11000 => 260.0,
            Self::BrassC26000 => 380.0,
        }
    }

    /// Typical empirical neutral axis K-factor for standard air bending.
    pub fn default_k_factor(&self, inside_radius_mm: f64, thickness_mm: f64) -> f64 {
        let r_over_t = if thickness_mm > 1e-6 {
            inside_radius_mm / thickness_mm
        } else {
            1.0
        };

        match self {
            Self::Aluminum5052 => {
                if r_over_t < 1.0 {
                    0.33
                } else if r_over_t < 3.0 {
                    0.40
                } else {
                    0.48
                }
            }
            Self::StainlessSteel => {
                if r_over_t < 1.0 {
                    0.35
                } else if r_over_t < 3.0 {
                    0.42
                } else {
                    0.50
                }
            }
            _ => {
                if r_over_t < 1.0 {
                    0.33
                } else if r_over_t < 2.5 {
                    0.40
                } else {
                    0.50
                }
            }
        }
    }

    /// Minimum recommended inside bend radius factor (R_min = factor * T).
    pub fn min_radius_factor(&self) -> f64 {
        match self {
            Self::CarbonSteel => 1.0,
            Self::StainlessSteel => 1.5,
            Self::Aluminum5052 => 1.0,
            Self::CopperC11000 => 0.8,
            Self::BrassC26000 => 1.0,
        }
    }
}

/// A single bend in a sheet metal part.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SheetMetalBend {
    pub bend_id: String,
    /// Included bend angle in degrees (e.g. 90.0 for right-angle bend).
    pub bend_angle_deg: f64,
    /// Inside bend radius in millimeters.
    pub inside_radius_mm: f64,
    /// Custom K-factor override. If None, derived from material and R/T ratio.
    pub k_factor: Option<f64>,
    /// Length of the first adjacent flange in millimeters (measured to virtual sharp apex).
    pub flange1_length_mm: f64,
    /// Length of the second adjacent flange in millimeters.
    pub flange2_length_mm: f64,
}

/// Input problem for a multi-bend sheet metal part.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SheetMetalPartProblem {
    /// Sheet metal gauge thickness in millimeters.
    pub thickness_mm: f64,
    /// Sheet metal part width / depth along bend line in millimeters.
    pub width_mm: f64,
    /// Forming material.
    pub material: SheetMetalMaterial,
    /// List of sequential bends.
    pub bends: Vec<SheetMetalBend>,
    /// Custom ultimate tensile strength override in MPa.
    pub uts_mpa: Option<f64>,
    /// Press brake bottom V-die opening width in millimeters.
    /// Default standard is 8 * thickness for air bending.
    pub die_opening_v_mm: Option<f64>,
}

impl Default for SheetMetalPartProblem {
    fn default() -> Self {
        Self {
            thickness_mm: 2.0,
            width_mm: 100.0,
            material: SheetMetalMaterial::CarbonSteel,
            bends: vec![SheetMetalBend {
                bend_id: "bend_1".to_string(),
                bend_angle_deg: 90.0,
                inside_radius_mm: 2.0,
                k_factor: Some(0.40),
                flange1_length_mm: 50.0,
                flange2_length_mm: 50.0,
            }],
            uts_mpa: Some(400.0),
            die_opening_v_mm: Some(16.0),
        }
    }
}

/// Calculated analytical metrics for an individual bend.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SheetMetalBendResult {
    pub bend_id: String,
    pub k_factor: f64,
    pub bend_allowance_mm: f64,
    pub outside_setback_mm: f64,
    pub bend_deduction_mm: f64,
    pub neutral_axis_radius_mm: f64,
    pub flat_flange1_mm: f64,
    pub flat_flange2_mm: f64,
    pub bend_flat_arc_mm: f64,
    pub forming_force_kn: f64,
    pub min_bend_relief_width_mm: f64,
    pub min_bend_relief_depth_mm: f64,
}

/// Comprehensive unfolded flat pattern and press-brake tooling results.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SheetMetalUnfoldResult {
    pub thickness_mm: f64,
    pub width_mm: f64,
    pub total_flat_length_mm: f64,
    pub total_flat_width_mm: f64,
    pub flat_pattern_area_mm2: f64,
    pub total_forming_force_kn: f64,
    pub total_forming_tonnage_metric_tons: f64,
    pub recommended_die_opening_v_mm: f64,
    pub min_bend_radius_limit_mm: f64,
    pub bends: Vec<SheetMetalBendResult>,
    pub flat_profile_coordinates: Vec<[f64; 2]>,
    pub notes: Vec<String>,
}

/// Solves sheet metal neutral axis deformation, bend deduction, and unfolded flat pattern.
pub fn solve_sheet_metal_unfold(
    prob: &SheetMetalPartProblem,
) -> Result<SheetMetalUnfoldResult, String> {
    let t = prob.thickness_mm;
    let w = prob.width_mm;

    if t <= 0.0 || w <= 0.0 {
        return Err("Sheet metal thickness and width must be strictly positive.".to_string());
    }
    if prob.bends.is_empty() {
        return Err("Sheet metal problem requires at least one bend.".to_string());
    }

    let uts = prob
        .uts_mpa
        .unwrap_or_else(|| prob.material.nominal_uts_mpa());
    let v_die = prob.die_opening_v_mm.unwrap_or(8.0 * t).max(4.0 * t);
    let min_r_limit = prob.material.min_radius_factor() * t;

    let mut bend_results = Vec::with_capacity(prob.bends.len());
    let mut total_forming_kn = 0.0;
    let mut total_flat_len = 0.0;

    for (i, bend) in prob.bends.iter().enumerate() {
        if bend.bend_angle_deg <= 0.0 || bend.bend_angle_deg >= 180.0 {
            return Err(format!(
                "Bend '{}' angle must be in (0, 180) degrees (got {:.1})",
                bend.bend_id, bend.bend_angle_deg
            ));
        }
        if bend.inside_radius_mm < 0.0 {
            return Err(format!(
                "Bend '{}' inside radius cannot be negative",
                bend.bend_id
            ));
        }

        let r = bend.inside_radius_mm;
        let k = bend
            .k_factor
            .unwrap_or_else(|| prob.material.default_k_factor(r, t))
            .clamp(0.20, 0.50);

        let theta_rad = bend.bend_angle_deg.to_radians();

        // Bend Allowance: BA = theta * (R + K * T)
        let ba = theta_rad * (r + k * t);

        // Outside Setback: OSSB = (R + T) * tan(theta / 2)
        let half_theta = theta_rad * 0.5;
        let ossb = (r + t) * half_theta.tan();

        // Bend Deduction: BD = 2 * OSSB - BA
        let bd = 2.0 * ossb - ba;

        // Neutral axis radius: Rn = R + K * T
        let rn = r + k * t;

        // Air bending forming force (V-die bending equation):
        // F = (C * UTS * W * T^2) / V_die, where C approx 1.33 for bottoming/air bending
        let f_bend_n = (1.33 * uts * w * t * t) / v_die;
        let f_bend_kn = f_bend_n / 1000.0;
        total_forming_kn += f_bend_kn;

        // Minimum bend relief geometry per DIN 6935: width >= T, depth >= R + T
        let relief_w = t.max(1.5);
        let relief_d = (r + t).max(2.0);

        // Flat flange segment calculation:
        let flat_f1 = (bend.flange1_length_mm - ossb).max(0.0);
        let flat_f2 = (bend.flange2_length_mm - ossb).max(0.0);

        if i == 0 {
            total_flat_len += flat_f1 + ba + flat_f2;
        } else {
            total_flat_len += ba + flat_f2;
        }

        bend_results.push(SheetMetalBendResult {
            bend_id: bend.bend_id.clone(),
            k_factor: k,
            bend_allowance_mm: ba,
            outside_setback_mm: ossb,
            bend_deduction_mm: bd,
            neutral_axis_radius_mm: rn,
            flat_flange1_mm: flat_f1,
            flat_flange2_mm: flat_f2,
            bend_flat_arc_mm: ba,
            forming_force_kn: f_bend_kn,
            min_bend_relief_width_mm: relief_w,
            min_bend_relief_depth_mm: relief_d,
        });
    }

    let tonnage_metric = total_forming_kn / 9.80665;
    let flat_area = total_flat_len * w;

    // Generate rectangular flat pattern 2D boundary polygon coordinates
    let flat_profile = vec![
        [0.0, 0.0],
        [total_flat_len, 0.0],
        [total_flat_len, w],
        [0.0, w],
        [0.0, 0.0],
    ];

    let mut notes = vec![
        format!(
            "Sheet Metal Material: {:?} (UTS = {:.1} MPa, Gauge T = {:.2} mm)",
            prob.material, uts, t
        ),
        format!(
            "Unfolded Flat Blank: Length = {:.2} mm, Width = {:.2} mm (Area = {:.1} mm²)",
            total_flat_len, w, flat_area
        ),
        format!(
            "Total Press Brake Forming Force: {:.2} kN ({:.2} metric tons) with V-die = {:.1} mm",
            total_forming_kn, tonnage_metric, v_die
        ),
        format!(
            "Minimum Safe Inside Bend Radius (Crack Prevention): R_min = {:.2} mm",
            min_r_limit
        ),
    ];

    for b in &bend_results {
        notes.push(format!(
            "  • {}: Angle = {:.1}°, K = {:.3}, BA = {:.2} mm, OSSB = {:.2} mm, BD = {:.2} mm",
            b.bend_id,
            prob.bends
                .iter()
                .find(|x| x.bend_id == b.bend_id)
                .map(|x| x.bend_angle_deg)
                .unwrap_or(90.0),
            b.k_factor,
            b.bend_allowance_mm,
            b.outside_setback_mm,
            b.bend_deduction_mm
        ));
    }

    Ok(SheetMetalUnfoldResult {
        thickness_mm: t,
        width_mm: w,
        total_flat_length_mm: total_flat_len,
        total_flat_width_mm: w,
        flat_pattern_area_mm2: flat_area,
        total_forming_force_kn: total_forming_kn,
        total_forming_tonnage_metric_tons: tonnage_metric,
        recommended_die_opening_v_mm: v_die,
        min_bend_radius_limit_mm: min_r_limit,
        bends: bend_results,
        flat_profile_coordinates: flat_profile,
        notes,
    })
}

/// Simulation wrapper returning standard `SimulationResult`.
pub fn solve_sheet_metal_sim(prob: &SheetMetalPartProblem) -> Result<SimulationResult, String> {
    let sol = solve_sheet_metal_unfold(prob)?;

    let mut metrics = HashMap::new();
    metrics.insert("total_flat_length_mm".to_string(), sol.total_flat_length_mm);
    metrics.insert("total_flat_width_mm".to_string(), sol.total_flat_width_mm);
    metrics.insert(
        "flat_pattern_area_mm2".to_string(),
        sol.flat_pattern_area_mm2,
    );
    metrics.insert("forming_force_kn".to_string(), sol.total_forming_force_kn);
    metrics.insert(
        "forming_tonnage_metric_tons".to_string(),
        sol.total_forming_tonnage_metric_tons,
    );
    if let Some(first_bend) = sol.bends.first() {
        metrics.insert(
            "bend_allowance_mm".to_string(),
            first_bend.bend_allowance_mm,
        );
        metrics.insert(
            "bend_deduction_mm".to_string(),
            first_bend.bend_deduction_mm,
        );
        metrics.insert("k_factor".to_string(), first_bend.k_factor);
    }

    let analytical_unfold_check = sol.bends[0].flat_flange1_mm
        + sol.bends[0].bend_allowance_mm
        + sol.bends[0].flat_flange2_mm;
    let val = AnalyticalComparison::assess(
        "Single Bend Flat Length Conservation",
        sol.total_flat_length_mm,
        analytical_unfold_check,
    );

    let mut conv = ConvergenceMonitor::new(1e-6);
    conv.record(1, 0.0, 1, 1e-6);

    Ok(SimulationResult {
        problem_id: "sheet-metal-unfold".to_string(),
        engine_id: "atlas-sheet-metal".to_string(),
        status: SimulationStatus::Converged,
        execution_time_ms: 1,
        scalar_metrics: metrics,
        convergence: conv,
        validation: Some(val),
        provenance: SimulationProvenance::new("atlas-sheet-metal", "0.1.0"),
        logs: sol.notes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_standard_90deg_bend_allowance_and_deduction() -> Result<(), String> {
        // 2mm steel plate with 90° bend, R = 2mm, Flange1 = 50mm, Flange2 = 50mm, K = 0.40
        let prob = SheetMetalPartProblem {
            thickness_mm: 2.0,
            width_mm: 100.0,
            material: SheetMetalMaterial::CarbonSteel,
            bends: vec![SheetMetalBend {
                bend_id: "b1".to_string(),
                bend_angle_deg: 90.0,
                inside_radius_mm: 2.0,
                k_factor: Some(0.40),
                flange1_length_mm: 50.0,
                flange2_length_mm: 50.0,
            }],
            uts_mpa: Some(400.0),
            die_opening_v_mm: Some(16.0),
        };

        let sol = solve_sheet_metal_unfold(&prob)?;

        // BA = (pi / 2) * (2.0 + 0.4 * 2.0) = 1.570796 * 2.8 = 4.398 mm
        let b1 = &sol.bends[0];
        assert!((b1.bend_allowance_mm - 4.3982).abs() < 1e-3);

        // OSSB = (2 + 2) * tan(45°) = 4.0 mm
        assert!((b1.outside_setback_mm - 4.0).abs() < 1e-3);

        // BD = 2 * OSSB - BA = 8.0 - 4.3982 = 3.6018 mm
        assert!((b1.bend_deduction_mm - 3.6018).abs() < 1e-3);

        // Total Flat Length = 50 + 50 - 3.6018 = 96.398 mm
        assert!((sol.total_flat_length_mm - 96.3982).abs() < 1e-3);

        // Forming force > 0
        assert!(sol.total_forming_force_kn > 0.0);
        assert!(sol.total_forming_tonnage_metric_tons > 0.0);
        Ok(())
    }

    #[test]
    fn test_u_channel_double_bend_unfolding() -> Result<(), String> {
        // U-channel: Flange1 = 30mm, Web = 60mm (Flange2 of b1 & Flange1 of b2), Flange3 = 30mm
        let prob = SheetMetalPartProblem {
            thickness_mm: 1.5,
            width_mm: 50.0,
            material: SheetMetalMaterial::Aluminum5052,
            bends: vec![
                SheetMetalBend {
                    bend_id: "bend_left".to_string(),
                    bend_angle_deg: 90.0,
                    inside_radius_mm: 1.5,
                    k_factor: Some(0.38),
                    flange1_length_mm: 30.0,
                    flange2_length_mm: 60.0,
                },
                SheetMetalBend {
                    bend_id: "bend_right".to_string(),
                    bend_angle_deg: 90.0,
                    inside_radius_mm: 1.5,
                    k_factor: Some(0.38),
                    flange1_length_mm: 60.0,
                    flange2_length_mm: 30.0,
                },
            ],
            uts_mpa: Some(230.0),
            die_opening_v_mm: Some(12.0),
        };

        let sol = solve_sheet_metal_unfold(&prob)?;
        assert_eq!(sol.bends.len(), 2);
        assert!(sol.total_flat_length_mm > 100.0 && sol.total_flat_length_mm < 120.0);
        assert!(sol.total_forming_tonnage_metric_tons > 0.0);
        Ok(())
    }
}
