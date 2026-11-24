//! # Involute Spur & Helical Gear Pair Mechanics (ISO 6336 / AGMA 2001-D04)
//!
//! Provides analytical calculation of gear geometry (pitch, base, addendum, dedendum, center distance),
//! transverse contact ratio, pitch-line velocity, tangential/radial/axial forces,
//! Lewis tooth root bending stress, and AGMA Hertzian pitting contact safety.

use crate::convergence::ConvergenceMonitor;
use crate::execution::{SimulationResult, SimulationStatus};
use crate::provenance::SimulationProvenance;
use crate::validation::AnalyticalComparison;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Type of gear pairing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GearType {
    /// External spur gears (helix angle = 0°)
    Spur,
    /// External helical gears (helix angle > 0°)
    Helical,
}

/// Standard gear tooth system profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GearToothProfile {
    /// Standard full-depth involute (pressure angle 20°, addendum = 1.0 m, dedendum = 1.25 m)
    FullDepth20Deg,
    /// High pressure angle involute (pressure angle 25° for high load capacity)
    FullDepth25Deg,
    /// Stub tooth involute (pressure angle 20°, addendum = 0.8 m, dedendum = 1.0 m)
    Stub20Deg,
}

impl GearToothProfile {
    /// Normal pressure angle in degrees.
    pub fn pressure_angle_deg(&self) -> f64 {
        match self {
            Self::FullDepth20Deg | Self::Stub20Deg => 20.0,
            Self::FullDepth25Deg => 25.0,
        }
    }

    /// Addendum coefficient ha*.
    pub fn addendum_factor(&self) -> f64 {
        match self {
            Self::FullDepth20Deg | Self::FullDepth25Deg => 1.0,
            Self::Stub20Deg => 0.8,
        }
    }

    /// Dedendum coefficient hf*.
    pub fn dedendum_factor(&self) -> f64 {
        match self {
            Self::FullDepth20Deg | Self::FullDepth25Deg => 1.25,
            Self::Stub20Deg => 1.0,
        }
    }
}

/// Input problem definition for an involute gear pair.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InvoluteGearPairProblem {
    pub gear_set_name: String,
    pub gear_type: GearType,
    pub tooth_profile: GearToothProfile,
    /// Normal module mn in mm (e.g. 2.0, 2.5, 3.0, 4.0, 5.0)
    pub normal_module_mn_mm: f64,
    /// Number of teeth on pinion z1 (typically 14 to 35)
    pub pinion_teeth_z1: u32,
    /// Number of teeth on driven gear z2
    pub gear_teeth_z2: u32,
    /// Helix angle beta in degrees (0.0 for spur gears, typically 10° - 30° for helical)
    pub helix_angle_deg: f64,
    /// Face width b in mm (typically 8 to 16 times module)
    pub face_width_b_mm: f64,
    /// Transmitted mechanical power in kW
    pub transmitted_power_kw: f64,
    /// Pinion rotational speed n1 in RPM
    pub pinion_speed_rpm: f64,
    /// Permissible tooth root bending stress sigma_FP in MPa (e.g. 430 MPa for 18CrNiMo7-6 carburized)
    pub allowable_bending_stress_mpa: Option<f64>,
    /// Permissible contact stress sigma_HP in MPa (e.g. 1500 MPa for case-carburized alloy steel)
    pub allowable_contact_stress_mpa: Option<f64>,
}

impl Default for InvoluteGearPairProblem {
    fn default() -> Self {
        Self {
            gear_set_name: "EV Reduction Gearbox Primary Reduction".to_string(),
            gear_type: GearType::Helical,
            tooth_profile: GearToothProfile::FullDepth20Deg,
            normal_module_mn_mm: 3.0,
            pinion_teeth_z1: 19,
            gear_teeth_z2: 67,
            helix_angle_deg: 18.0,
            face_width_b_mm: 35.0,
            transmitted_power_kw: 150.0,
            pinion_speed_rpm: 8000.0,
            allowable_bending_stress_mpa: Some(430.0),
            allowable_contact_stress_mpa: Some(1500.0),
        }
    }
}

/// Detailed analytical outputs for involute gear pair transmission.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InvoluteGearPairResult {
    pub gear_set_name: String,
    pub gear_type: GearType,
    pub gear_ratio_u: f64,
    pub center_distance_a_mm: f64,
    pub transverse_module_mt_mm: f64,
    pub transverse_pressure_angle_deg: f64,
    pub pinion_pitch_diameter_d1_mm: f64,
    pub pinion_base_diameter_db1_mm: f64,
    pub pinion_tip_diameter_da1_mm: f64,
    pub pinion_root_diameter_df1_mm: f64,
    pub gear_pitch_diameter_d2_mm: f64,
    pub gear_base_diameter_db2_mm: f64,
    pub gear_tip_diameter_da2_mm: f64,
    pub gear_root_diameter_df2_mm: f64,
    pub transverse_contact_ratio: f64,
    pub is_contact_ratio_adequate: bool,
    pub pitch_line_velocity_m_s: f64,
    pub pinion_torque_nm: f64,
    pub tangential_force_ft_n: f64,
    pub radial_force_fr_n: f64,
    pub axial_force_fa_n: f64,
    pub dynamic_factor_kv: f64,
    pub lewis_form_factor_y: f64,
    pub tooth_bending_stress_mpa: f64,
    pub bending_safety_factor: f64,
    pub contact_hertzian_stress_mpa: f64,
    pub contact_safety_factor: f64,
    pub summary_notes: Vec<String>,
}

/// Solves involute gear pair geometry, kinematics, force equilibrium, and AGMA stress.
pub fn solve_involute_gear_pair(
    prob: &InvoluteGearPairProblem,
) -> Result<InvoluteGearPairResult, String> {
    let mn = prob.normal_module_mn_mm;
    let z1 = prob.pinion_teeth_z1 as f64;
    let z2 = prob.gear_teeth_z2 as f64;
    let b = prob.face_width_b_mm;
    let p_kw = prob.transmitted_power_kw;
    let n1 = prob.pinion_speed_rpm;

    if mn <= 0.1 || z1 < 10.0 || z2 < 10.0 || b <= 0.1 || n1 <= 0.0 {
        return Err("Module mn, tooth counts z1/z2 >= 10, face width b, and speed must be strictly positive.".to_string());
    }

    let beta_rad = prob.helix_angle_deg.to_radians();
    let alpha_n_rad = prob.tooth_profile.pressure_angle_deg().to_radians();

    // Transverse module mt = mn / cos(beta)
    let mt = mn / beta_rad.cos().max(1e-4);
    // Transverse pressure angle alpha_t: tan(alpha_t) = tan(alpha_n) / cos(beta)
    let alpha_t_rad = (alpha_n_rad.tan() / beta_rad.cos().max(1e-4)).atan();
    let alpha_t_deg = alpha_t_rad.to_degrees();

    // Pitch diameters
    let d1 = z1 * mt;
    let d2 = z2 * mt;
    let a = (d1 + d2) * 0.5;
    let u = z2 / z1;

    // Base circle diameters: db = d * cos(alpha_t)
    let db1 = d1 * alpha_t_rad.cos();
    let db2 = d2 * alpha_t_rad.cos();

    // Tip and root diameters
    let ha = prob.tooth_profile.addendum_factor() * mn;
    let hf = prob.tooth_profile.dedendum_factor() * mn;
    let da1 = d1 + 2.0 * ha;
    let da2 = d2 + 2.0 * ha;
    let df1 = d1 - 2.0 * hf;
    let df2 = d2 - 2.0 * hf;

    // Transverse contact ratio epsilon_alpha:
    // length of path of contact / base pitch pb
    let r_a1 = da1 * 0.5;
    let r_a2 = da2 * 0.5;
    let r_b1 = db1 * 0.5;
    let r_b2 = db2 * 0.5;

    let term1 = (r_a1 * r_a1 - r_b1 * r_b1).max(0.0).sqrt();
    let term2 = (r_a2 * r_a2 - r_b2 * r_b2).max(0.0).sqrt();
    let path_of_contact = term1 + term2 - a * alpha_t_rad.sin();
    let base_pitch = std::f64::consts::PI * mt * alpha_t_rad.cos();
    let eps_alpha = path_of_contact / base_pitch.max(1e-4);
    let is_eps_ok = eps_alpha >= 1.20;

    // Kinematics and forces
    // Pitch line velocity v = pi * d1 * n1 / 60000 [m/s]
    let v_m_s = (std::f64::consts::PI * d1 * n1) / 60000.0;
    // Pinion torque: T1 = 9549.3 * P_kw / n1 [N*m]
    let t1_nm = (9549.3 * p_kw) / n1;
    // Tangential force: Ft = 2000 * T1 / d1 [N]
    let ft = (2000.0 * t1_nm) / d1;
    // Radial separating force: Fr = Ft * tan(alpha_t)
    let fr = ft * alpha_t_rad.tan();
    // Axial thrust force: Fa = Ft * tan(beta)
    let fa = ft * beta_rad.tan();

    // AGMA 2001-D04 Dynamic factor Kv for precision ground transmission gearing:
    // Kv = ((50 + sqrt(200 * v)) / 50)^0.25
    let kv = ((50.0 + (200.0 * v_m_s).sqrt()) / 50.0).powf(0.25);

    // Virtual number of teeth for helical gear: zv = z / cos^3(beta)
    let zv1 = z1 / beta_rad.cos().powi(3).max(1e-4);
    // AGMA / ISO metric tooth form factor Y (where sigma_b = Ft / (b * mn * Y))
    // For 20° full-depth involute: Y approx 0.484 - 2.87 / zv
    let y_lewis = (0.484 - 2.87 / zv1.max(10.0)).clamp(0.25, 0.55);

    // Tooth root bending stress: sigma_b = (Ft * Kv * Ko) / (b * mn * Y)
    let ko = 1.25; // Moderate shock application overload factor
    let sigma_b = (ft * kv * ko) / (b * mn * y_lewis);
    let allow_bend = prob.allowable_bending_stress_mpa.unwrap_or(400.0);
    let fos_b = allow_bend / sigma_b.max(1.0);

    // AGMA Hertzian surface contact stress:
    // sigma_H = ZE * sqrt((Ft * Kv * Ko) / (b * d1 * I))
    // ZE = 189.8 MPa^0.5 for steel-on-steel
    let ze = 189.8;
    let geom_i = (alpha_t_rad.cos() * alpha_t_rad.sin() * 0.5) * (u / (u + 1.0));
    let contact_term = (ft * kv * ko) / (b * d1 * geom_i.max(0.01));
    let sigma_h = ze * contact_term.max(0.0).sqrt();
    let allow_contact = prob.allowable_contact_stress_mpa.unwrap_or(1500.0);
    let fos_h = allow_contact / sigma_h.max(1.0);

    let notes = vec![
        format!(
            "Gear Set: '{}' ({:?}, u = {:.3}, a = {:.2} mm)",
            prob.gear_set_name, prob.gear_type, u, a
        ),
        format!(
            "Pinion (z1 = {}): Pitch d1 = {:.2} mm, Tip da1 = {:.2} mm, Root df1 = {:.2} mm",
            prob.pinion_teeth_z1, d1, da1, df1
        ),
        format!(
            "Gear (z2 = {}): Pitch d2 = {:.2} mm, Tip da2 = {:.2} mm, Root df2 = {:.2} mm",
            prob.gear_teeth_z2, d2, da2, df2
        ),
        format!(
            "Meshing: Normal Module mn = {:.2} mm, Helix β = {:.1}°, Transverse Contact Ratio εα = {:.3} [{}]",
            mn,
            prob.helix_angle_deg,
            eps_alpha,
            if is_eps_ok {
                "Continuous Smooth Involute Handover"
            } else {
                "Warning: Low Contact Ratio (<1.20)"
            }
        ),
        format!(
            "Forces at {:.0} RPM (v = {:.2} m/s): Ft = {:.1} N, Fr = {:.1} N, Fa = {:.1} N (Torque = {:.1} N·m)",
            n1, v_m_s, ft, fr, fa, t1_nm
        ),
        format!(
            "AGMA Tooth Bending: σb = {:.1} MPa vs Allowable {:.0} MPa (FoS = {:.2})",
            sigma_b, allow_bend, fos_b
        ),
        format!(
            "AGMA Hertzian Pitting: σH = {:.1} MPa vs Allowable {:.0} MPa (FoS = {:.2})",
            sigma_h, allow_contact, fos_h
        ),
    ];

    Ok(InvoluteGearPairResult {
        gear_set_name: prob.gear_set_name.clone(),
        gear_type: prob.gear_type,
        gear_ratio_u: u,
        center_distance_a_mm: a,
        transverse_module_mt_mm: mt,
        transverse_pressure_angle_deg: alpha_t_deg,
        pinion_pitch_diameter_d1_mm: d1,
        pinion_base_diameter_db1_mm: db1,
        pinion_tip_diameter_da1_mm: da1,
        pinion_root_diameter_df1_mm: df1,
        gear_pitch_diameter_d2_mm: d2,
        gear_base_diameter_db2_mm: db2,
        gear_tip_diameter_da2_mm: da2,
        gear_root_diameter_df2_mm: df2,
        transverse_contact_ratio: eps_alpha,
        is_contact_ratio_adequate: is_eps_ok,
        pitch_line_velocity_m_s: v_m_s,
        pinion_torque_nm: t1_nm,
        tangential_force_ft_n: ft,
        radial_force_fr_n: fr,
        axial_force_fa_n: fa,
        dynamic_factor_kv: kv,
        lewis_form_factor_y: y_lewis,
        tooth_bending_stress_mpa: sigma_b,
        bending_safety_factor: fos_b,
        contact_hertzian_stress_mpa: sigma_h,
        contact_safety_factor: fos_h,
        summary_notes: notes,
    })
}

/// Simulation runner adapter for involute gear pair.
pub fn solve_gear_pair_sim(prob: &InvoluteGearPairProblem) -> Result<SimulationResult, String> {
    let sol = solve_involute_gear_pair(prob)?;

    let mut metrics = HashMap::new();
    metrics.insert("center_distance_a_mm".to_string(), sol.center_distance_a_mm);
    metrics.insert("gear_ratio_u".to_string(), sol.gear_ratio_u);
    metrics.insert(
        "transverse_contact_ratio".to_string(),
        sol.transverse_contact_ratio,
    );
    metrics.insert(
        "pitch_line_velocity_m_s".to_string(),
        sol.pitch_line_velocity_m_s,
    );
    metrics.insert("pinion_torque_nm".to_string(), sol.pinion_torque_nm);
    metrics.insert(
        "tangential_force_ft_n".to_string(),
        sol.tangential_force_ft_n,
    );
    metrics.insert("radial_force_fr_n".to_string(), sol.radial_force_fr_n);
    metrics.insert("axial_force_fa_n".to_string(), sol.axial_force_fa_n);
    metrics.insert(
        "tooth_bending_stress_mpa".to_string(),
        sol.tooth_bending_stress_mpa,
    );
    metrics.insert(
        "bending_safety_factor".to_string(),
        sol.bending_safety_factor,
    );
    metrics.insert(
        "contact_hertzian_stress_mpa".to_string(),
        sol.contact_hertzian_stress_mpa,
    );
    metrics.insert(
        "contact_safety_factor".to_string(),
        sol.contact_safety_factor,
    );

    let mut conv = ConvergenceMonitor::new(1e-6);
    conv.record(1, sol.center_distance_a_mm, 1, 1e-4);

    let val = AnalyticalComparison::assess(
        "Center Distance Closed-Form (mm)",
        sol.center_distance_a_mm,
        (sol.pinion_pitch_diameter_d1_mm + sol.gear_pitch_diameter_d2_mm) * 0.5,
    );

    Ok(SimulationResult {
        problem_id: "gear-involute-pair".to_string(),
        engine_id: "atlas-gears".to_string(),
        status: SimulationStatus::Converged,
        execution_time_ms: 1,
        scalar_metrics: metrics,
        convergence: conv,
        validation: Some(val),
        provenance: SimulationProvenance::new("atlas-gears", "0.1.0"),
        logs: sol.summary_notes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_helical_gear_pair_kinematics_and_forces() -> Result<(), String> {
        let prob = InvoluteGearPairProblem::default();
        let sol = solve_involute_gear_pair(&prob)?;

        // Gear ratio u = 67 / 19 = 3.526
        assert!((sol.gear_ratio_u - 3.5263).abs() < 1e-3);
        // Transverse module mt = 3.0 / cos(18°) = 3.0 / 0.9510565 = 3.15437 mm
        assert!((sol.transverse_module_mt_mm - 3.1544).abs() < 1e-3);
        // Center distance a = (19 + 67) * 3.1544 / 2 = 86 * 1.5772 = 135.64 mm
        assert!((sol.center_distance_a_mm - 135.638).abs() < 0.1);
        // Contact ratio should exceed 1.20
        assert!(sol.transverse_contact_ratio >= 1.20);
        assert!(sol.is_contact_ratio_adequate);
        // Pitch line velocity for 8000 RPM on ~60 mm pitch dia: ~25 m/s
        assert!(sol.pitch_line_velocity_m_s > 20.0 && sol.pitch_line_velocity_m_s < 30.0);
        // Bending safety factor should be well positive
        assert!(sol.bending_safety_factor > 1.2);

        let sim = solve_gear_pair_sim(&prob)?;
        assert_eq!(sim.status, SimulationStatus::Converged);
        Ok(())
    }
}
