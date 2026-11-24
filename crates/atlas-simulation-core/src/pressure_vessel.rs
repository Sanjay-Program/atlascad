//! # ASME Boiler & Pressure Vessel Code (BPVC Section VIII Division 1)
//!
//! Provides analytical calculation of required wall thickness for cylindrical shells,
//! 2:1 semi-ellipsoidal heads, ASME torispherical heads, and hemispherical heads under internal pressure,
//! along with Maximum Allowable Working Pressure (MAWP) and hydrostatic proof test pressures.

use crate::convergence::ConvergenceMonitor;
use crate::execution::{SimulationResult, SimulationStatus};
use crate::provenance::SimulationProvenance;
use crate::validation::AnalyticalComparison;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// ASME vessel material specification per ASME BPVC Section II-D.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VesselMaterial {
    /// Carbon steel plate for moderate/low temp service (SA-516 Grade 70)
    Sa516Gr70,
    /// Austenitic stainless steel (SA-240 Type 304)
    Sa240Type304,
    /// Low-alloy chrome-moly steel (SA-387 Grade 11 Class 2)
    Sa387Gr11,
    /// High-nickel alloy (SB-168 Inconel 600)
    Sb168Inconel600,
}

impl VesselMaterial {
    /// ASME maximum allowable stress S in MPa at given design temperature.
    pub fn allowable_stress_mpa(&self, temp_c: f64) -> f64 {
        match self {
            Self::Sa516Gr70 => {
                if temp_c <= 150.0 {
                    138.0
                } else if temp_c <= 250.0 {
                    135.0
                } else if temp_c <= 350.0 {
                    120.0
                } else {
                    95.0
                }
            }
            Self::Sa240Type304 => {
                if temp_c <= 100.0 {
                    115.0
                } else if temp_c <= 200.0 {
                    103.0
                } else if temp_c <= 300.0 {
                    92.0
                } else {
                    82.0
                }
            }
            Self::Sa387Gr11 => {
                if temp_c <= 300.0 {
                    142.0
                } else if temp_c <= 400.0 {
                    138.0
                } else {
                    125.0
                }
            }
            Self::Sb168Inconel600 => {
                if temp_c <= 200.0 {
                    160.0
                } else if temp_c <= 400.0 {
                    152.0
                } else {
                    140.0
                }
            }
        }
    }

    /// Ambient allowable stress S_ambient in MPa (for hydrotest ratio).
    pub fn ambient_allowable_stress_mpa(&self) -> f64 {
        self.allowable_stress_mpa(20.0)
    }

    /// Density rho in kg/m³.
    pub fn density_kg_m3(&self) -> f64 {
        match self {
            Self::Sa516Gr70 | Self::Sa387Gr11 => 7850.0,
            Self::Sa240Type304 => 8000.0,
            Self::Sb168Inconel600 => 8470.0,
        }
    }
}

/// Type of formed pressure vessel closure head.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HeadType {
    /// ASME 2:1 Semi-Ellipsoidal Head (UG-32(d))
    Ellipsoidal2to1,
    /// ASME Torispherical Flanged & Dished Head (UG-32(e), r = 6% D, L = D)
    TorisphericalAsme,
    /// Hemispherical Head (UG-32(f), r = D/2)
    Hemispherical,
}

/// Joint radiographic examination efficiency E (UW-12).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum JointEfficiencyType {
    /// 100% full radiographic examination (E = 1.00)
    FullRadiography,
    /// Spot radiographic examination (E = 0.85)
    SpotRadiography,
    /// Visual examination without radiography (E = 0.70)
    NoneVisualOnly,
}

impl JointEfficiencyType {
    pub fn factor(&self) -> f64 {
        match self {
            Self::FullRadiography => 1.00,
            Self::SpotRadiography => 0.85,
            Self::NoneVisualOnly => 0.70,
        }
    }
}

/// Input problem definition for ASME Section VIII Div 1 pressure vessel analysis.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PressureVesselProblem {
    pub vessel_name: String,
    pub material: VesselMaterial,
    pub head_type: HeadType,
    pub joint_efficiency: JointEfficiencyType,
    /// Internal design gauge pressure P in bar (e.g. 16.0 bar = 1.6 MPa)
    pub design_pressure_bar: f64,
    /// Maximum design operating temperature in °C
    pub design_temperature_c: f64,
    /// Inside diameter of vessel shell D in mm
    pub inside_diameter_d_mm: f64,
    /// Tangent-to-tangent cylindrical shell length in mm
    pub tangent_length_l_mm: f64,
    /// Specified corrosion allowance CA in mm (e.g. 3.0 mm for carbon steel, 0.0 for stainless)
    pub corrosion_allowance_mm: f64,
    /// Selected nominal shell thickness in mm (optional; if None, rounded standard thickness is recommended)
    pub nominal_shell_thickness_mm: Option<f64>,
    /// Selected nominal head thickness in mm (optional)
    pub nominal_head_thickness_mm: Option<f64>,
}

impl Default for PressureVesselProblem {
    fn default() -> Self {
        Self {
            vessel_name: "Chemical Process Gas Accumulator Vessel".to_string(),
            material: VesselMaterial::Sa516Gr70,
            head_type: HeadType::Ellipsoidal2to1,
            joint_efficiency: JointEfficiencyType::SpotRadiography,
            design_pressure_bar: 20.0,
            design_temperature_c: 180.0,
            inside_diameter_d_mm: 1200.0,
            tangent_length_l_mm: 3000.0,
            corrosion_allowance_mm: 2.5,
            nominal_shell_thickness_mm: Some(16.0),
            nominal_head_thickness_mm: Some(16.0),
        }
    }
}

/// Detailed calculation outputs per ASME BPVC Section VIII Division 1.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PressureVesselResult {
    pub vessel_name: String,
    pub design_pressure_mpa: f64,
    pub design_temperature_c: f64,
    pub allowable_stress_s_mpa: f64,
    pub joint_efficiency_e: f64,
    pub corrosion_allowance_mm: f64,
    pub inside_radius_r_mm: f64,
    pub min_required_shell_thickness_mm: f64,
    pub actual_shell_thickness_mm: f64,
    pub min_required_head_thickness_mm: f64,
    pub actual_head_thickness_mm: f64,
    pub shell_mawp_corroded_mpa: f64,
    pub shell_mawp_corroded_bar: f64,
    pub head_mawp_corroded_mpa: f64,
    pub head_mawp_corroded_bar: f64,
    pub vessel_mawp_bar: f64,
    pub hydrostatic_test_pressure_bar: f64,
    pub internal_volume_m3: f64,
    pub estimated_empty_mass_kg: f64,
    pub is_asme_compliant: bool,
    pub summary_notes: Vec<String>,
}

/// Solves ASME Section VIII Div 1 cylindrical shell and formed heads thicknesses and MAWP.
pub fn solve_asme_pressure_vessel(
    prob: &PressureVesselProblem,
) -> Result<PressureVesselResult, String> {
    let p_bar = prob.design_pressure_bar;
    let d_inside = prob.inside_diameter_d_mm;
    let l_cyl = prob.tangent_length_l_mm;

    if p_bar <= 0.0 || d_inside <= 50.0 || l_cyl <= 50.0 {
        return Err(
            "Design pressure, diameter (>50mm), and tangent length must be strictly positive."
                .to_string(),
        );
    }

    // Convert P to MPa: 1 bar = 0.1 MPa
    let p_mpa = p_bar * 0.10;
    let r_inside = d_inside / 2.0;
    let s_mpa = prob
        .material
        .allowable_stress_mpa(prob.design_temperature_c);
    let s_amb = prob.material.ambient_allowable_stress_mpa();
    let e = prob.joint_efficiency.factor();
    let ca = prob.corrosion_allowance_mm.max(0.0);

    // 1. Cylindrical Shell Minimum Thickness (UG-27(c)(1)):
    // t_req = (P * R) / (S * E - 0.6 * P) + CA
    let denom_cyl = s_mpa * e - 0.6 * p_mpa;
    if denom_cyl <= 0.0 {
        return Err(
            "Internal design pressure exceeds maximum material allowable limit (S*E <= 0.6*P)."
                .to_string(),
        );
    }
    let t_cyl_corroded = (p_mpa * r_inside) / denom_cyl;
    let t_cyl_min = t_cyl_corroded + ca;

    // Actual shell thickness:
    let t_cyl_actual = prob
        .nominal_shell_thickness_mm
        .unwrap_or(t_cyl_min.ceil())
        .max(t_cyl_min);
    let t_cyl_eff = (t_cyl_actual - ca).max(0.1);

    // Shell MAWP corroded (UG-27(c)(1) inverted):
    // MAWP = (S * E * t_eff) / (R + 0.6 * t_eff)
    let mawp_cyl_mpa = (s_mpa * e * t_cyl_eff) / (r_inside + 0.6 * t_cyl_eff);
    let mawp_cyl_bar = mawp_cyl_mpa * 10.0;

    // 2. Formed Heads Minimum Thickness (UG-32):
    let (t_head_corroded, mawp_head_mpa) = match prob.head_type {
        HeadType::Ellipsoidal2to1 => {
            // UG-32(d): t = (P * D) / (2 * S * E - 0.2 * P)
            let denom_head = 2.0 * s_mpa * e - 0.2 * p_mpa;
            if denom_head <= 0.0 {
                return Err(
                    "Internal design pressure exceeds head allowable stress limit.".to_string(),
                );
            }
            let t_req = (p_mpa * d_inside) / denom_head;
            let t_head_actual = prob
                .nominal_head_thickness_mm
                .unwrap_or((t_req + ca).ceil())
                .max(t_req + ca);
            let t_head_eff = (t_head_actual - ca).max(0.1);
            let mawp = (2.0 * s_mpa * e * t_head_eff) / (d_inside + 0.2 * t_head_eff);
            (t_req, mawp)
        }
        HeadType::TorisphericalAsme => {
            // UG-32(e): L = D, r = 0.06 * D -> M = 0.25 * (3 + sqrt(L/r)) = 0.25 * (3 + sqrt(1/0.06)) = 1.77
            let l_crown = d_inside;
            let r_knuckle = 0.06 * d_inside;
            let m_factor = 0.25 * (3.0 + (l_crown / r_knuckle).sqrt());
            let denom_tori = 2.0 * s_mpa * e - 0.2 * p_mpa;
            if denom_tori <= 0.0 {
                return Err(
                    "Internal design pressure exceeds torispherical allowable limit.".to_string(),
                );
            }
            let t_req = (p_mpa * l_crown * m_factor) / denom_tori;
            let t_head_actual = prob
                .nominal_head_thickness_mm
                .unwrap_or((t_req + ca).ceil())
                .max(t_req + ca);
            let t_head_eff = (t_head_actual - ca).max(0.1);
            let mawp = (2.0 * s_mpa * e * t_head_eff) / (m_factor * l_crown + 0.2 * t_head_eff);
            (t_req, mawp)
        }
        HeadType::Hemispherical => {
            // UG-32(f): t = (P * R) / (2 * S * E - 0.2 * P)
            let denom_hemi = 2.0 * s_mpa * e - 0.2 * p_mpa;
            if denom_hemi <= 0.0 {
                return Err(
                    "Internal design pressure exceeds hemispherical allowable limit.".to_string(),
                );
            }
            let t_req = (p_mpa * r_inside) / denom_hemi;
            let t_head_actual = prob
                .nominal_head_thickness_mm
                .unwrap_or((t_req + ca).ceil())
                .max(t_req + ca);
            let t_head_eff = (t_head_actual - ca).max(0.1);
            let mawp = (2.0 * s_mpa * e * t_head_eff) / (r_inside + 0.2 * t_head_eff);
            (t_req, mawp)
        }
    };

    let t_head_min = t_head_corroded + ca;
    let t_head_actual = prob
        .nominal_head_thickness_mm
        .unwrap_or(t_head_min.ceil())
        .max(t_head_min);
    let mawp_head_bar = mawp_head_mpa * 10.0;

    // Overall vessel MAWP is governed by the weaker component:
    let vessel_mawp_bar = mawp_cyl_bar.min(mawp_head_bar);

    // 3. Hydrostatic Proof Test Pressure (UG-99(b)):
    // P_hydro = 1.3 * MAWP * (S_ambient / S_design)
    let hydro_test_bar = 1.3 * vessel_mawp_bar * (s_amb / s_mpa);

    // 4. Geometry and Volume:
    // Cylinder volume: V_cyl = pi * R^2 * L
    let r_m = r_inside / 1000.0;
    let l_m = l_cyl / 1000.0;
    let v_cyl = std::f64::consts::PI * r_m.powi(2) * l_m;
    // 2 Heads volume (for 2:1 ellipsoidal: V_head = pi * D^3 / 24, both heads = pi * D^3 / 12)
    let v_heads = match prob.head_type {
        HeadType::Ellipsoidal2to1 => (std::f64::consts::PI * (2.0 * r_m).powi(3)) / 12.0,
        HeadType::TorisphericalAsme => 0.085 * (2.0 * r_m).powi(3),
        HeadType::Hemispherical => (4.0 / 3.0) * std::f64::consts::PI * r_m.powi(3),
    };
    let total_internal_vol_m3 = v_cyl + v_heads;

    // Metal Mass Estimation:
    let rho = prob.material.density_kg_m3();
    let shell_mid_rad_m = (r_inside + t_cyl_actual * 0.5) / 1000.0;
    let shell_steel_vol_m3 =
        2.0 * std::f64::consts::PI * shell_mid_rad_m * l_m * (t_cyl_actual / 1000.0);
    // Head surface area (approx 1.084 * D^2 for 2:1 ellip each):
    let head_area_m2 = match prob.head_type {
        HeadType::Ellipsoidal2to1 => 1.084 * (2.0 * r_m).powi(2),
        HeadType::TorisphericalAsme => 0.93 * (2.0 * r_m).powi(2),
        HeadType::Hemispherical => 2.0 * std::f64::consts::PI * r_m.powi(2),
    };
    let heads_steel_vol_m3 = 2.0 * head_area_m2 * (t_head_actual / 1000.0);
    let empty_mass_kg = (shell_steel_vol_m3 + heads_steel_vol_m3) * rho;

    let is_ok =
        t_cyl_actual >= t_cyl_min && t_head_actual >= t_head_min && vessel_mawp_bar >= p_bar;

    let notes = vec![
        format!(
            "ASME BPVC Sec. VIII Div. 1: '{}' (ID = {:.0} mm, Length = {:.0} mm, Design P = {:.1} bar @ {:.0}°C)",
            prob.vessel_name, d_inside, l_cyl, p_bar, prob.design_temperature_c
        ),
        format!(
            "Material: Allowable Stress S = {:.1} MPa, Joint Efficiency E = {:.2}, Corrosion Allowance = {:.1} mm",
            s_mpa, e, ca
        ),
        format!(
            "Cylindrical Shell: Min Required t = {:.2} mm, Nominal t = {:.1} mm -> Shell MAWP = {:.2} bar",
            t_cyl_min, t_cyl_actual, mawp_cyl_bar
        ),
        format!(
            "Formed Heads: Min Required t = {:.2} mm, Nominal t = {:.1} mm -> Head MAWP = {:.2} bar",
            t_head_min, t_head_actual, mawp_head_bar
        ),
        format!(
            "Governing Vessel MAWP = {:.2} bar, Hydrostatic Proof Test Pressure = {:.2} bar (UG-99(b))",
            vessel_mawp_bar, hydro_test_bar
        ),
        format!(
            "Capacity: Internal Volume = {:.3} m³, Empty Shell & Heads Mass = {:.1} kg",
            total_internal_vol_m3, empty_mass_kg
        ),
    ];

    Ok(PressureVesselResult {
        vessel_name: prob.vessel_name.clone(),
        design_pressure_mpa: p_mpa,
        design_temperature_c: prob.design_temperature_c,
        allowable_stress_s_mpa: s_mpa,
        joint_efficiency_e: e,
        corrosion_allowance_mm: ca,
        inside_radius_r_mm: r_inside,
        min_required_shell_thickness_mm: t_cyl_min,
        actual_shell_thickness_mm: t_cyl_actual,
        min_required_head_thickness_mm: t_head_min,
        actual_head_thickness_mm: t_head_actual,
        shell_mawp_corroded_mpa: mawp_cyl_mpa,
        shell_mawp_corroded_bar: mawp_cyl_bar,
        head_mawp_corroded_mpa: mawp_head_mpa,
        head_mawp_corroded_bar: mawp_head_bar,
        vessel_mawp_bar,
        hydrostatic_test_pressure_bar: hydro_test_bar,
        internal_volume_m3: total_internal_vol_m3,
        estimated_empty_mass_kg: empty_mass_kg,
        is_asme_compliant: is_ok,
        summary_notes: notes,
    })
}

/// Simulation runner adapter for ASME pressure vessel.
pub fn solve_pressure_vessel_sim(prob: &PressureVesselProblem) -> Result<SimulationResult, String> {
    let sol = solve_asme_pressure_vessel(prob)?;

    let mut metrics = HashMap::new();
    metrics.insert(
        "min_required_shell_thickness_mm".to_string(),
        sol.min_required_shell_thickness_mm,
    );
    metrics.insert(
        "actual_shell_thickness_mm".to_string(),
        sol.actual_shell_thickness_mm,
    );
    metrics.insert(
        "min_required_head_thickness_mm".to_string(),
        sol.min_required_head_thickness_mm,
    );
    metrics.insert(
        "actual_head_thickness_mm".to_string(),
        sol.actual_head_thickness_mm,
    );
    metrics.insert("vessel_mawp_bar".to_string(), sol.vessel_mawp_bar);
    metrics.insert(
        "hydrostatic_test_pressure_bar".to_string(),
        sol.hydrostatic_test_pressure_bar,
    );
    metrics.insert("internal_volume_m3".to_string(), sol.internal_volume_m3);
    metrics.insert(
        "estimated_empty_mass_kg".to_string(),
        sol.estimated_empty_mass_kg,
    );

    let mut conv = ConvergenceMonitor::new(1e-6);
    conv.record(1, sol.vessel_mawp_bar, 1, 1e-4);

    let val = AnalyticalComparison::assess(
        "Pressure Vessel Shell Thickness (mm)",
        sol.actual_shell_thickness_mm,
        sol.min_required_shell_thickness_mm,
    );

    Ok(SimulationResult {
        problem_id: "pressure-vessel-asme-sec8".to_string(),
        engine_id: "atlas-pressure-vessels".to_string(),
        status: SimulationStatus::Converged,
        execution_time_ms: 1,
        scalar_metrics: metrics,
        convergence: conv,
        validation: Some(val),
        provenance: SimulationProvenance::new("atlas-pressure-vessels", "0.1.0"),
        logs: sol.summary_notes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_standard_sa516_chemical_accumulator_vessel() -> Result<(), String> {
        let prob = PressureVesselProblem::default();
        let sol = solve_asme_pressure_vessel(&prob)?;

        // P = 20 bar = 2.0 MPa, D = 1200 mm, R = 600 mm, S = 135 MPa at 180C, E = 0.85
        // Denom = 135 * 0.85 - 0.6 * 2.0 = 114.75 - 1.2 = 113.55 MPa
        // t_cyl_corroded = 2.0 * 600 / 113.55 = 10.57 mm
        // t_cyl_min = 10.57 + 2.5 = 13.07 mm
        assert!((sol.min_required_shell_thickness_mm - 13.07).abs() < 0.2);
        // Nominal thickness 16mm > 13.07 mm
        assert!(sol.actual_shell_thickness_mm >= sol.min_required_shell_thickness_mm);
        // MAWP must exceed design 20.0 bar
        assert!(sol.vessel_mawp_bar > 20.0);
        // Hydrotest pressure = 1.3 * MAWP * (138/135)
        assert!(sol.hydrostatic_test_pressure_bar > 30.0);
        // Volume: cylindrical 600mm rad, 3000mm length -> ~3.4 m³ + 2 heads ~ 3.9 m³
        assert!(sol.internal_volume_m3 > 3.5 && sol.internal_volume_m3 < 4.5);
        assert!(sol.is_asme_compliant);

        let sim = solve_pressure_vessel_sim(&prob)?;
        assert_eq!(sim.status, SimulationStatus::Converged);
        Ok(())
    }
}
