// crates/atlas-simulation-core/src/ev_powertrain.rs
//
// Automotive & Electric Vehicle (EV) Engineering:
// EV Longitudinal Dynamics, Motor Traction Envelope & Regenerative Braking Energy Recovery.
//
// Based on:
// - SAE J2452: Stepwise Coastdown Methodology for Measuring Tire Rolling Resistance
// - UNECE Regulation No. 13-H: Uniform Provisions Concerning the Approval of Passenger Cars with Regard to Braking
// - Ehsani, Gao & Emadi: "Modern Electric, Hybrid Electric, and Fuel Cell Vehicles" (CRC Press)
// - ISO 8714: Electric road vehicles — Reference energy consumption and range
//
// Strictly forbids unsafe code.

use serde::{Deserialize, Serialize};

fn default_problem_id() -> String {
    "ev_powertrain_problem".to_string()
}

/// EV drive axle configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum EvDrivetrainLayout {
    /// Front-Wheel Drive (FWD).
    FrontWheelDrive,
    /// Rear-Wheel Drive (RWD).
    #[default]
    RearWheelDrive,
    /// Dual-Motor All-Wheel Drive (AWD).
    AllWheelDrive,
}

/// EV powertrain simulation problem specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvPowertrainProblem {
    /// Problem identifier.
    #[serde(default = "default_problem_id")]
    pub problem_id: String,
    /// Drivetrain axle architecture layout.
    #[serde(default)]
    pub layout: EvDrivetrainLayout,
    /// Total curb + payload vehicle mass in kilograms (kg).
    pub vehicle_mass_kg: f64,
    /// Tire rolling resistance coefficient Crr (dimensionless, ~0.008 to 0.012).
    pub rolling_resistance_coeff: f64,
    /// Aerodynamic drag coefficient Cd (dimensionless, ~0.20 to 0.30).
    pub drag_coefficient: f64,
    /// Vehicle frontal projected area in square meters (m²).
    pub frontal_area_m2: f64,
    /// Effective loaded tire rolling radius in meters (m).
    pub tire_radius_m: f64,
    /// Single-speed transmission reduction gear ratio (e.g. 8.5 to 10.5).
    pub gear_ratio: f64,
    /// Mechanical driveline transmission efficiency (e.g. 0.94 to 0.97).
    pub driveline_efficiency: f64,
    /// Peak traction electric motor torque in Newton-meters (Nm).
    pub motor_peak_torque_nm: f64,
    /// Motor base corner speed (transition from constant torque to constant power) in RPM.
    pub motor_base_speed_rpm: f64,
    /// Maximum allowable electric motor operating rotational speed in RPM.
    pub motor_max_speed_rpm: f64,
    /// Usable high-voltage traction battery energy capacity in kiloWatt-hours (kWh).
    pub battery_capacity_kwh: f64,
    /// Battery pack nominal DC terminal voltage in Volts (V).
    pub battery_voltage_v: f64,
    /// Steady cruising target vehicle speed in kilometers per hour (km/h).
    pub cruising_speed_km_h: f64,
    /// Target deceleration rate during service braking event in meters per second squared (m/s²).
    pub braking_deceleration_m_s2: f64,
}

impl Default for EvPowertrainProblem {
    fn default() -> Self {
        Self {
            problem_id: default_problem_id(),
            layout: EvDrivetrainLayout::RearWheelDrive,
            vehicle_mass_kg: 1850.0,
            rolling_resistance_coeff: 0.009,
            drag_coefficient: 0.24,
            frontal_area_m2: 2.25,
            tire_radius_m: 0.330,
            gear_ratio: 9.0,
            driveline_efficiency: 0.95,
            motor_peak_torque_nm: 350.0,
            motor_base_speed_rpm: 4500.0,
            motor_max_speed_rpm: 16000.0,
            battery_capacity_kwh: 75.0,
            battery_voltage_v: 400.0,
            cruising_speed_km_h: 100.0,
            braking_deceleration_m_s2: 2.5,
        }
    }
}

/// EV powertrain simulation result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvPowertrainResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Aerodynamic drag resistance force at cruise speed in Newtons (N).
    pub aerodynamic_drag_force_n: f64,
    /// Tire rolling resistance force in Newtons (N).
    pub rolling_resistance_force_n: f64,
    /// Total steady road-load tractive resistance force in Newtons (N).
    pub total_road_load_force_n: f64,
    /// Net mechanical power required at wheels for steady cruising in kiloWatts (kW).
    pub wheel_power_at_cruise_kw: f64,
    /// High-voltage battery electrical power draw at cruise in kiloWatts (kW).
    pub battery_power_at_cruise_kw: f64,
    /// Steady-state electric vehicle driving range on full battery charge in kilometers (km).
    pub cruising_range_km: f64,
    /// Specific energy consumption at cruise in Watt-hours per kilometer (Wh/km).
    pub specific_energy_wh_km: f64,
    /// Maximum tractive force available at tire-road contact patch at low speed in Newtons (N).
    pub max_wheel_tractive_force_n: f64,
    /// Maximum initial longitudinal acceleration capability at zero speed in m/s².
    pub max_initial_acceleration_m_s2: f64,
    /// Theoretical 0 to 100 km/h acceleration time in seconds (s).
    pub acceleration_0_to_100_km_h_s: f64,
    /// Maximum vehicle road speed governed by motor redline in kilometers per hour (km/h).
    pub max_vehicle_speed_km_h: f64,
    /// Electric motor rotational speed at target cruising speed in RPM.
    pub motor_speed_at_cruise_rpm: f64,
    /// Total braking force demand during target deceleration event in Newtons (N).
    pub braking_force_demand_n: f64,
    /// Maximum electrical regenerative braking power recovered to battery in kiloWatts (kW).
    pub regen_braking_power_kw: f64,
    /// Regenerative braking energy capture fraction of total deceleration kinetic work (%).
    pub regen_capture_efficiency_pct: f64,
    /// Whether motor is operating within safe thermal/speed redline limits at cruise speed.
    pub is_cruising_speed_attainable: bool,
    /// Whether powertrain delivers sports-grade rapid acceleration (0-100 km/h <= 6.5s).
    pub is_acceleration_performance_sport: bool,
    /// Whether regenerative braking meets UNECE R13-H electrical energy recovery stability criteria.
    pub is_regen_braking_compliant: bool,
}

/// Solves EV longitudinal road load, motor torque-power envelope, cruising range, acceleration, and regen braking.
pub fn solve_ev_powertrain(problem: &EvPowertrainProblem) -> Result<EvPowertrainResult, String> {
    if problem.vehicle_mass_kg <= 200.0 || problem.tire_radius_m <= 0.10 {
        return Err(
            "Vehicle mass and tire radius must be positive realistic dimensions".to_string(),
        );
    }
    if problem.gear_ratio <= 1.0 || problem.driveline_efficiency <= 0.50 {
        return Err("Gear ratio and driveline efficiency must be realistic".to_string());
    }
    if problem.motor_peak_torque_nm <= 10.0 || problem.motor_max_speed_rpm <= 1000.0 {
        return Err("Motor specifications must be positive non-zero".to_string());
    }
    if problem.battery_capacity_kwh <= 1.0 {
        return Err("Battery capacity must be positive".to_string());
    }

    let g = 9.80665;
    let rho_air = 1.204; // Standard air density at 20°C, sea level (kg/m³)

    // Cruising velocity in m/s
    let v_cruise_m_s = problem.cruising_speed_km_h / 3.6;

    // Road load components:
    // 1. Tire rolling resistance force: F_roll = Crr * M * g
    let f_roll = problem.rolling_resistance_coeff * problem.vehicle_mass_kg * g;

    // 2. Aerodynamic drag force: F_aero = 0.5 * rho * Cd * A * v^2
    let f_aero = 0.5
        * rho_air
        * problem.drag_coefficient
        * problem.frontal_area_m2
        * v_cruise_m_s
        * v_cruise_m_s;

    let f_road_load = f_roll + f_aero;

    // Wheel mechanical power and battery electrical power at cruise
    let p_wheel_cruise_kw = (f_road_load * v_cruise_m_s) / 1000.0;
    // Electrical powertrain combined efficiency (inverter * motor * driveline) ~ 0.90 at cruise
    let combined_drivetrain_eta = problem.driveline_efficiency * 0.94;
    let p_battery_cruise_kw = p_wheel_cruise_kw / combined_drivetrain_eta;

    // Specific energy consumption (Wh/km):
    // E_km = (P_batt_W / (v_km_h)) = (P_batt_kW * 1000) / v_km_h
    let specific_energy_wh_km = (p_battery_cruise_kw * 1000.0) / problem.cruising_speed_km_h;

    // Driving range (km) on 95% usable battery capacity
    let cruising_range_km = (problem.battery_capacity_kwh * 0.95 * 1000.0) / specific_energy_wh_km;

    // Motor rotational speed at cruise:
    // N_motor = (v / (2 * pi * r_tire)) * 60 * i_gear
    let wheel_rev_per_s = v_cruise_m_s / (2.0 * std::f64::consts::PI * problem.tire_radius_m);
    let motor_speed_cruise_rpm = wheel_rev_per_s * 60.0 * problem.gear_ratio;

    // Maximum vehicle speed governed by motor redline:
    let max_wheel_rev_per_s = (problem.motor_max_speed_rpm / problem.gear_ratio) / 60.0;
    let max_speed_m_s = max_wheel_rev_per_s * 2.0 * std::f64::consts::PI * problem.tire_radius_m;
    let max_vehicle_speed_km_h = max_speed_m_s * 3.6;

    // Maximum tractive force at wheels in constant torque region:
    // F_trac_max = (T_motor_max * i_gear * eta_drive) / r_tire
    let f_trac_max =
        (problem.motor_peak_torque_nm * problem.gear_ratio * problem.driveline_efficiency)
            / problem.tire_radius_m;

    // Rotational mass inertia factor (typically 1.05 for EV with single-speed gearbox)
    let mass_factor = 1.05;
    let effective_mass = problem.vehicle_mass_kg * mass_factor;

    // Initial longitudinal acceleration (at v ~ 0, F_aero ~ 0):
    let max_initial_accel = (f_trac_max - f_roll) / effective_mass;

    // Peak motor shaft mechanical power in kW:
    // P_motor_max = (2 * pi * N_base / 60) * T_motor_peak
    let omega_base = (2.0 * std::f64::consts::PI * problem.motor_base_speed_rpm) / 60.0;
    let p_motor_peak_kw = (omega_base * problem.motor_peak_torque_nm) / 1000.0;

    // Numerical integration of 0 to 100 km/h acceleration:
    // dv/dt = (F_traction(v) - F_roll - F_aero(v)) / M_eff
    let v_target = 100.0 / 3.6; // 27.78 m/s
    let mut v_curr = 0.0;
    let mut t_accel = 0.0;
    let dt = 0.02; // 20 ms time step
    while v_curr < v_target && t_accel < 30.0 {
        let n_motor = (v_curr / (2.0 * std::f64::consts::PI * problem.tire_radius_m))
            * 60.0
            * problem.gear_ratio;
        let t_motor = if n_motor <= problem.motor_base_speed_rpm {
            problem.motor_peak_torque_nm
        } else {
            // Constant power field-weakening: T = P_peak / omega
            let omega = (2.0 * std::f64::consts::PI * n_motor.max(1.0)) / 60.0;
            ((p_motor_peak_kw * 1000.0) / omega).min(problem.motor_peak_torque_nm)
        };
        let f_trac =
            (t_motor * problem.gear_ratio * problem.driveline_efficiency) / problem.tire_radius_m;
        let f_aero_t =
            0.5 * rho_air * problem.drag_coefficient * problem.frontal_area_m2 * v_curr * v_curr;
        let f_net = (f_trac - f_roll - f_aero_t).max(10.0);
        let a = f_net / effective_mass;
        v_curr += a * dt;
        t_accel += dt;
    }

    // Regenerative braking mechanics per UNECE R13-H:
    // Target deceleration a_decel (e.g. 2.5 m/s² = ~0.25g service braking)
    let f_brake_demand = problem.vehicle_mass_kg * problem.braking_deceleration_m_s2;

    // Axle dynamic weight transfer:
    // Front dynamic load during braking is higher due to inertia transfer
    let front_axle_fraction = match problem.layout {
        EvDrivetrainLayout::FrontWheelDrive => 0.70, // 70% available on driven axle
        EvDrivetrainLayout::RearWheelDrive => 0.35,  // 35% on rear axle during decel
        EvDrivetrainLayout::AllWheelDrive => 0.95,   // 95% total driven
    };

    let f_brake_on_driven_axle = f_brake_demand * front_axle_fraction;
    // Motor regen torque limit:
    let max_regen_torque = problem.motor_peak_torque_nm * 0.85; // 85% peak in regen mode
    let max_regen_tractive_force = (max_regen_torque * problem.gear_ratio)
        / (problem.tire_radius_m * problem.driveline_efficiency);

    let actual_regen_braking_force = f_brake_on_driven_axle.min(max_regen_tractive_force);
    // Mechanical braking power absorbed at 60 km/h standard evaluation speed
    let v_eval_regen = 60.0 / 3.6;
    let p_regen_mech_kw = (actual_regen_braking_force * v_eval_regen) / 1000.0;
    // Electrical power returned to battery: eta_gen (~0.90) * eta_inv (~0.95) * eta_batt (~0.96)
    let p_regen_elec_kw = p_regen_mech_kw * (0.90 * 0.95 * 0.96);

    let regen_capture_pct =
        (actual_regen_braking_force / f_brake_demand) * (0.90 * 0.95 * 0.96) * 100.0;

    let is_cruise_attainable = max_vehicle_speed_km_h >= problem.cruising_speed_km_h;
    let is_sport_accel = t_accel <= 7.5; // Under 7.5s to 100 km/h
    let is_regen_compliant = regen_capture_pct >= 25.0; // Captures >= 25% deceleration kinetic work

    Ok(EvPowertrainResult {
        problem_id: problem.problem_id.clone(),
        aerodynamic_drag_force_n: f_aero,
        rolling_resistance_force_n: f_roll,
        total_road_load_force_n: f_road_load,
        wheel_power_at_cruise_kw: p_wheel_cruise_kw,
        battery_power_at_cruise_kw: p_battery_cruise_kw,
        cruising_range_km,
        specific_energy_wh_km,
        max_wheel_tractive_force_n: f_trac_max,
        max_initial_acceleration_m_s2: max_initial_accel,
        acceleration_0_to_100_km_h_s: t_accel,
        max_vehicle_speed_km_h,
        motor_speed_at_cruise_rpm: motor_speed_cruise_rpm,
        braking_force_demand_n: f_brake_demand,
        regen_braking_power_kw: p_regen_elec_kw,
        regen_capture_efficiency_pct: regen_capture_pct,
        is_cruising_speed_attainable: is_cruise_attainable,
        is_acceleration_performance_sport: is_sport_accel,
        is_regen_braking_compliant: is_regen_compliant,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_standard_ev_powertrain_performance() -> Result<(), String> {
        let prob = EvPowertrainProblem {
            problem_id: "model_ev_saloon".to_string(),
            layout: EvDrivetrainLayout::RearWheelDrive,
            vehicle_mass_kg: 1850.0,
            rolling_resistance_coeff: 0.009,
            drag_coefficient: 0.24,
            frontal_area_m2: 2.25,
            tire_radius_m: 0.330,
            gear_ratio: 9.0,
            driveline_efficiency: 0.95,
            motor_peak_torque_nm: 350.0,
            motor_base_speed_rpm: 4500.0,
            motor_max_speed_rpm: 16000.0,
            battery_capacity_kwh: 75.0,
            battery_voltage_v: 400.0,
            cruising_speed_km_h: 100.0,
            braking_deceleration_m_s2: 2.5,
        };

        let res = solve_ev_powertrain(&prob)?;
        assert!(res.total_road_load_force_n > 300.0 && res.total_road_load_force_n < 600.0);
        assert!(res.wheel_power_at_cruise_kw > 8.0 && res.wheel_power_at_cruise_kw < 16.0);
        assert!(res.battery_power_at_cruise_kw > 10.0 && res.battery_power_at_cruise_kw < 20.0);
        assert!(res.cruising_range_km > 350.0);
        assert!(res.specific_energy_wh_km > 120.0 && res.specific_energy_wh_km < 200.0);
        assert!(res.max_wheel_tractive_force_n > 8000.0);
        assert!(res.max_initial_acceleration_m_s2 > 4.0);
        assert!(res.acceleration_0_to_100_km_h_s < 7.5);
        assert!(res.max_vehicle_speed_km_h > 180.0);
        assert!(res.regen_braking_power_kw > 15.0);
        assert!(res.is_cruising_speed_attainable);
        assert!(res.is_acceleration_performance_sport);
        assert!(res.is_regen_braking_compliant);
        Ok(())
    }
}
