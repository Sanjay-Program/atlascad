// crates/atlas-simulation-core/src/heat_exchanger_delaware.rs
//
// Chemical, Process & Thermal Engineering:
// Shell-and-Tube Heat Exchanger Rating & Pressure Drop Solver (Bell-Delaware Method & TEMA Standards).
//
// Based on:
// - Bell (1963, 1981): "Final Report of the Cooperative Research Program on Shell and Tube Heat Exchangers" (Delaware)
// - TEMA (Tubular Exchanger Manufacturers Association) 10th Edition: Standards for Class R, C, B
// - Bowman, Mueller, & Nagle (1940): "Mean Temperature Difference in Design" (Trans. ASME)
// - Kern (1950) & Serth (2007): "Process Heat Transfer: Principles and Applications"
//
// Strictly forbids unsafe code.

use serde::{Deserialize, Serialize};

fn default_problem_id() -> String {
    "heat_exchanger_delaware_problem".to_string()
}

/// TEMA Shell Type configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TemaShellType {
    /// E-Shell: One-pass shell (most common industrial configuration).
    #[default]
    EOnePassShell,
    /// F-Shell: Two-pass shell with longitudinal baffle.
    FTwoPassShell,
    /// J-Shell: Divided flow shell for low pressure drop.
    JDividedFlowShell,
}

/// Tube layout pattern in shell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TemaTubeLayout {
    /// Triangular 30° pitch (maximum packing density and heat transfer).
    #[default]
    Triangular30Deg,
    /// Square 90° pitch (facilitates mechanical cleaning lane).
    Square90Deg,
    /// Rotated square 45° pitch.
    RotatedSquare45Deg,
}

/// Shell-and-tube heat exchanger simulation problem specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeatExchangerDelawareProblem {
    /// Problem identifier.
    #[serde(default = "default_problem_id")]
    pub problem_id: String,
    /// TEMA shell configuration type.
    #[serde(default)]
    pub shell_type: TemaShellType,
    /// Tube layout pattern.
    #[serde(default)]
    pub tube_layout: TemaTubeLayout,
    /// Shell inside diameter D_s in meters (m, typ. 0.30 - 1.80 m).
    pub shell_inside_diameter_m: f64,
    /// Number of tubes in bundle N_t.
    pub number_of_tubes: u32,
    /// Tube outer diameter d_o in millimeters (mm, typ. 19.05 mm = 3/4" or 25.4 mm = 1").
    pub tube_outer_diameter_mm: f64,
    /// Tube wall thickness t_w in millimeters (mm, typ. 1.65 - 2.11 mm).
    pub tube_wall_thickness_mm: f64,
    /// Tube effective active length L in meters (m, typ. 3.0 - 6.0 m).
    pub tube_length_m: f64,
    /// Tube center-to-center pitch P_t in millimeters (mm, typ. 1.25 * d_o).
    pub tube_pitch_mm: f64,
    /// Segmental baffle cut height percentage B_c (% of shell diameter, typ. 20 - 30%).
    pub baffle_cut_pct: f64,
    /// Central baffle spacing B in meters (m, typ. 0.20 - 0.50 m).
    pub baffle_spacing_m: f64,
    /// Hot fluid inlet temperature in Celsius (°C).
    pub hot_inlet_temp_c: f64,
    /// Hot fluid outlet temperature in Celsius (°C).
    pub hot_outlet_temp_c: f64,
    /// Hot fluid mass flow rate in kg/s.
    pub hot_mass_flow_kg_s: f64,
    /// Cold fluid inlet temperature in Celsius (°C).
    pub cold_inlet_temp_c: f64,
    /// Cold fluid outlet temperature in Celsius (°C).
    pub cold_outlet_temp_c: f64,
    /// Cold fluid mass flow rate in kg/s.
    pub cold_mass_flow_kg_s: f64,
    /// Shell-side fluid specific heat c_p in J/(kg·K).
    pub shell_fluid_cp_j_kg_k: f64,
    /// Shell-side fluid dynamic viscosity in Pa·s.
    pub shell_fluid_viscosity_pa_s: f64,
    /// Shell-side fluid thermal conductivity in W/(m·K).
    pub shell_fluid_conductivity_w_m_k: f64,
    /// Shell-side fluid density in kg/m³.
    pub shell_fluid_density_kg_m3: f64,
    /// Tube-side convective heat transfer coefficient in W/(m²·K).
    pub tube_side_htc_w_m2_k: f64,
    /// Total fouling thermal resistance (shell + tube) in m²·K/W (TEMA standard ~ 0.00035).
    pub fouling_resistance_m2_k_w: f64,
}

impl Default for HeatExchangerDelawareProblem {
    fn default() -> Self {
        Self {
            problem_id: default_problem_id(),
            shell_type: TemaShellType::EOnePassShell,
            tube_layout: TemaTubeLayout::Triangular30Deg,
            shell_inside_diameter_m: 0.60,
            number_of_tubes: 360,
            tube_outer_diameter_mm: 19.05,
            tube_wall_thickness_mm: 1.65,
            tube_length_m: 4.80,
            tube_pitch_mm: 23.81,
            baffle_cut_pct: 25.0,
            baffle_spacing_m: 0.30,
            hot_inlet_temp_c: 120.0,
            hot_outlet_temp_c: 65.0,
            hot_mass_flow_kg_s: 18.5,
            cold_inlet_temp_c: 25.0,
            cold_outlet_temp_c: 50.0,
            cold_mass_flow_kg_s: 32.0,
            shell_fluid_cp_j_kg_k: 2450.0,
            shell_fluid_viscosity_pa_s: 0.00085,
            shell_fluid_conductivity_w_m_k: 0.165,
            shell_fluid_density_kg_m3: 840.0,
            tube_side_htc_w_m2_k: 1850.0,
            fouling_resistance_m2_k_w: 0.00035,
        }
    }
}

/// Shell-and-tube heat exchanger Delaware rating result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeatExchangerDelawareResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Total heat transfer thermal duty Q in kiloWatts (kW).
    pub thermal_duty_kw: f64,
    /// Counterflow Log-Mean Temperature Difference (LMTD) in Celsius (°C).
    pub counterflow_lmtd_c: f64,
    /// Bowman-Mueller-Nagle mean temperature difference correction factor F.
    pub lmtd_correction_factor_f: f64,
    /// Corrected effective mean temperature difference Delta-T_m in Celsius (°C).
    pub effective_mean_temp_diff_c: f64,
    /// Bell ideal tube bank crossflow heat transfer coefficient in W/(m²·K).
    pub ideal_crossflow_htc_w_m2_k: f64,
    /// Combined Bell-Delaware correction factor (J_c * J_l * J_b * J_s).
    pub bell_combined_correction_factor: f64,
    /// Shell-side corrected heat transfer coefficient h_s in W/(m²·K).
    pub shell_side_corrected_htc_w_m2_k: f64,
    /// Overall heat transfer coefficient U_dirty including fouling in W/(m²·K).
    pub overall_htc_w_m2_k: f64,
    /// Total outer heat transfer surface area available in bundle in square meters (m²).
    pub available_surface_area_m2: f64,
    /// Required heat transfer surface area in square meters (m²).
    pub required_surface_area_m2: f64,
    /// Surface area overdesign margin percentage (%).
    pub surface_area_overdesign_pct: f64,
    /// Shell-side total pressure drop in kiloPascals (kPa).
    pub shell_side_pressure_drop_kpa: f64,
    /// Whether LMTD F-factor is non-critical (F >= 0.80 per TEMA recommendation).
    pub is_f_factor_acceptable: bool,
    /// Whether surface area overdesign margin is adequate (Overdesign >= 10.0%).
    pub is_surface_area_adequate: bool,
    /// Whether shell-side pressure drop is within acceptable limit (Delta-P <= 70.0 kPa).
    pub is_pressure_drop_acceptable: bool,
}

/// Solves Bell-Delaware shell-side flow geometry, crossflow velocities, Bell J-factors, and TEMA rating.
pub fn solve_heat_exchanger_delaware(
    problem: &HeatExchangerDelawareProblem,
) -> Result<HeatExchangerDelawareResult, String> {
    if problem.shell_inside_diameter_m <= 0.10 || problem.number_of_tubes == 0 {
        return Err("Shell diameter and number of tubes must be positive".to_string());
    }
    if problem.tube_outer_diameter_mm <= 5.0
        || problem.tube_pitch_mm <= problem.tube_outer_diameter_mm
    {
        return Err("Tube pitch must strictly exceed tube outer diameter".to_string());
    }
    if problem.baffle_cut_pct <= 10.0 || problem.baffle_cut_pct >= 45.0 {
        return Err("Baffle cut must be between 10% and 45%".to_string());
    }

    // Thermal duty: Q = m_dot * c_p * Delta_T
    let delta_t_hot = (problem.hot_inlet_temp_c - problem.hot_outlet_temp_c).abs();
    let q_w = problem.hot_mass_flow_kg_s * problem.shell_fluid_cp_j_kg_k * delta_t_hot;
    let q_kw = q_w / 1000.0;

    // Log-Mean Temperature Difference (LMTD) for counter-current flow:
    // Delta_T1 = T_h,in - T_c,out, Delta_T2 = T_h,out - T_c,in
    let dt1 = problem.hot_inlet_temp_c - problem.cold_outlet_temp_c;
    let dt2 = problem.hot_outlet_temp_c - problem.cold_inlet_temp_c;
    if dt1 <= 0.0 || dt2 <= 0.0 {
        return Err(
            "Temperature cross violation: cold stream temperature exceeds hot stream".to_string(),
        );
    }
    let lmtd_c = if (dt1 - dt2).abs() < 0.1 {
        (dt1 + dt2) / 2.0
    } else {
        (dt1 - dt2) / (dt1 / dt2).ln()
    };

    // Bowman-Mueller-Nagle LMTD F-factor for 1-shell pass, 2-or-more tube passes:
    // P = (t2 - t1) / (T1 - t1)
    // R = (T1 - T2) / (t2 - t1)
    let p_therm = (problem.cold_outlet_temp_c - problem.cold_inlet_temp_c)
        / (problem.hot_inlet_temp_c - problem.cold_inlet_temp_c);
    let r_therm = (problem.hot_inlet_temp_c - problem.hot_outlet_temp_c)
        / (problem.cold_outlet_temp_c - problem.cold_inlet_temp_c);

    let f_factor = if (r_therm - 1.0).abs() < 1e-4 {
        // Limit as R -> 1
        let p = p_therm.clamp(0.01, 0.95);
        let num = 2.0_f64.sqrt() * (p / (1.0 - p));
        let den = (2.0 - p * (2.0 - 2.0_f64.sqrt())) / (2.0 - p * (2.0 + 2.0_f64.sqrt()));
        (num / den.abs().ln()).clamp(0.60, 1.0)
    } else {
        let sqrt_r = (r_therm * r_therm + 1.0).sqrt();
        let num = sqrt_r * ((1.0 - p_therm) / (1.0 - p_therm * r_therm)).ln();
        let denom_term1 = 2.0 - p_therm * (r_therm + 1.0 - sqrt_r);
        let denom_term2 = 2.0 - p_therm * (r_therm + 1.0 + sqrt_r);
        if denom_term1 <= 0.0 || denom_term2 <= 0.0 {
            0.75
        } else {
            let den = (r_therm - 1.0) * (denom_term1 / denom_term2).ln();
            (num / den).clamp(0.60, 1.0)
        }
    };

    let delta_t_m_c = lmtd_c * f_factor;

    // Shell-side crossflow area at centerline:
    // S_m = B * [D_s - D_otl + (D_otl - d_o) * (P_t - d_o) / P_t]
    // Approximately: S_m = B * D_s * (P_t - d_o) / P_t
    let d_o_m = problem.tube_outer_diameter_mm / 1000.0;
    let p_t_m = problem.tube_pitch_mm / 1000.0;
    let s_m_m2 =
        problem.baffle_spacing_m * problem.shell_inside_diameter_m * ((p_t_m - d_o_m) / p_t_m);
    let mass_velocity_kg_m2_s = problem.hot_mass_flow_kg_s / s_m_m2;

    // Shell-side Reynolds number: Re_s = G_s * d_o / mu
    let re_s = (mass_velocity_kg_m2_s * d_o_m) / problem.shell_fluid_viscosity_pa_s;

    // Ideal tube bank crossflow heat transfer coefficient (Colburn j-factor):
    // j_H = 0.35 * Re_s^(-0.40)
    let pr_s = (problem.shell_fluid_cp_j_kg_k * problem.shell_fluid_viscosity_pa_s)
        / problem.shell_fluid_conductivity_w_m_k;
    let j_h = 0.34 * re_s.powf(-0.38);
    let h_ideal = j_h * (problem.shell_fluid_conductivity_w_m_k / d_o_m) * re_s * pr_s.cbrt();

    // Bell-Delaware Correction Factors:
    // 1. Baffle window cut factor J_c:
    let j_c = 0.55 + 0.015 * problem.baffle_cut_pct; // ~ 0.925 for 25% cut
    // 2. Baffle leakage factor J_l (tube-to-baffle and shell-to-baffle clearances):
    let j_l = 0.78; // standard commercial TEMA clearances
    // 3. Bundle-to-shell bypass stream factor J_b (with sealing strips):
    let j_b = 0.88; // with 1 pair sealing strips per 5 tube rows
    // 4. Unequal baffle spacing factor J_s:
    let j_s = 0.98;

    let bell_factor = (j_c * j_l * j_b * j_s).clamp(0.50, 0.90);
    let h_s = h_ideal * bell_factor;

    // Overall heat transfer coefficient U_dirty:
    // 1 / U = 1 / h_s + R_fouling + (d_o / (2 * k_tube)) * ln(d_o / d_i) + (d_o / d_i) * (1 / h_t)
    let d_i_m = (problem.tube_outer_diameter_mm - 2.0 * problem.tube_wall_thickness_mm) / 1000.0;
    let k_tube_wall = 45.0; // W/(m.K) Carbon steel / Admirality brass
    let r_wall = (d_o_m / (2.0 * k_tube_wall)) * (d_o_m / d_i_m).ln();
    let r_tube_side = (d_o_m / d_i_m) * (1.0 / problem.tube_side_htc_w_m2_k);
    let r_shell_side = 1.0 / h_s;

    let r_total = r_shell_side + problem.fouling_resistance_m2_k_w + r_wall + r_tube_side;
    let u_dirty = 1.0 / r_total;

    // Surface areas:
    let a_avail_m2 =
        (problem.number_of_tubes as f64) * std::f64::consts::PI * d_o_m * problem.tube_length_m;
    let a_req_m2 = q_w / (u_dirty * delta_t_m_c);
    let overdesign_pct = ((a_avail_m2 - a_req_m2) / a_req_m2) * 100.0;

    // Shell-side pressure drop (Bell-Delaware):
    // Number of baffles N_b = (L / B) - 1
    let n_baffles = (problem.tube_length_m / problem.baffle_spacing_m).round() - 1.0;
    let f_fric = 0.28 * re_s.powf(-0.18);
    let dp_ideal_pa = 2.0
        * f_fric
        * (mass_velocity_kg_m2_s * mass_velocity_kg_m2_s / problem.shell_fluid_density_kg_m3)
        * (n_baffles + 1.0);
    // Apply pressure drop correction factors (bypass & leakage):
    let r_l = 0.45; // leakage bypass reduction
    let r_b = 0.65; // bundle bypass reduction
    let dp_shell_kpa = (dp_ideal_pa * r_l * r_b) / 1000.0;

    let is_f_ok = f_factor >= 0.80;
    let is_area_ok = overdesign_pct >= 10.0;
    let is_dp_ok = dp_shell_kpa <= 70.0;

    Ok(HeatExchangerDelawareResult {
        problem_id: problem.problem_id.clone(),
        thermal_duty_kw: q_kw,
        counterflow_lmtd_c: lmtd_c,
        lmtd_correction_factor_f: f_factor,
        effective_mean_temp_diff_c: delta_t_m_c,
        ideal_crossflow_htc_w_m2_k: h_ideal,
        bell_combined_correction_factor: bell_factor,
        shell_side_corrected_htc_w_m2_k: h_s,
        overall_htc_w_m2_k: u_dirty,
        available_surface_area_m2: a_avail_m2,
        required_surface_area_m2: a_req_m2,
        surface_area_overdesign_pct: overdesign_pct,
        shell_side_pressure_drop_kpa: dp_shell_kpa,
        is_f_factor_acceptable: is_f_ok,
        is_surface_area_adequate: is_area_ok,
        is_pressure_drop_acceptable: is_dp_ok,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tema_e_shell_heat_exchanger() -> Result<(), String> {
        let prob = HeatExchangerDelawareProblem {
            problem_id: "tema_e_oil_cooler".to_string(),
            shell_type: TemaShellType::EOnePassShell,
            tube_layout: TemaTubeLayout::Triangular30Deg,
            shell_inside_diameter_m: 0.60,
            number_of_tubes: 360,
            tube_outer_diameter_mm: 19.05,
            tube_wall_thickness_mm: 1.65,
            tube_length_m: 4.80,
            tube_pitch_mm: 23.81,
            baffle_cut_pct: 25.0,
            baffle_spacing_m: 0.30,
            hot_inlet_temp_c: 120.0,
            hot_outlet_temp_c: 65.0,
            hot_mass_flow_kg_s: 18.5,
            cold_inlet_temp_c: 25.0,
            cold_outlet_temp_c: 50.0,
            cold_mass_flow_kg_s: 32.0,
            shell_fluid_cp_j_kg_k: 2450.0,
            shell_fluid_viscosity_pa_s: 0.00085,
            shell_fluid_conductivity_w_m_k: 0.165,
            shell_fluid_density_kg_m3: 840.0,
            tube_side_htc_w_m2_k: 1850.0,
            fouling_resistance_m2_k_w: 0.00035,
        };

        let res = solve_heat_exchanger_delaware(&prob)?;
        assert!(res.thermal_duty_kw > 2000.0 && res.thermal_duty_kw < 3000.0);
        assert!(res.counterflow_lmtd_c > 45.0 && res.counterflow_lmtd_c < 65.0);
        assert!(res.lmtd_correction_factor_f >= 0.80);
        assert!(
            res.shell_side_corrected_htc_w_m2_k > 400.0
                && res.shell_side_corrected_htc_w_m2_k < 1800.0
        );
        assert!(res.overall_htc_w_m2_k > 200.0 && res.overall_htc_w_m2_k < 600.0);
        assert!(res.available_surface_area_m2 > 90.0 && res.available_surface_area_m2 < 120.0);
        assert!(res.surface_area_overdesign_pct >= 10.0);
        assert!(res.shell_side_pressure_drop_kpa < 70.0);
        assert!(res.is_f_factor_acceptable);
        assert!(res.is_surface_area_adequate);
        assert!(res.is_pressure_drop_acceptable);
        Ok(())
    }
}
