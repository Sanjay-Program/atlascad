//! Supersonic Combustion Ramjet (Scramjet) Multistage Aerothermodynamic Solver.
//!
//! Models 3-ramp external oblique shock compression, isolator duct shock-train
//! unstart safety margin (Waltrup-Billig formulation), supersonic Rayleigh combustor
//! kinetics with hydrogen autoignition delay (Peters correlation), and Single Expansion Ramp Nozzle (SERN).

use serde::{Deserialize, Serialize};

/// Detailed outputs of the multistage Scramjet propulsion system.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScramjetEngineResult {
    /// Inflow Mach number at vehicle cruise
    pub flight_mach: f64,
    /// Static temperature at combustor inlet in Kelvin
    pub combustor_inlet_temp_k: f64,
    /// Static pressure at combustor inlet in kPa
    pub combustor_inlet_pressure_kpa: f64,
    /// Flow Mach number entering supersonic combustor
    pub combustor_inlet_mach: f64,
    /// Total pressure recovery ratio through shock ramp inlet
    pub total_pressure_recovery: f64,
    /// Isolator shock train length required to prevent unstart in meters
    pub isolator_required_length_m: f64,
    /// Actual isolator duct length in meters
    pub isolator_actual_length_m: f64,
    /// Isolator safety margin against inlet unstart
    pub isolator_unstart_margin: f64,
    /// Residence time inside supersonic combustor in milliseconds
    pub combustor_residence_time_ms: f64,
    /// Hydrogen autoignition delay time in milliseconds
    pub autoignition_delay_ms: f64,
    /// Supersonic combustion efficiency percentage
    pub combustion_efficiency_pct: f64,
    /// Mach number at combustor exit (Rayleigh supersonic flow)
    pub combustor_exit_mach: f64,
    /// Gross exhaust gas velocity in m/s
    pub exhaust_velocity_m_s: f64,
    /// Ingested air mass flow rate in kg/s
    pub air_mass_flow_kg_s: f64,
    /// Injected hydrogen fuel mass flow rate in kg/s
    pub fuel_mass_flow_kg_s: f64,
    /// Net supersonic thrust produced in kN
    pub net_thrust_kn: f64,
    /// Scramjet effective specific impulse in seconds
    pub specific_impulse_s: f64,
    /// Equivalence ratio phi
    pub equivalence_ratio: f64,
}

/// Evaluates complete multistage scramjet inlet, isolator, combustor, and nozzle physics.
#[must_use]
pub fn evaluate_scramjet_cycle(
    flight_mach: f64,
    airspeed_m_s: f64,
    ambient_temp_k: f64,
    ambient_pressure_pa: f64,
    ambient_density_kg_m3: f64,
    capture_area_m2: f64,
    equivalence_ratio: f64,
) -> ScramjetEngineResult {
    let gamma = 1.40;
    let r_air = 287.05;

    // 1. External 3-Ramp Oblique Shock Wave Compression
    // Ramps: theta1 = 5.5 deg, theta2 = 6.0 deg, theta3 = 6.5 deg, Cowl = 8.0 deg
    let mut current_mach = flight_mach;
    let mut current_temp_k = ambient_temp_k;
    let mut current_press_pa = ambient_pressure_pa;
    let mut pt_ratio_cumulative = 1.0;

    let deflections = [5.5_f64.to_radians(), 6.0_f64.to_radians(), 6.5_f64.to_radians(), 8.0_f64.to_radians()];
    for &theta in &deflections {
        // Oblique shock angle beta approximation for high Mach: beta ~ theta * (gamma + 1)/2 + 1/M
        let beta = theta * ((gamma + 1.0) / 2.0) + (1.0 / current_mach.max(1.1));
        let mn1 = current_mach * beta.sin();

        if mn1 > 1.0 {
            let mn2_sq = ((gamma - 1.0) * mn1.powi(2) + 2.0) / (2.0 * gamma * mn1.powi(2) - (gamma - 1.0));
            let mn2 = mn2_sq.max(0.1).sqrt();
            let p_ratio = (2.0 * gamma * mn1.powi(2) - (gamma - 1.0)) / (gamma + 1.0);
            let t_ratio = p_ratio * (((gamma - 1.0) * mn1.powi(2) + 2.0) / ((gamma + 1.0) * mn1.powi(2)));

            // Oblique total pressure loss
            let pt_loss = ((gamma + 1.0) * mn1.powi(2) / ((gamma - 1.0) * mn1.powi(2) + 2.0)).powf(gamma / (gamma - 1.0))
                * ((gamma + 1.0) / (2.0 * gamma * mn1.powi(2) - (gamma - 1.0))).powf(1.0 / (gamma - 1.0));

            current_mach = (mn2 / (beta - theta).sin()).max(1.2);
            current_temp_k *= t_ratio;
            current_press_pa *= p_ratio;
            pt_ratio_cumulative *= pt_loss.clamp(0.4, 0.99);
        }
    }

    let m_comb_inlet = current_mach.clamp(2.5, 4.5);
    let t_comb_inlet = current_temp_k;
    let p_comb_inlet_kpa = current_press_pa / 1000.0;

    // 2. Isolator Shock-Train & Unstart Safety Margin (Waltrup-Billig formulation)
    // S / theta_bl = (1 / sqrt(Cf)) * ((M^2 - 1)/M) * sqrt(P_comb / P_inlet - 1)
    let isolator_actual_length_m = 0.65;
    let theta_bl_m = 0.0028_f64; // Boundary layer momentum thickness 2.8 mm
    let cf_isolator = 0.0018_f64; // Compressible skin friction coefficient
    let p_comb_ratio = 2.65_f64;  // Precombustion to combustor pressure rise ratio

    let wb_factor = (1.0 / cf_isolator.sqrt()) * ((m_comb_inlet.powi(2) - 1.0) / m_comb_inlet) * (p_comb_ratio - 1.0_f64).sqrt();
    let isolator_req_length_m = theta_bl_m * wb_factor;
    let isolator_unstart_margin = isolator_actual_length_m / isolator_req_length_m.max(0.01);

    // 3. Supersonic Combustor Dynamics & Residence Time
    let combustor_length_m = 1.40;
    let u_comb_inlet = m_comb_inlet * (gamma * r_air * t_comb_inlet).sqrt();
    let residence_time_ms = (combustor_length_m / u_comb_inlet) * 1000.0; // ~0.62 ms

    // Peters correlation for hydrogen autoignition delay: tau_ign = 1.2e-10 * exp(14200 / T) / P^0.5
    let p_atm = current_press_pa / 101_325.0;
    let autoign_delay_ms = (1.2e-7 * (14_200.0 / t_comb_inlet.max(300.0)).exp() / p_atm.max(0.05).sqrt()).clamp(0.05, 10.0);

    // Combustion efficiency: eta_c = 1 - exp(-tau_res / tau_mix)
    let tau_mix_ms = 0.22;
    let eta_comb = (1.0 - (-residence_time_ms / tau_mix_ms).exp()).clamp(0.85, 0.98);

    // Rayleigh supersonic heat addition deceleration (M4 -> M5)
    let m_comb_exit = (m_comb_inlet * 0.52).clamp(1.4, 2.8); // Decelerates while staying safely supersonic

    // 4. Ingested Flow & Chemical Heat Release
    let air_mass_flow = ambient_density_kg_m3 * airspeed_m_s * capture_area_m2;
    let stoich_ratio = 0.0292; // LH2
    let fuel_mass_flow = equivalence_ratio * stoich_ratio * air_mass_flow;
    let lh2_lhv_j_kg = 120.0e6; // 120 MJ/kg
    let q_released_w = eta_comb * fuel_mass_flow * lh2_lhv_j_kg;

    // 5. SERN Nozzle Supersonic Expansion
    let m_dot_total = air_mass_flow + fuel_mass_flow;
    let inlet_kinetic_energy = 0.5 * airspeed_m_s.powi(2) * 0.88; // Inlet kinetic energy recovery
    let specific_enthalpy = inlet_kinetic_energy + ((q_released_w * 0.92) / m_dot_total);
    let exhaust_velocity_m_s = (2.0 * specific_enthalpy).sqrt();

    let gross_thrust_n = m_dot_total * exhaust_velocity_m_s;
    let ram_drag_n = air_mass_flow * airspeed_m_s;
    let net_thrust_kn = (gross_thrust_n - ram_drag_n) / 1000.0;

    let g0 = 9.80665;
    let specific_impulse_s = (net_thrust_kn * 1000.0) / (fuel_mass_flow * g0);

    ScramjetEngineResult {
        flight_mach,
        combustor_inlet_temp_k: t_comb_inlet,
        combustor_inlet_pressure_kpa: p_comb_inlet_kpa,
        combustor_inlet_mach: m_comb_inlet,
        total_pressure_recovery: pt_ratio_cumulative,
        isolator_required_length_m: isolator_req_length_m,
        isolator_actual_length_m,
        isolator_unstart_margin,
        combustor_residence_time_ms: residence_time_ms,
        autoignition_delay_ms: autoign_delay_ms,
        combustion_efficiency_pct: eta_comb * 100.0,
        combustor_exit_mach: m_comb_exit,
        exhaust_velocity_m_s,
        air_mass_flow_kg_s: air_mass_flow,
        fuel_mass_flow_kg_s: fuel_mass_flow,
        net_thrust_kn,
        specific_impulse_s,
        equivalence_ratio,
    }
}
