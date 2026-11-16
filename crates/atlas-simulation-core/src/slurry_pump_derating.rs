//! Centrifugal Slurry Pump Hydraulic De-Rating & Wear Life (ANSI/HI 12.1-12.6 / Cave / ISO 9906)
//!
//! Provides first-principles closed-form analytical solutions for:
//! 1. Solid-liquid slurry mixture specific gravity S_m and volumetric solids fraction C_v.
//! 2. ANSI/HI 12.1-12.6 and Cave hydraulic head de-rating ratio (H_R) and efficiency ratio (eta_R).
//! 3. Derated slurry total dynamic head (TDH_slurry) and required driver shaft brake power (BHP).
//! 4. Impeller tip peripheral velocity (v_tip) and Pagalthivarthi-Roco abrasive erosion wear rate.
//! 5. Impeller wet-end service wear life (hours) and motor driver reserve power factor.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

/// Industrial slurry class classification according to particle abrasiveness.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SlurryAbrasivenessClass {
    /// Class 1 (Mild): Non-abrasive clays, fine limestone, silt.
    Class1Mild,
    /// Class 2 (Medium): Fine sand, fly ash, coal washings.
    Class2Medium,
    /// Class 3 (Severe): Coarse silica sand, copper/iron ore tailings, phosphatic rock.
    Class3Severe,
    /// Class 4 (Extreme): Primary SAG mill discharge, gravel, granite crushed rock.
    Class4Extreme,
}

/// Input parameters for centrifugal slurry pump hydraulic de-rating analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlurryPumpProblem {
    /// Problem identifier.
    pub problem_id: String,
    /// Slurry particle abrasiveness classification.
    pub abrasiveness_class: SlurryAbrasivenessClass,
    /// Slurry mixture volumetric flow rate in cubic meters per hour (m³/h, standard 50 to 5,000 m³/h).
    pub slurry_flow_rate_m3_h: f64,
    /// Clean water total dynamic head at operating point in meters of water (m, standard 10 to 90 m).
    pub clear_water_head_m: f64,
    /// Clean water pump hydraulic efficiency at operating point (standard 0.65 to 0.88).
    pub clear_water_efficiency: f64,
    /// Solid particles specific gravity S_s (dimensionless, standard 2.2 to 5.2).
    pub solid_specific_gravity: f64,
    /// Concentration of solids by weight C_w in percentage (wt%, standard 10 to 65%).
    pub solids_concentration_wt_pct: f64,
    /// Median particle diameter d_50 in micrometers (µm, standard 20 to 1,500 µm).
    pub median_particle_size_d50_um: f64,
    /// Pump impeller outer diameter in millimeters (mm, standard 250 to 1,500 mm).
    pub impeller_diameter_mm: f64,
    /// Pump rotational shaft speed in revolutions per minute (RPM, standard 400 to 1,800 RPM).
    pub pump_rotational_speed_rpm: f64,
    /// Impeller sacrificial wear liner allowable thickness in millimeters (mm, standard 15 to 50 mm).
    pub allowable_wear_thickness_mm: f64,
}

impl Default for SlurryPumpProblem {
    fn default() -> Self {
        Self {
            problem_id: "mining_tailings_centrifugal_slurry_pump".to_string(),
            abrasiveness_class: SlurryAbrasivenessClass::Class3Severe,
            slurry_flow_rate_m3_h: 650.0,       // 650 m³/h mining tailings slurry
            clear_water_head_m: 45.0,           // 45 m water head
            clear_water_efficiency: 0.78,       // 78% clear water efficiency
            solid_specific_gravity: 2.75,       // Copper/quartz porphyry rock
            solids_concentration_wt_pct: 35.0,  // 35% solids by weight
            median_particle_size_d50_um: 110.0, // 110 µm median particle size
            impeller_diameter_mm: 680.0,        // Ø680 mm high-chrome impeller
            pump_rotational_speed_rpm: 850.0,   // 850 RPM
            allowable_wear_thickness_mm: 25.0,  // 25 mm sacrificial casing/vane wear allowance
        }
    }
}

/// Simulation results for slurry pump de-rating, power demands, and wear lifecycle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlurryPumpResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Slurry mixture specific gravity S_m (ratio to water density 1000 kg/m³).
    pub slurry_mixture_specific_gravity: f64,
    /// Slurry mixture density in kilograms per cubic meter (kg/m³).
    pub slurry_density_kg_m3: f64,
    /// Solids concentration by volume C_v in percentage (vol%).
    pub solids_volumetric_concentration_pct: f64,
    /// ANSI/HI 12.1-12.6 hydraulic head ratio H_R (H_slurry / H_water).
    pub head_derating_ratio_hr: f64,
    /// Hydraulic efficiency reduction ratio eta_R (eta_slurry / eta_water).
    pub efficiency_derating_ratio_er: f64,
    /// Derated slurry total dynamic head TDH_slurry in meters of slurry (m).
    pub derated_slurry_head_m: f64,
    /// Slurry pump operating hydraulic efficiency.
    pub slurry_operating_efficiency: f64,
    /// Shaft brake horsepower power requirement in kiloWatts (kW).
    pub shaft_power_kw: f64,
    /// Recommended nominal electric motor size (kW) including 15% safety margin.
    pub recommended_motor_power_kw: f64,
    /// Impeller outer tip peripheral velocity in meters per second (m/s).
    pub impeller_tip_speed_m_s: f64,
    /// Projected abrasive wear erosion rate in millimeters per 1,000 operating hours (mm / 1000 h).
    pub wear_erosion_rate_mm_per_1000h: f64,
    /// Estimated impeller wet-end service wear life in operating hours (h).
    pub estimated_wear_life_hours: f64,
    /// Whether head de-rating ratio is within acceptable limits (H_R >= 0.80).
    pub is_head_derating_acceptable: bool,
    /// Whether impeller peripheral tip speed avoids severe localized gouging (v_tip <= 32 m/s).
    pub is_tip_speed_within_limits: bool,
    /// Whether projected wear life satisfies standard mining maintenance cycle (>= 3,000 h).
    pub is_wear_life_satisfactory: bool,
    /// Overall slurry pump hydraulic selection and reliability status.
    pub is_slurry_pump_viable: bool,
}

/// Solves centrifugal slurry pump ANSI/HI 12.1-12.6 hydraulic de-rating and abrasive wear dynamics.
pub fn solve_slurry_pump_derating(problem: &SlurryPumpProblem) -> Result<SlurryPumpResult, String> {
    if problem.slurry_flow_rate_m3_h <= 0.0 {
        return Err("Slurry flow rate must be positive".to_string());
    }
    if problem.clear_water_head_m <= 0.0 {
        return Err("Clear water head must be positive".to_string());
    }
    if problem.clear_water_efficiency <= 0.20 || problem.clear_water_efficiency > 0.95 {
        return Err("Clear water efficiency must be between 0.20 and 0.95".to_string());
    }
    if problem.solid_specific_gravity <= 1.0 {
        return Err("Solid specific gravity must exceed 1.0 (water)".to_string());
    }
    if problem.solids_concentration_wt_pct <= 0.0 || problem.solids_concentration_wt_pct >= 80.0 {
        return Err("Solids concentration by weight must be between 0 and 80 wt%".to_string());
    }
    if problem.impeller_diameter_mm <= 50.0 {
        return Err("Impeller diameter must exceed 50 mm".to_string());
    }
    if problem.pump_rotational_speed_rpm <= 50.0 {
        return Err("Pump speed must exceed 50 RPM".to_string());
    }

    let c_w = problem.solids_concentration_wt_pct / 100.0;
    let s_s = problem.solid_specific_gravity;

    // Slurry mixture specific gravity S_m = 1 / [1 - C_w * (1 - 1/S_s)]:
    let s_m = 1.0 / (1.0 - c_w * (1.0 - 1.0 / s_s));
    let rho_slurry = s_m * 1000.0; // kg/m³

    // Volumetric solids concentration: C_v = C_w * S_m / S_s:
    let c_v = (c_w * s_m) / s_s;
    let c_v_pct = c_v * 100.0;

    // ANSI/HI 12.1-12.6 and Cave model for Head Ratio H_R:
    // H_R = 1 - 0.0015 * C_w_pct * (S_s - 1) * (d50 / 100)^0.22
    let d50_factor = (problem.median_particle_size_d50_um / 100.0).powf(0.22);
    let hr_reduction = 0.0016 * problem.solids_concentration_wt_pct * (s_s - 1.0) * d50_factor;
    let h_r = (1.0 - hr_reduction).clamp(0.70, 0.99);

    // Efficiency ratio eta_R ~ H_R:
    let eta_r = h_r;

    // Derated head in meters of slurry:
    let tdh_slurry = problem.clear_water_head_m * h_r;

    // Operating efficiency on slurry:
    let eta_slurry = problem.clear_water_efficiency * eta_r;

    // Shaft power: P = (rho_m * g * Q * TDH_slurry) / (3.6e6 * eta_slurry) [kW]
    let g = 9.80665;
    let p_shaft_kw = (rho_slurry * g * problem.slurry_flow_rate_m3_h * tdh_slurry) / (3.6e6 * eta_slurry);

    // Recommended motor power with standard 15% nameplate margin:
    let p_motor_kw = p_shaft_kw * 1.15;

    // Impeller tip speed: v_tip = pi * D * N / 60 [m/s]:
    let d_impeller_m = problem.impeller_diameter_mm / 1000.0;
    let v_tip_m_s = (std::f64::consts::PI * d_impeller_m * problem.pump_rotational_speed_rpm) / 60.0;

    // Pagalthivarthi-Roco abrasive wear model:
    // Wear rate W = k_wear * C_v * (v_tip / 25)^2.8 * (d50 / 100)^0.8 [mm / 1000 h]
    let k_wear = match problem.abrasiveness_class {
        SlurryAbrasivenessClass::Class1Mild => 0.85,
        SlurryAbrasivenessClass::Class2Medium => 2.10,
        SlurryAbrasivenessClass::Class3Severe => 4.50,
        SlurryAbrasivenessClass::Class4Extreme => 9.20,
    };
    let wear_rate_1000h = k_wear * c_v * (v_tip_m_s / 25.0).powf(2.8) * (problem.median_particle_size_d50_um / 100.0).powf(0.8);
    let wear_rate_safe = wear_rate_1000h.max(0.10);

    // Wear life in operating hours:
    let estimated_wear_life_h = (problem.allowable_wear_thickness_mm / wear_rate_safe) * 1000.0;

    // Verification assessments:
    let is_hr_ok = h_r >= 0.80;
    let is_tip_ok = v_tip_m_s <= 32.0; // Above 32 m/s causes accelerated abrasive cavitation-erosion
    let is_life_ok = estimated_wear_life_h >= 3000.0; // 3,000 hours mining shutdown maintenance benchmark
    let is_overall_viable = is_hr_ok && is_tip_ok && is_life_ok;

    Ok(SlurryPumpResult {
        problem_id: problem.problem_id.clone(),
        slurry_mixture_specific_gravity: s_m,
        slurry_density_kg_m3: rho_slurry,
        solids_volumetric_concentration_pct: c_v_pct,
        head_derating_ratio_hr: h_r,
        efficiency_derating_ratio_er: eta_r,
        derated_slurry_head_m: tdh_slurry,
        slurry_operating_efficiency: eta_slurry,
        shaft_power_kw: p_shaft_kw,
        recommended_motor_power_kw: p_motor_kw,
        impeller_tip_speed_m_s: v_tip_m_s,
        wear_erosion_rate_mm_per_1000h: wear_rate_safe,
        estimated_wear_life_hours: estimated_wear_life_h,
        is_head_derating_acceptable: is_hr_ok,
        is_tip_speed_within_limits: is_tip_ok,
        is_wear_life_satisfactory: is_life_ok,
        is_slurry_pump_viable: is_overall_viable,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mining_tailings_slurry_pump() -> Result<(), String> {
        let prob = SlurryPumpProblem::default();
        let res = solve_slurry_pump_derating(&prob)?;

        assert!(res.slurry_mixture_specific_gravity > 1.20 && res.slurry_mixture_specific_gravity < 1.45);
        assert!(res.solids_volumetric_concentration_pct > 12.0 && res.solids_volumetric_concentration_pct < 25.0);
        assert!(res.head_derating_ratio_hr >= 0.85 && res.head_derating_ratio_hr <= 0.98);
        assert!(res.derated_slurry_head_m >= 38.0);
        assert!(res.shaft_power_kw > 100.0);
        assert!(res.impeller_tip_speed_m_s > 20.0 && res.impeller_tip_speed_m_s <= 32.0);
        assert!(res.estimated_wear_life_hours >= 3000.0);
        assert!(res.is_head_derating_acceptable);
        assert!(res.is_tip_speed_within_limits);
        assert!(res.is_wear_life_satisfactory);
        assert!(res.is_slurry_pump_viable);
        Ok(())
    }
}
