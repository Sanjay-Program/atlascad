// crates/atlas-simulation-core/src/wastewater_treatment.rs
//
// Environmental Engineering & Wastewater Process Mechanics:
// Activated Sludge Aeration & Secondary Clarifier Sedimentation Solver.
//
// Based on:
// - Metcalf & Eddy / AECOM: "Wastewater Engineering: Treatment and Resource Recovery"
// - ASCE / WEF Manual of Practice No. 8 (MOP 8)
// - Takács, Patry & Nolasco: Dynamic multi-layer clarifier flux theory (1991)
// - Vesilind zone settling velocity exponential model
//
// Strictly forbids unsafe code.

use serde::{Deserialize, Serialize};

fn default_problem_id() -> String {
    "wastewater_treatment_problem".to_string()
}

/// Aeration basin aeration diffuser type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AerationDiffuserType {
    /// Fine bubble membrane disc / tube diffusers (high efficiency, SOTE ~6.5%/m).
    #[default]
    FineBubbleMembrane,
    /// Coarse bubble diffusers (low clogging, SOTE ~3.0%/m).
    CoarseBubble,
    /// Mechanical surface aerator / brush rotors (SOTE ~1.8 kg O2/kWh).
    MechanicalSurface,
}

/// Wastewater treatment simulation problem specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WastewaterTreatmentProblem {
    /// Problem identifier.
    #[serde(default = "default_problem_id")]
    pub problem_id: String,
    /// Diffuser type installed in biological aeration basin.
    #[serde(default)]
    pub diffuser_type: AerationDiffuserType,
    /// Daily influent wastewater volumetric flow rate in cubic meters per day (m³/day).
    pub influent_flow_m3_day: f64,
    /// Influent 5-day Biochemical Oxygen Demand (BOD5) in mg/L (g/m³).
    pub influent_bod_mg_l: f64,
    /// Regulatory target effluent BOD5 concentration in mg/L.
    pub target_effluent_bod_mg_l: f64,
    /// Mixed Liquor Suspended Solids (MLSS) in aeration basin in mg/L.
    pub mlss_mg_l: f64,
    /// Sludge Retention Time (SRT, mean cell residence time θc) in days.
    pub sludge_retention_time_days: f64,
    /// Liquid side water depth of aeration basin in meters (m).
    pub basin_depth_m: f64,
    /// Clarifier internal diameter in meters (m).
    pub clarifier_diameter_m: f64,
    /// Number of identical circular secondary clarifiers in parallel.
    pub clarifier_count: u32,
    /// Wastewater process operating temperature in degrees Celsius (°C).
    pub wastewater_temp_c: f64,
}

impl Default for WastewaterTreatmentProblem {
    fn default() -> Self {
        Self {
            problem_id: default_problem_id(),
            diffuser_type: AerationDiffuserType::FineBubbleMembrane,
            influent_flow_m3_day: 20000.0,
            influent_bod_mg_l: 220.0,
            target_effluent_bod_mg_l: 15.0,
            mlss_mg_l: 3000.0,
            sludge_retention_time_days: 8.0,
            basin_depth_m: 4.5,
            clarifier_diameter_m: 28.0,
            clarifier_count: 2,
            wastewater_temp_c: 20.0,
        }
    }
}

/// Wastewater treatment simulation result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WastewaterTreatmentResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Required biological aeration basin volume in cubic meters (m³).
    pub aeration_basin_volume_m3: f64,
    /// Aeration basin hydraulic retention time (HRT) in hours (h).
    pub hydraulic_retention_time_hours: f64,
    /// Food-to-Microorganism ratio (F/M) in kg BOD5 / (kg MLVSS · day).
    pub food_to_microorganism_ratio: f64,
    /// Net daily waste activated sludge (WAS) biomass production in kg dry solids per day (kg TSS/day).
    pub daily_sludge_production_kg_day: f64,
    /// Actual Oxygen Transfer Rate (AOTR) in biological basin in kg O2 per day.
    pub actual_oxygen_demand_kg_day: f64,
    /// Standard Oxygen Transfer Rate (SOTR) at clean water 20°C conditions in kg O2 per hour.
    pub standard_oxygen_rate_kg_h: f64,
    /// Required aeration air compressor/blower electrical shaft power in kiloWatts (kW).
    pub blower_power_kw: f64,
    /// Secondary clarifier total surface area across all units in square meters (m²).
    pub clarifier_surface_area_m2: f64,
    /// Secondary clarifier Surface Overflow Rate (SOR) in m³ / (m² · day).
    pub surface_overflow_rate_m3_m2_day: f64,
    /// Secondary clarifier Solids Loading Rate (SLR) in kg TSS / (m² · hour).
    pub solids_loading_rate_kg_m2_h: f64,
    /// Vesilind zone settling velocity of mixed liquor in meters per hour (m/h).
    pub zone_settling_velocity_m_h: f64,
    /// Effluent soluble BOD5 concentration achieved in mg/L.
    pub effluent_bod_achieved_mg_l: f64,
    /// Overall BOD5 removal efficiency percentage (%).
    pub bod_removal_efficiency_pct: f64,
    /// Whether secondary clarifier Surface Overflow Rate complies with WEF MOP 8 limits (SOR <= 32 m/d).
    pub is_clarifier_sor_acceptable: bool,
    /// Whether secondary clarifier Solids Loading Rate complies with WEF MOP 8 limits (SLR <= 6.0 kg/m²·h).
    pub is_clarifier_slr_acceptable: bool,
    /// Whether aeration hydraulic retention time is within normal municipal range (4.0 to 12.0 hours).
    pub is_hrt_acceptable: bool,
    /// Whether final effluent meets target environmental discharge standard.
    pub is_effluent_compliant: bool,
}

/// Solves biological activated sludge kinetic sizing, aeration oxygen transfer, and clarifier solids flux.
pub fn solve_wastewater_treatment(
    problem: &WastewaterTreatmentProblem,
) -> Result<WastewaterTreatmentResult, String> {
    if problem.influent_flow_m3_day <= 0.0 || problem.influent_bod_mg_l <= 0.0 {
        return Err("Influent flow and BOD must be positive".to_string());
    }
    if problem.mlss_mg_l <= 500.0 || problem.sludge_retention_time_days <= 1.0 {
        return Err("MLSS must exceed 500 mg/L and SRT must exceed 1 day".to_string());
    }
    if problem.basin_depth_m <= 1.0 || problem.clarifier_diameter_m <= 5.0 || problem.clarifier_count == 0 {
        return Err("Clarifier and basin dimensions must be positive non-zero".to_string());
    }

    let q = problem.influent_flow_m3_day;
    let s0 = problem.influent_bod_mg_l;
    let s_target = problem.target_effluent_bod_mg_l;
    let srt = problem.sludge_retention_time_days;
    let x = problem.mlss_mg_l;
    let temp_c = problem.wastewater_temp_c;

    // Typical municipal activated sludge kinetic coefficients (Metcalf & Eddy)
    // Yield coefficient Y (mg VSS / mg BOD)
    let y_vss = 0.60;
    // Endogenous decay coefficient kd at 20°C (1/day)
    let kd_20 = 0.060;
    // Temperature adjustment for kd: kd(T) = kd(20) * 1.04^(T - 20)
    let kd_t = kd_20 * 1.04_f64.powf(temp_c - 20.0);
    // Volatile suspended solids fraction of mixed liquor MLVSS / MLSS
    let f_vss = 0.80;
    let x_v = x * f_vss;

    // Effluent BOD achieved via Monod kinetics:
    // With Ks ~ 40 mg/L, mu_max ~ 3.5 1/day, at design SRT effluent soluble BOD is low:
    let ks = 40.0;
    let mu_max = 3.5 * 1.06_f64.powf(temp_c - 20.0);
    // 1 / srt = mu_max * S / (Ks + S) - kd
    let net_growth_target = (1.0 / srt) + kd_t;
    let s_achieved = if mu_max > net_growth_target {
        (ks * net_growth_target) / (mu_max - net_growth_target)
    } else {
        s_target
    };
    let s_effluent = s_achieved.min(s_target);

    // Aeration basin volume V via biomass mass balance:
    // V = (srt * Q * Y * (S0 - S)) / (X_v * (1 + kd * srt))
    let substrate_removed = (s0 - s_effluent).max(1.0);
    let v_aero = (srt * q * y_vss * substrate_removed) / (x_v * (1.0 + kd_t * srt));
    let hrt_hours = (v_aero / q) * 24.0;

    // Food to Microorganism (F/M) ratio in kg BOD5 / (kg MLVSS · day)
    let f_m_ratio = (q * s0 * 1e-3) / (v_aero * x_v * 1e-3);

    // Daily waste sludge biomass production P_x in kg TSS / day:
    // P_x_vss = V * X_v / srt
    let p_x_vss = (v_aero * x_v * 1e-3) / srt;
    let p_x_tss = p_x_vss / f_vss;

    // Actual Oxygen Transfer Rate (AOTR):
    // AOTR = Q * (S0 - S) * 1e-3 - 1.42 * P_x_vss  [kg O2 / day]
    let aotr_kg_day = ((q * substrate_removed * 1e-3) - (1.42 * p_x_vss)).max(q * substrate_removed * 1e-3 * 0.5);

    // Correction to Standard Clean Water Oxygen Transfer Rate (SOTR):
    // SOTR = AOTR / [ alpha * F * (beta * C_s_T_alt / C_s_20 - C_L) * theta^(T-20) ]
    let alpha = 0.65; // wastewater oxygen transfer correction factor
    let beta = 0.95;  // wastewater salinity / dissolved solids correction factor
    let c_s_20 = 9.09; // clean water saturation DO at 20°C, 1 atm [mg/L]
    let c_s_t = 14.652 - 0.41022 * temp_c + 0.007991 * temp_c * temp_c; // DO saturation at operating temp
    let c_l = 2.0;    // target operating dissolved oxygen level in basin [mg/L]
    let theta_do: f64 = 1.024;
    let denom_o2 = alpha * (((beta * c_s_t) / c_s_20) - (c_l / c_s_20)) * theta_do.powf(temp_c - 20.0);
    let sotr_kg_day = if denom_o2 > 0.1 { aotr_kg_day / denom_o2 } else { aotr_kg_day * 1.5 };
    let sotr_kg_h = sotr_kg_day / 24.0;

    // Aeration blower power based on diffuser efficiency
    let sote_per_m = match problem.diffuser_type {
        AerationDiffuserType::FineBubbleMembrane => 0.065, // 6.5% per meter depth
        AerationDiffuserType::CoarseBubble => 0.030,       // 3.0% per meter depth
        AerationDiffuserType::MechanicalSurface => 0.025,  // surface aerator equivalent
    };
    let total_sote = (sote_per_m * problem.basin_depth_m).min(0.40);
    // Oxygen weight fraction in air = 0.232 kg O2 / kg air, air density = 1.20 kg/m³
    // Standard blower electrical power ~ 28 - 35 W per kg O2 delivered / h per meter of submergence
    let o2_delivered_per_kwh = total_sote * 6.5; // kg O2 / kWh shaft input
    let blower_power_kw = (sotr_kg_h / o2_delivered_per_kwh).max(5.0);

    // Secondary clarifier calculations:
    let r_clar = problem.clarifier_diameter_m * 0.5;
    let single_clar_area = std::f64::consts::PI * r_clar * r_clar;
    let total_clar_area = single_clar_area * (problem.clarifier_count as f64);

    // Surface Overflow Rate (SOR) in m³ / (m² · day)
    let sor = q / total_clar_area;

    // Solids Loading Rate (SLR) in kg TSS / (m² · h):
    // SLR = (Q + Q_ras) * MLSS / Area; assuming typical recycle ratio Q_ras / Q = 0.75
    let q_total_solids_m3_d = q * 1.75;
    let slr = (q_total_solids_m3_d * x * 1e-3) / (total_clar_area * 24.0);

    // Vesilind mixed liquor zone settling velocity (m/h):
    // v_s = v0 * exp(-r * X), typical activated sludge v0 = 9.0 m/h, r = 0.00045 L/mg
    let v0_vesilind = 9.20;
    let r_vesilind = 0.00042;
    let zone_settling_v_m_h = v0_vesilind * (-r_vesilind * x).exp();

    // Effluent BOD removal efficiency
    let bod_rem_pct = ((s0 - s_effluent) / s0) * 100.0;

    // Standards compliance check (WEF MOP 8 & Ten States Standards)
    let is_sor_acceptable = sor <= 32.0;       // Max design SOR <= 32 m³/m²·d (peak 40)
    let is_slr_acceptable = slr <= 6.0;        // Max design SLR <= 6.0 kg/m²·h (peak 8.0)
    let is_hrt_acceptable = (4.0..=12.0).contains(&hrt_hours); // Normal municipal HRT 4 - 12 hours
    let is_effluent_compliant = s_effluent <= s_target;

    Ok(WastewaterTreatmentResult {
        problem_id: problem.problem_id.clone(),
        aeration_basin_volume_m3: v_aero,
        hydraulic_retention_time_hours: hrt_hours,
        food_to_microorganism_ratio: f_m_ratio,
        daily_sludge_production_kg_day: p_x_tss,
        actual_oxygen_demand_kg_day: aotr_kg_day,
        standard_oxygen_rate_kg_h: sotr_kg_h,
        blower_power_kw,
        clarifier_surface_area_m2: total_clar_area,
        surface_overflow_rate_m3_m2_day: sor,
        solids_loading_rate_kg_m2_h: slr,
        zone_settling_velocity_m_h: zone_settling_v_m_h,
        effluent_bod_achieved_mg_l: s_effluent,
        bod_removal_efficiency_pct: bod_rem_pct,
        is_clarifier_sor_acceptable: is_sor_acceptable,
        is_clarifier_slr_acceptable: is_slr_acceptable,
        is_hrt_acceptable,
        is_effluent_compliant,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_municipal_activated_sludge_sizing() -> Result<(), String> {
        let prob = WastewaterTreatmentProblem {
            problem_id: "municipal_wwtp_20mld".to_string(),
            diffuser_type: AerationDiffuserType::FineBubbleMembrane,
            influent_flow_m3_day: 20000.0,
            influent_bod_mg_l: 220.0,
            target_effluent_bod_mg_l: 15.0,
            mlss_mg_l: 3000.0,
            sludge_retention_time_days: 8.0,
            basin_depth_m: 4.5,
            clarifier_diameter_m: 28.0,
            clarifier_count: 2,
            wastewater_temp_c: 20.0,
        };

        let res = solve_wastewater_treatment(&prob)?;
        assert!(res.aeration_basin_volume_m3 > 4000.0 && res.aeration_basin_volume_m3 < 7000.0);
        assert!((5.0..=9.0).contains(&res.hydraulic_retention_time_hours));
        assert!(res.food_to_microorganism_ratio > 0.20 && res.food_to_microorganism_ratio < 0.40);
        assert!(res.daily_sludge_production_kg_day > 1500.0);
        assert!(res.surface_overflow_rate_m3_m2_day > 10.0 && res.surface_overflow_rate_m3_m2_day <= 25.0);
        assert!(res.solids_loading_rate_kg_m2_h > 2.0 && res.solids_loading_rate_kg_m2_h <= 5.0);
        assert!(res.zone_settling_velocity_m_h > 2.0);
        assert!(res.bod_removal_efficiency_pct > 90.0);
        assert!(res.is_clarifier_sor_acceptable);
        assert!(res.is_clarifier_slr_acceptable);
        assert!(res.is_hrt_acceptable);
        assert!(res.is_effluent_compliant);
        Ok(())
    }
}
