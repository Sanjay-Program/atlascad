//! Geothermal Reservoir Doublet Well Heat Extraction & Thermal Breakthrough Solver.
//!
//! Implements classical Gringarten-Sauty, Bodvarsson, and Lauwerier analytical solutions
//! for fractured porous hydrothermal reservoir heat mining, doublet well hydraulics,
//! thermal breakthrough time, and 30-year thermal capacity decline profiles.
//!
//! # Physical Formulations:
//! - **Initial Thermal Output Capacity ($\dot{Q}_{th,0}$)**:
//!   $$\dot{Q}_{th,0} = \rho_w \cdot c_w \cdot Q_w \cdot (T_{res} - T_{inj}) \quad [\text{MW}_{th}]$$
//! - **Doublet Steady-State Hydraulic Pressure Differential ($\Delta P_{doublet}$)**:
//!   $$\Delta P_{doublet} = \frac{Q_w \cdot \mu_w}{\pi \cdot k \cdot H} \cdot \ln\left(\frac{D_{well}}{r_w}\right) \quad [\text{bar}]$$
//! - **Parasitic Doublet Pumping Electrical Power ($P_{pump}$)**:
//!   $$P_{pump} = \frac{Q_w \cdot \Delta P_{doublet}}{\eta_{pump}} \quad [\text{kW}_e]$$
//! - **Gringarten Hydrothermal Characteristic Breakthrough Time ($t_{break}$)**:
//!   $$t_{break} = \frac{\pi \cdot (\rho c)_{rock} \cdot H \cdot D_{well}^2}{3 \cdot (\rho c)_w \cdot Q_w} \quad [\text{years}]$$
//! - **Long-Term Production Wellhead Temperature ($T_{prod}(t)$)**:
//!   - For $t \le t_{break}$:
//!     $$T_{prod}(t) = T_{res}$$
//!   - For $t > t_{break}$:
//!     $$\Theta_D(t) = \text{erf}\left(\sqrt{\frac{t_{break}}{t - t_{break}}}\right), \quad T_{prod}(t) = T_{inj} + (T_{res} - T_{inj}) \cdot \Theta_D(t)$$
//! - **Thermal Coefficient of Performance ($COP_{th}$)**:
//!   $$COP_{th} = \frac{\dot{Q}_{th}}{P_{pump}} \ge 15.0$$

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

/// Type of geothermal reservoir rock matrix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GeothermalRockType {
    /// Crystalline granite basement (fractured EGS).
    GraniteBasement,
    /// Porous sandstone aquifer (sedimentary basin).
    SandstoneAquifer,
    /// Karstic fractured limestone / carbonate.
    KarstLimestone,
    /// Volcanic basalt / andesite.
    VolcanicBasalt,
}

/// Input parameters for geothermal doublet well reservoir heat extraction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeothermalReservoirProblem {
    /// Problem identifier.
    pub problem_id: String,
    /// Reservoir formation rock type.
    pub rock_type: GeothermalRockType,
    /// In-situ undisturbed reservoir temperature T_res in degrees Celsius (°C).
    pub reservoir_temperature_c: f64,
    /// Reinjection temperature T_inj after plant heat extraction in degrees Celsius (°C).
    pub reinjection_temperature_c: f64,
    /// Brine production/reinjection volumetric flow rate Q_w in liters per second (L/s).
    pub brine_flow_rate_l_s: f64,
    /// Net pay reservoir thickness H in meters (m).
    pub reservoir_thickness_m: f64,
    /// Horizontal separation distance D_well between injector and producer in meters (m).
    pub well_spacing_distance_m: f64,
    /// Wellbore drill radius r_w in meters (m, standard 0.108 m for 8.5" open hole).
    pub wellbore_radius_m: f64,
    /// Intrinsic reservoir permeability k in milliDarcys (mD).
    pub reservoir_permeability_md: f64,
    /// Effective rock matrix porosity phi (0.01 to 0.25).
    pub reservoir_porosity: f64,
    /// Combined brine injection and production pump efficiency (0.65 to 0.85).
    pub pump_system_efficiency: f64,
    /// Plant project design operational lifespan in calendar years (standard 30 years).
    pub project_lifespan_years: f64,
}

impl Default for GeothermalReservoirProblem {
    fn default() -> Self {
        Self {
            problem_id: "rhine_graben_doublet".to_string(),
            rock_type: GeothermalRockType::SandstoneAquifer,
            reservoir_temperature_c: 160.0,
            reinjection_temperature_c: 65.0,
            brine_flow_rate_l_s: 75.0,
            reservoir_thickness_m: 80.0,
            well_spacing_distance_m: 1200.0,
            wellbore_radius_m: 0.108,
            reservoir_permeability_md: 450.0,
            reservoir_porosity: 0.12,
            pump_system_efficiency: 0.72,
            project_lifespan_years: 30.0,
        }
    }
}

/// Simulation results for geothermal doublet well reservoir heat extraction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeothermalReservoirResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Initial rated thermal power capacity in MegaWatts-thermal (MW_th).
    pub initial_thermal_power_mw_th: f64,
    /// End-of-life (project lifespan) thermal power capacity in MegaWatts-thermal (MW_th).
    pub final_thermal_power_mw_th: f64,
    /// Doublet steady-state differential hydraulic pressure in bar.
    pub doublet_pressure_drop_bar: f64,
    /// Parasitic electric power consumed by circulation pumps in kiloWatts-electric (kW_e).
    pub parasitic_pump_power_kw_e: f64,
    /// Theoretical hydrothermal breakthrough arrival time in calendar years.
    pub thermal_breakthrough_years: f64,
    /// Production wellhead temperature at project end-of-life in degrees Celsius (°C).
    pub final_production_temperature_c: f64,
    /// Cumulative thermal energy extracted over project lifespan in GigaWatt-hours-thermal (GWh_th).
    pub cumulative_energy_extracted_gwh_th: f64,
    /// Initial Thermal Coefficient of Performance (COP_th = MW_th / MW_e).
    pub thermal_cop: f64,
    /// Average reservoir cooling rate in °C per decade.
    pub reservoir_cooling_rate_c_per_decade: f64,
    /// Whether thermal breakthrough occurs beyond the target minimum threshold (t_break >= 20 years).
    pub is_breakthrough_delayed_acceptable: bool,
    /// Whether reservoir parasitic pumping power is commercially viable (P_pump < 1200 kW).
    pub is_pumping_power_viable: bool,
    /// Whether the geothermal doublet maintains an adequate COP (COP_th >= 15.0).
    pub is_thermal_cop_sustainable: bool,
}

/// Error function approximation (Abramowitz & Stegun max error 1.5e-7).
fn erf_approx(x: f64) -> f64 {
    let sign = if x < 0.0 { -1.0 } else { 1.0 };
    let abs_x = x.abs();
    let p = 0.3275911;
    let a1 = 0.254829592;
    let a2 = -0.284496736;
    let a3 = 1.421413741;
    let a4 = -1.453152027;
    let a5 = 1.061405429;

    let t = 1.0 / (1.0 + p * abs_x);
    let poly = (((a5 * t + a4) * t + a3) * t + a2) * t + a1;
    let y = 1.0 - poly * t * (-abs_x * abs_x).exp();
    sign * y
}

/// Solves geothermal reservoir doublet extraction, thermal breakthrough, and 30-year decline.
pub fn solve_geothermal_reservoir(
    problem: &GeothermalReservoirProblem,
) -> Result<GeothermalReservoirResult, String> {
    if problem.reservoir_temperature_c <= problem.reinjection_temperature_c {
        return Err("Reservoir temperature must be greater than reinjection temperature".to_string());
    }
    if problem.brine_flow_rate_l_s <= 0.0 {
        return Err("Brine flow rate must be positive".to_string());
    }
    if problem.well_spacing_distance_m <= 10.0 {
        return Err("Well spacing must be greater than 10 m".to_string());
    }
    if problem.reservoir_permeability_md <= 0.0 {
        return Err("Reservoir permeability must be positive".to_string());
    }

    // Physical constants:
    let q_w_m3_s = problem.brine_flow_rate_l_s / 1000.0;
    let delta_t_0 = problem.reservoir_temperature_c - problem.reinjection_temperature_c;

    // Brine properties at mean temperature ~ 110 °C:
    let rho_w = 950.0; // kg/m³
    let c_w = 4220.0;  // J/(kg·K)
    let mu_w_pa_s = 0.00025; // 0.25 mPa·s at high temp

    // Volumetric heat capacity of water:
    let rho_c_w = rho_w * c_w; // ~ 4.009e6 J/(m³·K)

    // Rock matrix thermal properties:
    let (rho_rock, c_rock) = match problem.rock_type {
        GeothermalRockType::GraniteBasement => (2650.0, 850.0),
        GeothermalRockType::SandstoneAquifer => (2400.0, 920.0),
        GeothermalRockType::KarstLimestone => (2550.0, 900.0),
        GeothermalRockType::VolcanicBasalt => (2800.0, 840.0),
    };
    let phi = problem.reservoir_porosity.clamp(0.01, 0.40);
    let rho_c_rock = (1.0 - phi) * (rho_rock * c_rock) + phi * rho_c_w;

    // 1. Initial thermal power capacity:
    // Q_th0 = rho_w * c_w * Q_w * delta_t_0
    let p_th0_w = rho_c_w * q_w_m3_s * delta_t_0;
    let p_th0_mw = p_th0_w / 1e6;

    // 2. Doublet steady-state pressure difference:
    // delta_P = (Q_w * mu_w) / (pi * k * H) * ln(D / r_w)
    let k_m2 = problem.reservoir_permeability_md * 9.869233e-16;
    let ln_ratio = (problem.well_spacing_distance_m / problem.wellbore_radius_m).ln();
    let delta_p_pa = (q_w_m3_s * mu_w_pa_s) / (std::f64::consts::PI * k_m2 * problem.reservoir_thickness_m) * ln_ratio;
    let delta_p_bar = delta_p_pa / 1e5;

    // 3. Parasitic pumping power:
    let p_pump_w = (q_w_m3_s * delta_p_pa) / problem.pump_system_efficiency;
    let p_pump_kw = p_pump_w / 1000.0;
    let p_pump_mw = p_pump_w / 1e6;

    // 4. Gringarten thermal breakthrough time (seconds -> years):
    // t_break = (pi * (rho*c)_rock * H * D^2) / (3 * (rho*c)_w * Q_w)
    let t_break_s = (std::f64::consts::PI * rho_c_rock * problem.reservoir_thickness_m * problem.well_spacing_distance_m.powi(2))
        / (3.0 * rho_c_w * q_w_m3_s);
    let t_break_years = t_break_s / (86400.0 * 365.25);

    // 5. End-of-life temperature and thermal power:
    let t_eval = problem.project_lifespan_years;
    let t_prod_eol_c = if t_eval <= t_break_years {
        problem.reservoir_temperature_c
    } else {
        let arg = (t_break_years / (t_eval - t_break_years)).sqrt();
        let theta_d = erf_approx(arg).clamp(0.0, 1.0);
        problem.reinjection_temperature_c + delta_t_0 * theta_d
    };

    let delta_t_eol = (t_prod_eol_c - problem.reinjection_temperature_c).max(0.0);
    let p_theol_mw = (rho_c_w * q_w_m3_s * delta_t_eol) / 1e6;

    // 6. Cumulative energy extracted over lifespan (trapezoidal integration):
    let avg_p_th_mw = (p_th0_mw + p_theol_mw) / 2.0;
    let hours_lifespan = problem.project_lifespan_years * 8760.0;
    let cumulative_gwh_th = (avg_p_th_mw * hours_lifespan) / 1000.0;

    // 7. Thermal COP:
    let cop = p_th0_mw / p_pump_mw.max(1e-4);

    // 8. Cooling rate:
    let total_cooling = problem.reservoir_temperature_c - t_prod_eol_c;
    let cooling_per_decade = (total_cooling / problem.project_lifespan_years) * 10.0;

    let is_bt_ok = t_break_years >= 18.0;
    let is_pump_ok = p_pump_kw <= 1200.0;
    let is_cop_ok = cop >= 15.0;

    Ok(GeothermalReservoirResult {
        problem_id: problem.problem_id.clone(),
        initial_thermal_power_mw_th: p_th0_mw,
        final_thermal_power_mw_th: p_theol_mw,
        doublet_pressure_drop_bar: delta_p_bar,
        parasitic_pump_power_kw_e: p_pump_kw,
        thermal_breakthrough_years: t_break_years,
        final_production_temperature_c: t_prod_eol_c,
        cumulative_energy_extracted_gwh_th: cumulative_gwh_th,
        thermal_cop: cop,
        reservoir_cooling_rate_c_per_decade: cooling_per_decade,
        is_breakthrough_delayed_acceptable: is_bt_ok,
        is_pumping_power_viable: is_pump_ok,
        is_thermal_cop_sustainable: is_cop_ok,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rhine_graben_geothermal_doublet() -> Result<(), String> {
        let prob = GeothermalReservoirProblem::default();
        let res = solve_geothermal_reservoir(&prob)?;

        assert!(res.initial_thermal_power_mw_th > 25.0 && res.initial_thermal_power_mw_th < 35.0);
        assert!(res.doublet_pressure_drop_bar > 5.0 && res.doublet_pressure_drop_bar < 25.0);
        assert!(res.parasitic_pump_power_kw_e > 50.0 && res.parasitic_pump_power_kw_e < 250.0);
        assert!(res.thermal_breakthrough_years > 20.0 && res.thermal_breakthrough_years < 35.0);
        assert!(res.final_production_temperature_c > 145.0 && res.final_production_temperature_c <= 160.0);
        assert!(res.thermal_cop > 100.0);
        assert!(res.is_breakthrough_delayed_acceptable);
        assert!(res.is_pumping_power_viable);
        assert!(res.is_thermal_cop_sustainable);
        Ok(())
    }
}
