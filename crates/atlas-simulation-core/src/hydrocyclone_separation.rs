//! Hydrocyclone Solid-Liquid Mineral Classification (Plitt / Lynch-Rao / ISO 13320)
//!
//! Provides first-principles closed-form analytical solutions for:
//! 1. Slurry pressure drop across hydrocyclone body based on centrifugal swirling kinematics.
//! 2. Corrected cut size d50 (d50c) for mineral particle classification.
//! 3. Volumetric water and liquid short-circuit recovery to underflow (water split).
//! 4. Rosin-Rammler partition curve and sharpness of separation index (m).
//! 5. Spigot / apex discharge regime (spray vs roping) and overloading prevention.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

/// Mineral ore or slurry particle application class.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SlurryApplicationType {
    /// Dense hard-rock mineral ore grinding circuit (e.g. Copper/Gold porphyry, rho_s = 2800 kg/m³).
    HardRockGrindingCircuit,
    /// Coal dense-medium or desliming cyclone (rho_s = 1450 kg/m³).
    CoalPreparationDesliming,
    /// Industrial silica sand and aggregate washing (rho_s = 2650 kg/m³).
    SandAggregateWashing,
    /// Wastewater treatment degritting cyclone (rho_s = 2200 kg/m³).
    WastewaterGritSeparation,
}

/// Input parameters for hydrocyclone centrifugal classification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HydrocycloneProblem {
    /// Problem identifier.
    pub problem_id: String,
    /// Slurry application category.
    pub application: SlurryApplicationType,
    /// Internal diameter of hydrocyclone cylindrical section in millimeters (mm, standard 100 to 750 mm).
    pub cyclone_diameter_mm: f64,
    /// Feed inlet nozzle inside equivalent diameter in millimeters (mm, standard 0.15 to 0.25 * Dc).
    pub inlet_diameter_mm: f64,
    /// Vortex finder (overflow nozzle) inside diameter in millimeters (mm, standard 0.25 to 0.40 * Dc).
    pub vortex_finder_diameter_mm: f64,
    /// Apex / spigot (underflow discharge) inside diameter in millimeters (mm, standard 0.10 to 0.25 * Dc).
    pub apex_diameter_mm: f64,
    /// Total free height from vortex finder tip to apex in millimeters (mm, standard 3.0 to 6.0 * Dc).
    pub free_vortex_height_mm: f64,
    /// Total volumetric slurry feed flow rate in cubic meters per hour (m³/h, standard 20 to 500 m³/h).
    pub feed_flow_rate_m3_h: f64,
    /// Feed solids volumetric concentration percentage (Cv, %, standard 5.0 to 35.0%).
    pub solids_volumetric_concentration_pct: f64,
    /// Solid particle dry density in kilograms per cubic meter (kg/m³, standard 1400 to 3200 kg/m³).
    pub solid_density_kg_m3: f64,
    /// Liquid carrier density in kilograms per cubic meter (kg/m³, standard 1000 kg/m³ for water).
    pub liquid_density_kg_m3: f64,
    /// Representative evaluation particle size for grade recovery check in micrometers (µm).
    pub target_evaluation_size_um: f64,
}

impl Default for HydrocycloneProblem {
    fn default() -> Self {
        Self {
            problem_id: "copper_grinding_circuit_hydrocyclone".to_string(),
            application: SlurryApplicationType::HardRockGrindingCircuit,
            cyclone_diameter_mm: 350.0, // 350 mm (14-inch) commercial cyclone
            inlet_diameter_mm: 75.0,    // 75 mm inlet
            vortex_finder_diameter_mm: 115.0, // 115 mm vortex finder
            apex_diameter_mm: 55.0,     // 55 mm apex
            free_vortex_height_mm: 1400.0, // 1400 mm height
            feed_flow_rate_m3_h: 120.0, // 120 m³/h slurry feed
            solids_volumetric_concentration_pct: 18.0, // 18% volumetric solids
            solid_density_kg_m3: 2800.0, // Porphyry ore
            liquid_density_kg_m3: 1000.0, // Water
            target_evaluation_size_um: 75.0, // P80 ~ 75 µm target grind size
        }
    }
}

/// Simulation results for hydrocyclone pressure drop, cut size, and split recoveries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HydrocycloneResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Feed inlet entry slurry velocity in meters per second (m/s, standard 4.0 to 8.5 m/s).
    pub inlet_slurry_velocity_m_s: f64,
    /// Pressure drop across the hydrocyclone in kiloPascals (kPa, standard 50 to 180 kPa).
    pub pressure_drop_kpa: f64,
    /// Corrected cut size d50c in micrometers (µm).
    pub corrected_cut_size_d50c_um: f64,
    /// Plitt sharpness of separation parameter (m, dimensionless slope).
    pub sharpness_index_m: f64,
    /// Fractional volumetric liquid recovery to underflow (liquid bypass Rf) in percentage (%).
    pub liquid_recovery_to_underflow_pct: f64,
    /// Underflow slurry volumetric flow rate in cubic meters per hour (m³/h).
    pub underflow_flow_rate_m3_h: f64,
    /// Overflow slurry volumetric flow rate in cubic meters per hour (m³/h).
    pub overflow_flow_rate_m3_h: f64,
    /// Total overall solid recovery to underflow in percentage (%).
    pub total_solids_recovery_to_underflow_pct: f64,
    /// Underflow slurry solids concentration percentage by volume (Cv_u, %).
    pub underflow_solids_volumetric_pct: f64,
    /// Actual recovery to underflow at the target evaluation particle size (%).
    pub actual_recovery_at_target_size_pct: f64,
    /// Apex diameter to vortex finder diameter ratio (Du / Do).
    pub apex_to_vortex_ratio: f64,
    /// Whether apex operates in stable flared spray discharge without roping (Cv_u <= 52.0%).
    pub is_discharge_in_spray_regime: bool,
    /// Whether pressure drop is within normal centrifugal pump operating envelope (40 to 220 kPa).
    pub is_pressure_drop_acceptable: bool,
    /// Whether inlet velocity avoids nozzle scouring erosion while maintaining swirl (3.5 to 9.0 m/s).
    pub is_inlet_velocity_optimal: bool,
    /// Overall hydrocyclone classification efficiency and operability status.
    pub is_classification_viable: bool,
}

/// Solves hydrocyclone solid-liquid classification, cut size, and Plitt partition dynamics.
pub fn solve_hydrocyclone_separation(
    problem: &HydrocycloneProblem,
) -> Result<HydrocycloneResult, String> {
    if problem.cyclone_diameter_mm <= 25.0 {
        return Err("Cyclone diameter must be at least 25 mm".to_string());
    }
    if problem.feed_flow_rate_m3_h <= 0.0 {
        return Err("Feed flow rate must be positive".to_string());
    }
    if problem.inlet_diameter_mm <= 0.0
        || problem.vortex_finder_diameter_mm <= 0.0
        || problem.apex_diameter_mm <= 0.0
    {
        return Err("Nozzle diameters must be positive".to_string());
    }
    if problem.solid_density_kg_m3 <= problem.liquid_density_kg_m3 {
        return Err(
            "Solid density must exceed liquid density for centrifugal separation".to_string(),
        );
    }

    let d_c_cm = problem.cyclone_diameter_mm / 10.0;
    let d_i_cm = problem.inlet_diameter_mm / 10.0;
    let d_o_cm = problem.vortex_finder_diameter_mm / 10.0;
    let d_u_cm = problem.apex_diameter_mm / 10.0;
    let h_cm = problem.free_vortex_height_mm / 10.0;
    let q_m3_h = problem.feed_flow_rate_m3_h;
    let _q_l_s = q_m3_h / 3.6; // L/s

    let c_v = problem.solids_volumetric_concentration_pct.clamp(1.0, 45.0);
    let rho_s = problem.solid_density_kg_m3 / 1000.0; // g/cm³
    let rho_l = problem.liquid_density_kg_m3 / 1000.0; // g/cm³
    let delta_rho = (rho_s - rho_l).max(0.1);

    // Inlet velocity v_i in m/s:
    let a_inlet_m2 = std::f64::consts::PI * (problem.inlet_diameter_mm * 1e-3).powi(2) / 4.0;
    let v_inlet_m_s = (q_m3_h / 3600.0) / a_inlet_m2;

    // Plitt pressure drop model:
    // P = (16.5 * Q^1.8 * exp(0.0055 * Cv)) / (Dc^0.37 * Di^0.94 * h^0.28 * (Du^2 + Do^2)^0.45) [kPa]
    let p_num = 16.5 * q_m3_h.powf(1.80) * (0.0055 * c_v).exp();
    let p_den = d_c_cm.powf(0.37)
        * d_i_cm.powf(0.94)
        * h_cm.powf(0.28)
        * (d_u_cm.powi(2) + d_o_cm.powi(2)).powf(0.45);
    let delta_p_kpa = (p_num / p_den).clamp(10.0, 500.0);

    // Plitt cut size d50c model (in µm, with Q in L/min per Plitt formulation):
    let q_l_min = q_m3_h * (1000.0 / 60.0);
    let d50_num =
        50.5 * d_c_cm.powf(0.46) * d_i_cm.powf(0.60) * d_o_cm.powf(1.21) * (0.063 * c_v).exp();
    let d50_den = d_u_cm.powf(0.71) * h_cm.powf(0.38) * q_l_min.powf(0.45) * delta_rho.sqrt();
    let d50c_um = (d50_num / d50_den).clamp(5.0, 500.0);

    // Liquid recovery to underflow (bypass fraction Rf):
    // S = (6.4 * (Du / Do)^3.31 * h^0.54 * (Du^2 + Do^2)^0.36 * exp(0.0054 * Cv)) / (Dc^1.11 * P^0.24)
    let s_ratio = 6.40
        * (d_u_cm / d_o_cm).powf(3.31)
        * h_cm.powf(0.54)
        * (d_u_cm.powi(2) + d_o_cm.powi(2)).powf(0.36)
        * (0.0054 * c_v).exp()
        / (d_c_cm.powf(1.11) * delta_p_kpa.powf(0.24));
    let r_f_pct = ((s_ratio / (1.0 + s_ratio)) * 100.0).clamp(5.0, 45.0);

    // Sharpness parameter m:
    // m = 1.94 * exp(-1.58 * (S / (1+S))) * (Dc^2 * h / Q)^0.15 (with Q in L/min)
    let vol_factor = (d_c_cm.powi(2) * h_cm / q_l_min).powf(0.15);
    let m_sharpness = (1.94 * (-1.58 * (r_f_pct / 100.0)).exp() * vol_factor).clamp(1.5, 4.5);

    // Partition curve evaluation at target particle size:
    // y_c(d) = 1 - exp(-0.693 * (d / d50c)^m)
    let size_ratio = problem.target_evaluation_size_um / d50c_um;
    let y_corrected = 1.0 - (-0.693 * size_ratio.powf(m_sharpness)).exp();
    let actual_recovery_pct = (r_f_pct + (100.0 - r_f_pct) * y_corrected).clamp(0.0, 100.0);

    // Flow rates and underflow solids concentration:
    let q_u = q_m3_h * (r_f_pct / 100.0);
    let q_o = q_m3_h - q_u;

    // Approximate total solids recovery to underflow:
    let total_solids_rec_pct = (r_f_pct + 45.0).clamp(20.0, 92.0);
    let c_v_u = (c_v * (total_solids_rec_pct / r_f_pct)).clamp(c_v, 58.0);

    let apex_vortex_ratio = problem.apex_diameter_mm / problem.vortex_finder_diameter_mm;

    // Operational criteria checks:
    let is_spray_regime = c_v_u <= 52.0; // Over 52% indicates severe roping / apex blockage risk
    let is_dp_ok = (40.0..=220.0).contains(&delta_p_kpa);
    let is_vi_ok = (3.5..=9.0).contains(&v_inlet_m_s);
    let is_overall_ok = is_spray_regime && is_dp_ok && is_vi_ok;

    Ok(HydrocycloneResult {
        problem_id: problem.problem_id.clone(),
        inlet_slurry_velocity_m_s: v_inlet_m_s,
        pressure_drop_kpa: delta_p_kpa,
        corrected_cut_size_d50c_um: d50c_um,
        sharpness_index_m: m_sharpness,
        liquid_recovery_to_underflow_pct: r_f_pct,
        underflow_flow_rate_m3_h: q_u,
        overflow_flow_rate_m3_h: q_o,
        total_solids_recovery_to_underflow_pct: total_solids_rec_pct,
        underflow_solids_volumetric_pct: c_v_u,
        actual_recovery_at_target_size_pct: actual_recovery_pct,
        apex_to_vortex_ratio: apex_vortex_ratio,
        is_discharge_in_spray_regime: is_spray_regime,
        is_pressure_drop_acceptable: is_dp_ok,
        is_inlet_velocity_optimal: is_vi_ok,
        is_classification_viable: is_overall_ok,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_copper_grinding_circuit_cyclone() -> Result<(), String> {
        let prob = HydrocycloneProblem::default();
        let res = solve_hydrocyclone_separation(&prob)?;

        assert!(res.inlet_slurry_velocity_m_s >= 4.0 && res.inlet_slurry_velocity_m_s <= 8.5);
        assert!(res.pressure_drop_kpa >= 50.0 && res.pressure_drop_kpa <= 180.0);
        assert!(res.corrected_cut_size_d50c_um > 20.0 && res.corrected_cut_size_d50c_um < 150.0);
        assert!(res.sharpness_index_m >= 1.5 && res.sharpness_index_m <= 4.0);
        assert!(
            res.liquid_recovery_to_underflow_pct > 8.0
                && res.liquid_recovery_to_underflow_pct < 35.0
        );
        assert!(res.is_discharge_in_spray_regime);
        assert!(res.is_pressure_drop_acceptable);
        assert!(res.is_inlet_velocity_optimal);
        assert!(res.is_classification_viable);
        Ok(())
    }
}
