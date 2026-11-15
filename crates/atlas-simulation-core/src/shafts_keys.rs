//! # Transmission Shaft Sizing & Parallel Drive Key Engineering (DIN 6885-1 / ASME B106.1M)
//!
//! Provides analytical calculation of transmission shaft diameters under combined bending and torsion,
//! torsional angular deflection, DIN 6885-1 parallel key dimensions, shear stress, and hub contact pressure.

use crate::convergence::ConvergenceMonitor;
use crate::execution::{SimulationResult, SimulationStatus};
use crate::provenance::SimulationProvenance;
use crate::validation::AnalyticalComparison;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Material grade for transmission shafting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShaftMaterial {
    /// AISI 1045 / C45 normalized (Yield = 310 MPa, UTS = 570 MPa)
    CarbonSteelC45,
    /// AISI 4140 / 42CrMo4 quenched & tempered (Yield = 650 MPa, UTS = 900 MPa)
    AlloySteel42CrMo4,
    /// AISI 4340 / 34CrNiMo6 high-strength alloy (Yield = 850 MPa, UTS = 1000 MPa)
    NickelChromeSteel4340,
    /// AISI 316 / 1.4401 austenitic stainless (Yield = 220 MPa, UTS = 520 MPa)
    Stainless316,
}

impl ShaftMaterial {
    pub fn yield_strength_mpa(&self) -> f64 {
        match self {
            Self::CarbonSteelC45 => 310.0,
            Self::AlloySteel42CrMo4 => 650.0,
            Self::NickelChromeSteel4340 => 850.0,
            Self::Stainless316 => 220.0,
        }
    }

    pub fn ultimate_strength_mpa(&self) -> f64 {
        match self {
            Self::CarbonSteelC45 => 570.0,
            Self::AlloySteel42CrMo4 => 900.0,
            Self::NickelChromeSteel4340 => 1000.0,
            Self::Stainless316 => 520.0,
        }
    }

    pub fn shear_modulus_gpa(&self) -> f64 {
        match self {
            Self::CarbonSteelC45 | Self::AlloySteel42CrMo4 | Self::NickelChromeSteel4340 => 79.3,
            Self::Stainless316 => 74.0,
        }
    }
}

/// Material for mating hub attached to shaft via keyway.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HubMaterial {
    CastIronGG25,
    StructuralSteelS275,
    AlloySteel42CrMo4,
}

impl HubMaterial {
    /// Maximum permissible contact surface pressure p_perm in MPa.
    pub fn permissible_contact_pressure_mpa(&self) -> f64 {
        match self {
            Self::CastIronGG25 => 100.0,
            Self::StructuralSteelS275 => 150.0,
            Self::AlloySteel42CrMo4 => 220.0,
        }
    }
}

/// Problem input specification for shaft and keyway design.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShaftKeyProblem {
    pub shaft_name: String,
    pub shaft_material: ShaftMaterial,
    pub hub_material: HubMaterial,
    /// Transmitted mechanical power in kW
    pub transmitted_power_kw: f64,
    /// Rotational speed in RPM
    pub rotational_speed_rpm: f64,
    /// Peak bending moment in N·m (from pulleys, gears, or belt tension)
    pub bending_moment_nm: f64,
    /// Shaft unsupported span length in meters (for torsional deflection)
    pub shaft_length_m: f64,
    /// Desired mechanical safety factor (default 2.0)
    pub safety_factor: Option<f64>,
    /// ASME shock/fatigue bending factor Kb (default 1.5)
    pub shock_factor_kb: Option<f64>,
    /// ASME shock/fatigue torsion factor Kt (default 1.0)
    pub shock_factor_kt: Option<f64>,
    /// Selected nominal shaft diameter in mm (if None, analytical minimum is adopted)
    pub selected_shaft_diameter_mm: Option<f64>,
    /// Key length in mm (if None, typical 1.5 * diameter is selected)
    pub key_length_mm: Option<f64>,
}

impl Default for ShaftKeyProblem {
    fn default() -> Self {
        Self {
            shaft_name: "Gearbox Intermediate Countershaft".to_string(),
            shaft_material: ShaftMaterial::AlloySteel42CrMo4,
            hub_material: HubMaterial::StructuralSteelS275,
            transmitted_power_kw: 45.0,
            rotational_speed_rpm: 1450.0,
            bending_moment_nm: 350.0,
            shaft_length_m: 0.60,
            safety_factor: Some(2.0),
            shock_factor_kb: Some(1.5),
            shock_factor_kt: Some(1.2),
            selected_shaft_diameter_mm: Some(45.0),
            key_length_mm: Some(60.0),
        }
    }
}

/// Standard DIN 6885-1 Parallel Key specification for a given shaft diameter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Din6885KeyDimensions {
    pub key_width_b_mm: f64,
    pub key_height_h_mm: f64,
    pub shaft_keyway_depth_t1_mm: f64,
    pub hub_keyway_depth_t2_mm: f64,
}

/// Resolves standard DIN 6885-1 key size from shaft diameter d in mm.
pub fn lookup_din6885_key(shaft_dia_mm: f64) -> Din6885KeyDimensions {
    let d = shaft_dia_mm;
    if d <= 12.0 {
        Din6885KeyDimensions { key_width_b_mm: 4.0, key_height_h_mm: 4.0, shaft_keyway_depth_t1_mm: 2.5, hub_keyway_depth_t2_mm: 1.8 }
    } else if d <= 17.0 {
        Din6885KeyDimensions { key_width_b_mm: 5.0, key_height_h_mm: 5.0, shaft_keyway_depth_t1_mm: 3.0, hub_keyway_depth_t2_mm: 2.3 }
    } else if d <= 22.0 {
        Din6885KeyDimensions { key_width_b_mm: 6.0, key_height_h_mm: 6.0, shaft_keyway_depth_t1_mm: 3.5, hub_keyway_depth_t2_mm: 2.8 }
    } else if d <= 30.0 {
        Din6885KeyDimensions { key_width_b_mm: 8.0, key_height_h_mm: 7.0, shaft_keyway_depth_t1_mm: 4.0, hub_keyway_depth_t2_mm: 3.3 }
    } else if d <= 38.0 {
        Din6885KeyDimensions { key_width_b_mm: 10.0, key_height_h_mm: 8.0, shaft_keyway_depth_t1_mm: 5.0, hub_keyway_depth_t2_mm: 3.3 }
    } else if d <= 44.0 {
        Din6885KeyDimensions { key_width_b_mm: 12.0, key_height_h_mm: 8.0, shaft_keyway_depth_t1_mm: 5.0, hub_keyway_depth_t2_mm: 3.3 }
    } else if d <= 50.0 {
        Din6885KeyDimensions { key_width_b_mm: 14.0, key_height_h_mm: 9.0, shaft_keyway_depth_t1_mm: 5.5, hub_keyway_depth_t2_mm: 3.8 }
    } else if d <= 58.0 {
        Din6885KeyDimensions { key_width_b_mm: 16.0, key_height_h_mm: 10.0, shaft_keyway_depth_t1_mm: 6.0, hub_keyway_depth_t2_mm: 4.3 }
    } else if d <= 65.0 {
        Din6885KeyDimensions { key_width_b_mm: 18.0, key_height_h_mm: 11.0, shaft_keyway_depth_t1_mm: 7.0, hub_keyway_depth_t2_mm: 4.4 }
    } else if d <= 75.0 {
        Din6885KeyDimensions { key_width_b_mm: 20.0, key_height_h_mm: 12.0, shaft_keyway_depth_t1_mm: 7.5, hub_keyway_depth_t2_mm: 4.9 }
    } else if d <= 85.0 {
        Din6885KeyDimensions { key_width_b_mm: 22.0, key_height_h_mm: 14.0, shaft_keyway_depth_t1_mm: 9.0, hub_keyway_depth_t2_mm: 5.4 }
    } else {
        Din6885KeyDimensions { key_width_b_mm: 25.0, key_height_h_mm: 14.0, shaft_keyway_depth_t1_mm: 9.0, hub_keyway_depth_t2_mm: 5.4 }
    }
}

/// Comprehensive calculation results for transmission shaft and keyway.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShaftKeyResult {
    pub shaft_name: String,
    pub torque_nm: f64,
    pub bending_moment_nm: f64,
    pub equivalent_torsional_moment_nm: f64,
    pub equivalent_bending_moment_nm: f64,
    pub min_required_diameter_mm: f64,
    pub actual_shaft_diameter_mm: f64,
    pub shaft_torsional_stress_mpa: f64,
    pub shaft_bending_stress_mpa: f64,
    pub shaft_von_mises_stress_mpa: f64,
    pub shaft_safety_factor: f64,
    pub torsional_deflection_deg: f64,
    pub torsional_deflection_per_meter_deg: f64,
    pub key_dimensions: Din6885KeyDimensions,
    pub key_length_mm: f64,
    pub key_shear_stress_mpa: f64,
    pub key_shear_safety_factor: f64,
    pub hub_contact_pressure_mpa: f64,
    pub hub_pressure_safety_factor: f64,
    pub is_design_adequate: bool,
    pub summary_notes: Vec<String>,
}

/// Solves transmission shaft sizing and DIN 6885-1 parallel key verification.
pub fn solve_shaft_and_key(prob: &ShaftKeyProblem) -> Result<ShaftKeyResult, String> {
    if prob.transmitted_power_kw <= 0.0 || prob.rotational_speed_rpm <= 0.0 {
        return Err("Transmitted power and rotational speed must be strictly positive.".to_string());
    }

    let fos = prob.safety_factor.unwrap_or(2.0).max(1.0);
    let kb = prob.shock_factor_kb.unwrap_or(1.5).max(1.0);
    let kt = prob.shock_factor_kt.unwrap_or(1.0).max(1.0);

    // T = 9548.8 * P_kW / n_rpm [N·m]
    let torque_nm = (9548.8 * prob.transmitted_power_kw) / prob.rotational_speed_rpm;
    let mb_nm = prob.bending_moment_nm.abs();

    // Equivalent moments per ASME B106.1M / Guest Tresca:
    // Te = sqrt((Kb * Mb)^2 + (Kt * T)^2)
    let te_nm = ((kb * mb_nm).powi(2) + (kt * torque_nm).powi(2)).sqrt();
    // Me = 0.5 * (Kb * Mb + Te)
    let me_nm = 0.5 * (kb * mb_nm + te_nm);

    // Allowable torsional shear stress:
    // Conservative industrial guideline: tau_perm = 0.18 * UTS or 0.30 * Yield
    let yield_str = prob.shaft_material.yield_strength_mpa();
    let uts_str = prob.shaft_material.ultimate_strength_mpa();
    let tau_perm_mpa = (0.30 * yield_str).min(0.18 * uts_str) / (fos / 1.5);

    // Minimum required diameter:
    // d_min = (16 * Te / (pi * tau_perm))^(1/3)
    // Convert Te from N·m to N·mm (* 1000)
    let te_nmm = te_nm * 1000.0;
    let d_min_mm = (16.0 * te_nmm / (std::f64::consts::PI * tau_perm_mpa)).cbrt();

    // Nominal selected shaft diameter:
    let actual_d_mm = prob.selected_shaft_diameter_mm.unwrap_or(d_min_mm).max(d_min_mm);

    // Actual stresses:
    // tau = 16 * T / (pi * d^3)
    let t_nmm = torque_nm * 1000.0;
    let tau_actual_mpa = 16.0 * t_nmm / (std::f64::consts::PI * actual_d_mm.powi(3));
    // sigma_b = 32 * Mb / (pi * d^3)
    let mb_nmm = mb_nm * 1000.0;
    let sigma_b_actual_mpa = 32.0 * mb_nmm / (std::f64::consts::PI * actual_d_mm.powi(3));
    // von Mises: sigma_vm = sqrt(sigma_b^2 + 3 * tau^2)
    let sigma_vm_mpa = (sigma_b_actual_mpa.powi(2) + 3.0 * tau_actual_mpa.powi(2)).sqrt();
    let shaft_fos = yield_str / sigma_vm_mpa.max(1e-3);

    // Torsional angular deflection:
    // theta_rad = 32 * T * L / (pi * G * d^4)
    let g_mpa = prob.shaft_material.shear_modulus_gpa() * 1000.0;
    let l_mm = prob.shaft_length_m * 1000.0;
    let theta_rad = (32.0 * t_nmm * l_mm) / (std::f64::consts::PI * g_mpa * actual_d_mm.powi(4));
    let theta_deg = theta_rad.to_degrees();
    let theta_deg_per_meter = theta_deg / prob.shaft_length_m.max(0.01);

    // Parallel Key sizing per DIN 6885-1:
    let key_dims = lookup_din6885_key(actual_d_mm);
    let key_len_mm = prob.key_length_mm.unwrap_or_else(|| (1.5 * actual_d_mm).clamp(20.0, 150.0));
    // Effective key shear length (subtract rounded ends if Type A key: Leff = L - b)
    let leff_mm = (key_len_mm - key_dims.key_width_b_mm).max(key_len_mm * 0.75);

    // Key shear stress:
    // F_tangential = 2 * T / d
    // tau_key = F_tangential / (b * Leff) = 2 * T / (d * b * Leff)
    let f_tang_n = (2.0 * t_nmm) / actual_d_mm;
    let tau_key_mpa = f_tang_n / (key_dims.key_width_b_mm * leff_mm);
    // Standard key steel C45 has yield ~310 MPa, tau_perm = 0.577 * Yield / Fos
    let key_yield_mpa = 310.0;
    let key_tau_perm = 0.577 * key_yield_mpa / fos;
    let key_shear_fos = key_tau_perm / tau_key_mpa.max(1e-3);

    // Hub contact pressure:
    // p = F_tangential / (t2 * Leff) = 2 * T / (d * t2 * Leff)
    let p_hub_mpa = f_tang_n / (key_dims.hub_keyway_depth_t2_mm * leff_mm);
    let p_perm_hub = prob.hub_material.permissible_contact_pressure_mpa();
    let hub_pressure_fos = p_perm_hub / p_hub_mpa.max(1e-3);

    let is_ok = shaft_fos >= fos && key_shear_fos >= 1.0 && hub_pressure_fos >= 1.0;

    let notes = vec![
        format!("Shaft Design: '{}' (Power = {:.1} kW @ {:.0} RPM -> Torque = {:.1} N·m)", prob.shaft_name, prob.transmitted_power_kw, prob.rotational_speed_rpm, torque_nm),
        format!("Diameter: Min Required = {:.1} mm, Actual = {:.1} mm (von Mises Stress = {:.1} MPa, FoS = {:.2})", d_min_mm, actual_d_mm, sigma_vm_mpa, shaft_fos),
        format!("Torsional Deflection: Total = {:.3}°, Specific = {:.3}°/m (Criterion <= 0.25°/m for precision, <= 1.0°/m general)", theta_deg, theta_deg_per_meter),
        format!("DIN 6885-1 Key: {:.0}x{:.0} mm (L = {:.0} mm, Leff = {:.1} mm)", key_dims.key_width_b_mm, key_dims.key_height_h_mm, key_len_mm, leff_mm),
        format!("Key Stress: Shear = {:.1} MPa (FoS = {:.2}), Hub Contact Pressure = {:.1} MPa / Permissible {:.1} MPa (FoS = {:.2})", tau_key_mpa, key_shear_fos, p_hub_mpa, p_perm_hub, hub_pressure_fos),
    ];

    Ok(ShaftKeyResult {
        shaft_name: prob.shaft_name.clone(),
        torque_nm,
        bending_moment_nm: mb_nm,
        equivalent_torsional_moment_nm: te_nm,
        equivalent_bending_moment_nm: me_nm,
        min_required_diameter_mm: d_min_mm,
        actual_shaft_diameter_mm: actual_d_mm,
        shaft_torsional_stress_mpa: tau_actual_mpa,
        shaft_bending_stress_mpa: sigma_b_actual_mpa,
        shaft_von_mises_stress_mpa: sigma_vm_mpa,
        shaft_safety_factor: shaft_fos,
        torsional_deflection_deg: theta_deg,
        torsional_deflection_per_meter_deg: theta_deg_per_meter,
        key_dimensions: key_dims,
        key_length_mm: key_len_mm,
        key_shear_stress_mpa: tau_key_mpa,
        key_shear_safety_factor: key_shear_fos,
        hub_contact_pressure_mpa: p_hub_mpa,
        hub_pressure_safety_factor: hub_pressure_fos,
        is_design_adequate: is_ok,
        summary_notes: notes,
    })
}

/// Simulation runner adapter for transmission shaft and keyway.
pub fn solve_shaft_keys_sim(prob: &ShaftKeyProblem) -> Result<SimulationResult, String> {
    let sol = solve_shaft_and_key(prob)?;

    let mut metrics = HashMap::new();
    metrics.insert("torque_nm".to_string(), sol.torque_nm);
    metrics.insert("min_required_diameter_mm".to_string(), sol.min_required_diameter_mm);
    metrics.insert("actual_shaft_diameter_mm".to_string(), sol.actual_shaft_diameter_mm);
    metrics.insert("shaft_von_mises_stress_mpa".to_string(), sol.shaft_von_mises_stress_mpa);
    metrics.insert("shaft_safety_factor".to_string(), sol.shaft_safety_factor);
    metrics.insert("torsional_deflection_deg".to_string(), sol.torsional_deflection_deg);
    metrics.insert("key_shear_stress_mpa".to_string(), sol.key_shear_stress_mpa);
    metrics.insert("hub_contact_pressure_mpa".to_string(), sol.hub_contact_pressure_mpa);

    let mut conv = ConvergenceMonitor::new(1e-6);
    conv.record(1, sol.min_required_diameter_mm, 1, 1e-4);

    let val = AnalyticalComparison::assess(
        "Transmission Shaft Diameter (mm)",
        sol.actual_shaft_diameter_mm,
        sol.min_required_diameter_mm,
    );

    Ok(SimulationResult {
        problem_id: "shaft-din6885-sizing".to_string(),
        engine_id: "atlas-shafts-keys".to_string(),
        status: SimulationStatus::Converged,
        execution_time_ms: 1,
        scalar_metrics: metrics,
        convergence: conv,
        validation: Some(val),
        provenance: SimulationProvenance::new("atlas-shafts-keys", "0.1.0"),
        logs: sol.summary_notes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_industrial_gearbox_countershaft_sizing() -> Result<(), String> {
        let prob = ShaftKeyProblem::default();
        let sol = solve_shaft_and_key(&prob)?;

        // T = 9548.8 * 45 / 1450 = 296.34 N·m
        assert!((sol.torque_nm - 296.34).abs() < 1.0);
        // Equivalent Te = sqrt((1.5 * 350)^2 + (1.2 * 296.34)^2) = sqrt(525^2 + 355.6^2) = 634.1 N·m
        assert!((sol.equivalent_torsional_moment_nm - 634.1).abs() < 5.0);
        // Required diameter ~ 28 - 35 mm (29.8 mm)
        assert!(sol.min_required_diameter_mm > 25.0 && sol.min_required_diameter_mm < 40.0);
        // Actual shaft 45 mm has safety factor > 2.0
        assert!(sol.shaft_safety_factor > 2.0);
        // DIN 6885 key for 45mm shaft is 14x9
        assert_eq!(sol.key_dimensions.key_width_b_mm, 14.0);
        assert_eq!(sol.key_dimensions.key_height_h_mm, 9.0);
        // Deflection < 0.50 deg (0.32 deg, 0.53 deg/m <= 1.0 deg/m)
        assert!(sol.torsional_deflection_deg < 0.50);
        assert!(sol.is_design_adequate);

        let sim = solve_shaft_keys_sim(&prob)?;
        assert_eq!(sim.status, SimulationStatus::Converged);
        Ok(())
    }
}
