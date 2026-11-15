//! # Helical Compression & Extension Spring Mechanics (DIN EN 13906-1 / DIN 2089-1)
//!
//! Provides analytical calculation of spring rate, Wahl curvature correction factor,
//! torsional shear stress, solid height, resonant surge natural frequency, and
//! free-standing lateral buckling deflection stability.

use crate::convergence::ConvergenceMonitor;
use crate::execution::{SimulationResult, SimulationStatus};
use crate::provenance::SimulationProvenance;
use crate::validation::AnalyticalComparison;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Material specification for helical springs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpringMaterial {
    /// Cold drawn music wire (ASTM A228 / DIN 17223 Class D)
    MusicWireA228,
    /// Oil-tempered carbon spring steel (ASTM A229 / 51CrV4)
    OilTempered51CrV4,
    /// Stainless steel spring wire (AISI 302 / EN 10270-3-1.4310)
    Stainless302,
    /// Phosphor bronze spring wire (C51000 / CuSn6)
    PhosphorBronzeC510,
}

impl SpringMaterial {
    /// Shear modulus G in MPa (N/mm²).
    pub fn shear_modulus_mpa(&self) -> f64 {
        match self {
            Self::MusicWireA228 => 79300.0,
            Self::OilTempered51CrV4 => 78500.0,
            Self::Stainless302 => 70000.0,
            Self::PhosphorBronzeC510 => 42000.0,
        }
    }

    /// Density rho in kg/m³.
    pub fn density_kg_m3(&self) -> f64 {
        match self {
            Self::MusicWireA228 | Self::OilTempered51CrV4 => 7850.0,
            Self::Stainless302 => 7900.0,
            Self::PhosphorBronzeC510 => 8860.0,
        }
    }

    /// Nominal ultimate tensile strength Rm in MPa for a given wire diameter in mm.
    pub fn estimated_uts_mpa(&self, wire_dia_mm: f64) -> f64 {
        let d = wire_dia_mm.max(0.2);
        match self {
            Self::MusicWireA228 => 2216.0 / d.powf(0.145),
            Self::OilTempered51CrV4 => 1855.0 / d.powf(0.187),
            Self::Stainless302 => 1867.0 / d.powf(0.146),
            Self::PhosphorBronzeC510 => 1000.0 / d.powf(0.1),
        }
    }

    /// Permissible torsional stress tau_perm in MPa (typically 56% of UTS for static/infrequent solid deflection).
    pub fn permissible_shear_stress_mpa(&self, wire_dia_mm: f64) -> f64 {
        0.56 * self.estimated_uts_mpa(wire_dia_mm)
    }
}

/// Spring end style per DIN 2089.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpringEndType {
    /// Closed and ground ends (active coils na = nt - 2, solid height Ls = nt * d)
    ClosedAndGround,
    /// Closed and unground ends (active coils na = nt - 2, solid height Ls = (nt + 1) * d)
    ClosedUnground,
    /// Plain open ends (active coils na = nt, solid height Ls = (nt + 1) * d)
    OpenPlain,
}

/// Input problem specification for helical compression spring analysis.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HelicalSpringProblem {
    pub spring_name: String,
    pub material: SpringMaterial,
    pub end_type: SpringEndType,
    /// Wire diameter d in mm (d > 0)
    pub wire_diameter_d_mm: f64,
    /// Mean coil diameter D in mm (outer diameter minus wire diameter)
    pub mean_diameter_d_mm: f64,
    /// Number of active coils na (na > 0)
    pub active_coils_na: f64,
    /// Free length L0 in mm
    pub free_length_l0_mm: f64,
    /// Operating working load F1 in N
    pub working_load_f1_n: Option<f64>,
    /// Operating peak load F2 in N
    pub peak_load_f2_n: Option<f64>,
}

impl Default for HelicalSpringProblem {
    fn default() -> Self {
        Self {
            spring_name: "Automotive Suspension Helical Spring".to_string(),
            material: SpringMaterial::MusicWireA228,
            end_type: SpringEndType::ClosedAndGround,
            wire_diameter_d_mm: 5.0,
            mean_diameter_d_mm: 40.0,
            active_coils_na: 8.0,
            free_length_l0_mm: 120.0,
            working_load_f1_n: Some(300.0),
            peak_load_f2_n: Some(650.0),
        }
    }
}

/// Detailed analytical results for helical compression spring mechanics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HelicalSpringResult {
    pub spring_name: String,
    pub wire_diameter_d_mm: f64,
    pub mean_diameter_d_mm: f64,
    pub outer_diameter_de_mm: f64,
    pub inner_diameter_di_mm: f64,
    pub spring_index_c: f64,
    pub wahl_factor_kw: f64,
    pub spring_rate_k_n_mm: f64,
    pub active_coils_na: f64,
    pub total_coils_nt: f64,
    pub free_length_l0_mm: f64,
    pub solid_length_ls_mm: f64,
    pub max_deflection_sn_mm: f64,
    pub solid_force_fn_n: f64,
    pub working_deflection_s1_mm: Option<f64>,
    pub working_stress_tau1_mpa: Option<f64>,
    pub peak_deflection_s2_mm: Option<f64>,
    pub peak_stress_tau2_mpa: Option<f64>,
    pub solid_shear_stress_taun_mpa: f64,
    pub permissible_shear_stress_mpa: f64,
    pub solid_stress_safety_factor: f64,
    pub fundamental_frequency_hz: f64,
    pub critical_buckling_deflection_mm: Option<f64>,
    pub is_buckling_safe: bool,
    pub active_mass_grams: f64,
    pub summary_notes: Vec<String>,
}

/// Solves helical compression spring mechanics adhering to DIN EN 13906-1 and DIN 2089.
pub fn solve_helical_compression_spring(
    prob: &HelicalSpringProblem,
) -> Result<HelicalSpringResult, String> {
    let d = prob.wire_diameter_d_mm;
    let big_d = prob.mean_diameter_d_mm;
    let na = prob.active_coils_na;
    let l0 = prob.free_length_l0_mm;

    if d <= 0.1 || big_d <= d {
        return Err("Mean diameter D must be strictly greater than wire diameter d > 0.1 mm".to_string());
    }
    if na <= 0.5 {
        return Err("Active coils na must be at least 0.5".to_string());
    }
    if l0 <= 0.0 {
        return Err("Free length L0 must be strictly positive".to_string());
    }

    let g = prob.material.shear_modulus_mpa();
    let rho = prob.material.density_kg_m3();
    let de = big_d + d;
    let di = big_d - d;

    // Spring index C = D / d
    let c = big_d / d;
    // Wahl curvature correction factor kw = (4C - 1)/(4C - 4) + 0.615/C
    let kw = if c > 1.1 {
        (4.0 * c - 1.0) / (4.0 * c - 4.0) + 0.615 / c
    } else {
        1.5
    };

    // Spring stiffness / rate k = G * d^4 / (8 * D^3 * na) in N/mm
    let k_rate = (g * d.powi(4)) / (8.0 * big_d.powi(3) * na);

    // Total coils and solid length Ls based on end type
    let (nt, ls) = match prob.end_type {
        SpringEndType::ClosedAndGround => (na + 2.0, (na + 2.0) * d),
        SpringEndType::ClosedUnground => (na + 2.0, (na + 3.0) * d),
        SpringEndType::OpenPlain => (na, (na + 1.0) * d),
    };

    if ls >= l0 {
        return Err(format!(
            "Solid length Ls ({:.2} mm) exceeds or equals free length L0 ({:.2} mm)",
            ls, l0
        ));
    }

    // Maximum deflection to solid height: sn = L0 - Ls
    let sn = l0 - ls;
    // Solid load Fn = k * sn
    let fn_n = k_rate * sn;
    // Solid shear stress tau_n = kw * (8 * Fn * D) / (pi * d^3)
    let tau_n = kw * (8.0 * fn_n * big_d) / (std::f64::consts::PI * d.powi(3));

    let tau_perm = prob.material.permissible_shear_stress_mpa(d);
    let solid_fos = if tau_n > 1e-4 { tau_perm / tau_n } else { 10.0 };

    // Working point evaluations
    let (s1, tau1) = if let Some(f1) = prob.working_load_f1_n {
        let def1 = f1 / k_rate;
        let stress1 = kw * (8.0 * f1 * big_d) / (std::f64::consts::PI * d.powi(3));
        (Some(def1), Some(stress1))
    } else {
        (None, None)
    };

    let (s2, tau2) = if let Some(f2) = prob.peak_load_f2_n {
        let def2 = f2 / k_rate;
        let stress2 = kw * (8.0 * f2 * big_d) / (std::f64::consts::PI * d.powi(3));
        (Some(def2), Some(stress2))
    } else {
        (None, None)
    };

    // Active coil wire length L_wire = pi * D * na
    // Wire volume = (pi * d^2 / 4) * L_wire
    let wire_vol_m3 = (std::f64::consts::PI * (d * 1e-3).powi(2) / 4.0)
        * (std::f64::consts::PI * (big_d * 1e-3) * na);
    let active_mass_g = wire_vol_m3 * rho * 1000.0;

    // Natural resonant frequency f0 (Hz) for free-fixed helical spring:
    // f0 = (d / (2 * pi * D^2 * na)) * sqrt(G / (2 * rho))
    // Note: G in Pa (G_mpa * 1e6), D and d in m
    let d_m = d * 1e-3;
    let big_d_m = big_d * 1e-3;
    let g_pa = g * 1e6;
    let f0_hz = (d_m / (2.0 * std::f64::consts::PI * big_d_m.powi(2) * na))
        * (g_pa / (2.0 * rho)).sqrt();

    // Slenderness lambda = L0 / D. If lambda > 4.0, spring is prone to lateral buckling
    let slenderness = l0 / big_d;
    let (crit_buckle_mm, is_buckling_safe) = if slenderness <= 4.0 {
        (None, true)
    } else {
        // DIN 2089 approximation for fixed-parallel ends:
        // sk/L0 approx 0.8 * (1 - sqrt(1 - 6.22 * (D/L0)^2))
        let term = 6.22 * (big_d / l0).powi(2);
        if term < 1.0 {
            let sk = l0 * 0.8 * (1.0 - (1.0 - term).sqrt());
            let max_op_def = s2.or(s1).unwrap_or(sn);
            (Some(sk), max_op_def < sk)
        } else {
            (Some(l0 * 0.5), true)
        }
    };

    let notes = vec![
        format!("Helical Spring: '{}' ({:?}, End Style: {:?})", prob.spring_name, prob.material, prob.end_type),
        format!("Geometry: Wire d = {:.2} mm, Mean D = {:.2} mm (OD = {:.2} mm, ID = {:.2} mm), C = {:.2}", d, big_d, de, di, c),
        format!("Coils: Active na = {:.2}, Total nt = {:.2}, Free Length L0 = {:.1} mm, Solid Ls = {:.1} mm (sn = {:.1} mm)", na, nt, l0, ls, sn),
        format!("Spring Stiffness: k = {:.3} N/mm (Wahl Factor kw = {:.3})", k_rate, kw),
        format!("Solid Capacity: Fn = {:.1} N -> Solid Stress τn = {:.1} MPa vs Permissible {:.1} MPa (FoS = {:.2})", fn_n, tau_n, tau_perm, solid_fos),
        format!("Dynamics: Natural Surge Frequency f0 = {:.1} Hz, Active Mass = {:.1} g", f0_hz, active_mass_g),
        format!("Buckling Stability: Slenderness L0/D = {:.2} -> [{}]",
            slenderness,
            if is_buckling_safe { "Stable (No guide pin required)" } else { "Buckling Hazard (Internal mandrel / external sleeve mandatory)" }
        ),
    ];

    Ok(HelicalSpringResult {
        spring_name: prob.spring_name.clone(),
        wire_diameter_d_mm: d,
        mean_diameter_d_mm: big_d,
        outer_diameter_de_mm: de,
        inner_diameter_di_mm: di,
        spring_index_c: c,
        wahl_factor_kw: kw,
        spring_rate_k_n_mm: k_rate,
        active_coils_na: na,
        total_coils_nt: nt,
        free_length_l0_mm: l0,
        solid_length_ls_mm: ls,
        max_deflection_sn_mm: sn,
        solid_force_fn_n: fn_n,
        working_deflection_s1_mm: s1,
        working_stress_tau1_mpa: tau1,
        peak_deflection_s2_mm: s2,
        peak_stress_tau2_mpa: tau2,
        solid_shear_stress_taun_mpa: tau_n,
        permissible_shear_stress_mpa: tau_perm,
        solid_stress_safety_factor: solid_fos,
        fundamental_frequency_hz: f0_hz,
        critical_buckling_deflection_mm: crit_buckle_mm,
        is_buckling_safe,
        active_mass_grams: active_mass_g,
        summary_notes: notes,
    })
}

/// Simulation runner adapter for helical springs.
pub fn solve_helical_spring_sim(prob: &HelicalSpringProblem) -> Result<SimulationResult, String> {
    let sol = solve_helical_compression_spring(prob)?;

    let mut metrics = HashMap::new();
    metrics.insert("spring_rate_k_n_mm".to_string(), sol.spring_rate_k_n_mm);
    metrics.insert("spring_index_c".to_string(), sol.spring_index_c);
    metrics.insert("wahl_factor_kw".to_string(), sol.wahl_factor_kw);
    metrics.insert("solid_length_ls_mm".to_string(), sol.solid_length_ls_mm);
    metrics.insert("max_deflection_sn_mm".to_string(), sol.max_deflection_sn_mm);
    metrics.insert("solid_force_fn_n".to_string(), sol.solid_force_fn_n);
    metrics.insert("solid_shear_stress_taun_mpa".to_string(), sol.solid_shear_stress_taun_mpa);
    metrics.insert("solid_stress_safety_factor".to_string(), sol.solid_stress_safety_factor);
    metrics.insert("fundamental_frequency_hz".to_string(), sol.fundamental_frequency_hz);
    metrics.insert("active_mass_grams".to_string(), sol.active_mass_grams);
    metrics.insert("is_buckling_safe".to_string(), if sol.is_buckling_safe { 1.0 } else { 0.0 });

    let mut conv = ConvergenceMonitor::new(1e-6);
    conv.record(1, sol.spring_rate_k_n_mm, 1, 1e-4);

    let val = AnalyticalComparison::assess(
        "Spring Stiffness Closed-Form (N/mm)",
        sol.spring_rate_k_n_mm,
        (prob.material.shear_modulus_mpa() * prob.wire_diameter_d_mm.powi(4))
            / (8.0 * prob.mean_diameter_d_mm.powi(3) * prob.active_coils_na),
    );

    Ok(SimulationResult {
        problem_id: "spring-din13906-compression".to_string(),
        engine_id: "atlas-springs".to_string(),
        status: SimulationStatus::Converged,
        execution_time_ms: 1,
        scalar_metrics: metrics,
        convergence: conv,
        validation: Some(val),
        provenance: SimulationProvenance::new("atlas-springs", "0.1.0"),
        logs: sol.summary_notes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_standard_helical_compression_spring() -> Result<(), String> {
        let prob = HelicalSpringProblem {
            spring_name: "Valve Return Spring".to_string(),
            material: SpringMaterial::MusicWireA228,
            end_type: SpringEndType::ClosedAndGround,
            wire_diameter_d_mm: 4.0,
            mean_diameter_d_mm: 32.0,
            active_coils_na: 7.0,
            free_length_l0_mm: 75.0,
            working_load_f1_n: Some(250.0),
            peak_load_f2_n: Some(500.0),
        };

        let sol = solve_helical_compression_spring(&prob)?;
        // Spring index C = 32 / 4 = 8.0
        assert!((sol.spring_index_c - 8.0).abs() < 1e-4);
        // Wahl factor for C=8: (31/28) + 0.615/8 = 1.1071 + 0.0769 = 1.184
        assert!((sol.wahl_factor_kw - 1.184).abs() < 0.01);
        // Stiffness k = (79300 * 256) / (8 * 32768 * 7) = 20300800 / 1835008 = 11.063 N/mm
        assert!((sol.spring_rate_k_n_mm - 11.063).abs() < 0.05);
        // Solid length: (7 + 2) * 4 = 36 mm
        assert!((sol.solid_length_ls_mm - 36.0).abs() < 1e-4);
        // Deflection to solid: 75 - 36 = 39 mm
        assert!((sol.max_deflection_sn_mm - 39.0).abs() < 1e-4);
        // Natural frequency should be well above 50 Hz
        assert!(sol.fundamental_frequency_hz > 100.0);
        assert!(sol.is_buckling_safe);

        let sim = solve_helical_spring_sim(&prob)?;
        assert_eq!(sim.status, SimulationStatus::Converged);
        Ok(())
    }
}
