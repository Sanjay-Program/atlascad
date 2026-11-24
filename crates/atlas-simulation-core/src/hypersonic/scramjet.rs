//! Supersonic Combustion Ramjet (Scramjet) Multistage Aerothermodynamic Solver.
//!
//! Models 4-ramp external oblique shock compression (exact theta-beta-Mach weak-shock
//! solution), isolator duct shock-train
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

/// Solves the theta-beta-Mach oblique shock relation for the weak (attached) shock angle.
///
/// Returns `None` when the deflection exceeds the detachment angle, in which case no
/// attached oblique shock exists and the inlet would unstart.
#[must_use]
fn weak_oblique_shock_angle(mach: f64, deflection_rad: f64, gamma: f64) -> Option<f64> {
    if mach <= 1.0 || deflection_rad <= 0.0 {
        return None;
    }

    let mach_angle = (1.0 / mach).asin();
    let rhs = |beta: f64| {
        let sin_b = beta.sin();
        let cos_b = beta.cos();
        (2.0 * cos_b / sin_b * (mach * mach * sin_b * sin_b - 1.0))
            / (mach * mach * (gamma + (2.0 * beta).cos()) + 2.0)
    };
    let target = deflection_rad.tan();

    // Below the weak-shock root and above the strong-shock root, `rhs - target` is
    // negative. Scan upward from the Mach angle to bracket the first (weak) root.
    let upper = std::f64::consts::FRAC_PI_2 - 1.0e-3;
    let mut lo = mach_angle + 1.0e-9;
    let mut hi = 0.0;
    let steps = 512;
    for step in 1..=steps {
        let beta = mach_angle + (upper - mach_angle) * (step as f64 / steps as f64);
        if rhs(beta) - target > 0.0 {
            hi = beta;
            break;
        }
        lo = beta;
    }
    if hi == 0.0 {
        return None;
    }

    // Bisection on the bracketed weak-shock root.
    for _ in 0..80 {
        let mid = 0.5 * (lo + hi);
        if rhs(mid) - target > 0.0 {
            hi = mid;
        } else {
            lo = mid;
        }
    }

    Some(0.5 * (lo + hi))
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

    // 1. External 4-Ramp Oblique Shock Wave Compression
    // Ramps: theta1 = 5.5 deg, theta2 = 6.0 deg, theta3 = 6.5 deg, Cowl = 8.0 deg
    let mut current_mach = flight_mach;
    let mut current_temp_k = ambient_temp_k;
    let mut current_press_pa = ambient_pressure_pa;
    let mut pt_ratio_cumulative = 1.0;

    let deflections = [
        5.5_f64.to_radians(),
        6.0_f64.to_radians(),
        6.5_f64.to_radians(),
        8.0_f64.to_radians(),
    ];
    for &theta in &deflections {
        // Exact weak-shock solution of the theta-beta-Mach relation for this ramp.
        let Some(beta) = weak_oblique_shock_angle(current_mach, theta, gamma) else {
            // Detached shock: the inlet cannot ingest the flow attached, so compression stops.
            break;
        };
        let mn1 = current_mach * beta.sin();
        if mn1 <= 1.0 {
            break;
        }

        let mn2_sq =
            ((gamma - 1.0) * mn1.powi(2) + 2.0) / (2.0 * gamma * mn1.powi(2) - (gamma - 1.0));
        let mn2 = mn2_sq.max(1.0e-6).sqrt();
        let p_ratio = (2.0 * gamma * mn1.powi(2) - (gamma - 1.0)) / (gamma + 1.0);
        let t_ratio =
            p_ratio * (((gamma - 1.0) * mn1.powi(2) + 2.0) / ((gamma + 1.0) * mn1.powi(2)));

        // Oblique total pressure loss through the normal component of the shock.
        let pt_loss = ((gamma + 1.0) * mn1.powi(2) / ((gamma - 1.0) * mn1.powi(2) + 2.0))
            .powf(gamma / (gamma - 1.0))
            * ((gamma + 1.0) / (2.0 * gamma * mn1.powi(2) - (gamma - 1.0)))
                .powf(1.0 / (gamma - 1.0));

        current_mach = mn2 / (beta - theta).sin();
        current_temp_k *= t_ratio;
        current_press_pa *= p_ratio;
        pt_ratio_cumulative *= pt_loss;
    }

    // Operating envelope guard: below Mach 1.2 the isolator thermally chokes and above
    // Mach 6 the ramp system provides insufficient compression for supersonic combustion.
    let m_comb_inlet = current_mach.clamp(1.2, 6.0);
    let t_comb_inlet = current_temp_k;
    let p_comb_inlet_kpa = current_press_pa / 1000.0;

    // 2. Isolator Shock-Train & Unstart Safety Margin (Waltrup-Billig formulation)
    // S / theta_bl = (1 / sqrt(Cf)) * ((M^2 - 1)/M) * sqrt(P_comb / P_inlet - 1)
    let isolator_actual_length_m = 0.65;
    let theta_bl_m = 0.0028_f64; // Boundary layer momentum thickness 2.8 mm
    let cf_isolator = 0.0018_f64; // Compressible skin friction coefficient
    let p_comb_ratio = 2.65_f64; // Precombustion to combustor pressure rise ratio

    let wb_factor = (1.0 / cf_isolator.sqrt())
        * ((m_comb_inlet.powi(2) - 1.0) / m_comb_inlet)
        * (p_comb_ratio - 1.0_f64).sqrt();
    let isolator_req_length_m = theta_bl_m * wb_factor;
    let isolator_unstart_margin = isolator_actual_length_m / isolator_req_length_m.max(0.01);

    // 3. Supersonic Combustor Dynamics & Residence Time
    let combustor_length_m = 1.40;
    let u_comb_inlet = m_comb_inlet * (gamma * r_air * t_comb_inlet).sqrt();
    let residence_time_ms = (combustor_length_m / u_comb_inlet) * 1000.0; // ~0.62 ms

    // Peters correlation for hydrogen autoignition delay: tau_ign = 1.2e-10 * exp(14200 / T) / P^0.5
    let p_atm = current_press_pa / 101_325.0;
    let autoign_delay_ms = (1.2e-7 * (14_200.0 / t_comb_inlet.max(300.0)).exp()
        / p_atm.max(0.05).sqrt())
    .clamp(0.05, 10.0);

    // Combustion efficiency: eta_c = 1 - exp(-tau_res / tau_mix)
    let tau_mix_ms = 0.22;
    let eta_comb = (1.0 - (-residence_time_ms / tau_mix_ms).exp()).clamp(0.85, 0.98);

    // Rayleigh supersonic heat addition deceleration (M4 -> M5).
    // A constant-cp Rayleigh solution would drive the static temperature far beyond the
    // dissociation-limited H2/air flame temperature at Mach 11, so the exit Mach is a
    // lumped estimate (0.52 x inlet Mach) that keeps the combustor supersonic. A real-gas
    // equilibrium combustor is the next fidelity step.
    let m_comb_exit = (m_comb_inlet * 0.52).clamp(1.4, 2.8);

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_weak_oblique_shock_angle_matches_compressible_flow_table() {
        // NACA 1135 / Anderson table: M = 2.0, theta = 10 deg -> beta = 39.31 deg.
        let beta = weak_oblique_shock_angle(2.0, 10.0_f64.to_radians(), 1.4).unwrap_or(f64::NAN);
        assert!(
            (beta.to_degrees() - 39.31).abs() < 0.05,
            "beta = {:.3} deg",
            beta.to_degrees()
        );
    }

    #[test]
    fn test_oblique_shock_detachment_returns_none() {
        // The maximum attached deflection at M = 2.0 is ~22.97 deg, so 30 deg detaches.
        assert!(weak_oblique_shock_angle(2.0, 30.0_f64.to_radians(), 1.4).is_none());
    }

    #[test]
    fn test_hypersonic_inlet_is_attached_and_compressed() {
        let res = evaluate_scramjet_cycle(11.0, 3333.33, 228.65, 868.02, 0.0132, 3.65, 1.05);
        assert!(res.combustor_inlet_mach > 1.2 && res.combustor_inlet_mach < 6.0);
        assert!(res.combustor_inlet_pressure_kpa > 20.0);
        assert!(res.total_pressure_recovery > 0.40 && res.total_pressure_recovery < 0.95);
        assert!(res.isolator_unstart_margin > 1.0);
        assert!(res.net_thrust_kn > 50.0);
    }
}
