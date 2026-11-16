// crates/atlas-simulation-core/src/crash_deceleration.rs
//
// Automotive Safety & Crashworthiness Engineering:
// Vehicle Frontal Offset Crash Impulse and Occupant Deceleration Severity Solver.
//
// Based on:
// - ECE R94: Frontal Collision with Deformable Barrier (40% offset, 56/64 km/h)
// - FMVSS 208: Occupant Crash Protection (NHTSA, USA)
// - SAE J211-1: Instrumentation for Impact Test (CFC 1000 / CFC 180 filter classes)
// - Prasad & Mertz (1985): "The Position of the United States Delegation to the ISO Working Group 6 on the Evaluation of Head Injury Criterion"
// - Euro NCAP Frontal Impact Assessment Protocol
//
// Strictly forbids unsafe code.

use serde::{Deserialize, Serialize};

fn default_problem_id() -> String {
    "vehicle_crash_deceleration_problem".to_string()
}

/// Frontal crash impact scenario configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum CrashScenario {
    /// 40% Overlap Offset Deformable Barrier at 64 km/h (Euro NCAP / ECE R94).
    #[default]
    OffsetDeformableBarrier40Pct,
    /// Full-width Rigid Barrier at 50/56 km/h (FMVSS 208 / ECE R12).
    FullWidthRigidBarrier,
    /// Small Overlap Rigid Barrier at 64 km/h (IIHS 25% driver-side).
    SmallOverlapRigidBarrier25Pct,
}

/// Occupant restraint system type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum RestraintSystemType {
    /// Standard 3-point seatbelt with dual pyrotechnic pretensioner and 4.0 kN load limiter.
    #[default]
    BeltPretensionerWithLoadLimiter,
    /// 3-point belt with dual-stage frontal airbag and adaptive 3.5 kN load limiter.
    AdvancedAirbagAndAdaptiveBelt,
    /// Conventional 3-point belt without active pretensioner (legacy baseline).
    ConventionalThreePointBelt,
}

/// Frontal crash simulation problem specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrashDecelerationProblem {
    /// Problem identifier.
    #[serde(default = "default_problem_id")]
    pub problem_id: String,
    /// Impact crash scenario.
    #[serde(default)]
    pub scenario: CrashScenario,
    /// Restraint system technology.
    #[serde(default)]
    pub restraint: RestraintSystemType,
    /// Vehicle total test mass in kilograms (kg, curb + 50th percentile dummy).
    pub vehicle_mass_kg: f64,
    /// Initial impact velocity in kilometers per hour (km/h, typically 56.0 or 64.0).
    pub impact_velocity_km_h: f64,
    /// Maximum available structural crumple zone crush space in meters (m).
    pub crumple_zone_length_m: f64,
    /// Front structural crash rail mean plastic collapse force in kiloNewtons (kN).
    pub crash_box_mean_force_kn: f64,
    /// Structural crash energy absorption efficiency factor eta (0.60 - 0.85).
    pub structural_energy_efficiency: f64,
    /// Dummy occupant mass in kilograms (kg, 50th percentile Hybrid III male ~ 78.0 kg).
    pub occupant_mass_kg: f64,
}

impl Default for CrashDecelerationProblem {
    fn default() -> Self {
        Self {
            problem_id: default_problem_id(),
            scenario: CrashScenario::OffsetDeformableBarrier40Pct,
            restraint: RestraintSystemType::AdvancedAirbagAndAdaptiveBelt,
            vehicle_mass_kg: 1650.0,
            impact_velocity_km_h: 64.0,
            crumple_zone_length_m: 0.72,
            crash_box_mean_force_kn: 310.0,
            structural_energy_efficiency: 0.75,
            occupant_mass_kg: 78.0,
        }
    }
}

/// Frontal crash deceleration and injury metric result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrashDecelerationResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Initial kinetic energy before collision in kiloJoules (kJ).
    pub initial_kinetic_energy_kj: f64,
    /// Actual dynamic crush displacement of the front structure in millimeters (mm).
    pub dynamic_crush_distance_mm: f64,
    /// Total crash pulse duration until vehicle stoppage in milliseconds (ms).
    pub crash_pulse_duration_ms: f64,
    /// Mean vehicle deceleration over crash duration in Earth gravities (g).
    pub mean_vehicle_deceleration_g: f64,
    /// Peak vehicle deceleration during structural collapse in Earth gravities (g).
    pub peak_vehicle_deceleration_g: f64,
    /// Peak seatbelt restraint shoulder force on occupant in kiloNewtons (kN).
    pub peak_seatbelt_force_kn: f64,
    /// Occupant chest deceleration (3 ms cumulative clip) in Earth gravities (g).
    pub occupant_chest_deceleration_3ms_g: f64,
    /// Estimated occupant chest sternal deflection in millimeters (mm).
    pub occupant_chest_deflection_mm: f64,
    /// Head Injury Criterion over 15 ms window (HIC15).
    pub head_injury_criterion_hic15: f64,
    /// Prasad-Mertz probability of severe (AIS 4+) head trauma (%).
    pub ais4_plus_injury_probability_pct: f64,
    /// Whether dynamic crush is contained within available crumple length (no passenger cell intrusion).
    pub is_crumple_zone_sufficient: bool,
    /// Whether HIC15 satisfies FMVSS 208 and Euro NCAP limit (HIC15 <= 700).
    pub is_hic15_compliant: bool,
    /// Whether chest deflection satisfies ECE R94 limit (Deflection <= 42 mm).
    pub is_chest_deflection_compliant: bool,
}

/// Solves vehicle crash deceleration pulse, occupant kinematics, and injury criteria.
pub fn solve_crash_deceleration(
    problem: &CrashDecelerationProblem,
) -> Result<CrashDecelerationResult, String> {
    if problem.vehicle_mass_kg <= 200.0 || problem.vehicle_mass_kg >= 15000.0 {
        return Err("Vehicle mass must be between 200 and 15000 kg".to_string());
    }
    if problem.impact_velocity_km_h <= 5.0 || problem.impact_velocity_km_h >= 160.0 {
        return Err("Impact velocity must be between 5 and 160 km/h".to_string());
    }
    if problem.crumple_zone_length_m <= 0.10 || problem.crash_box_mean_force_kn <= 10.0 {
        return Err("Crumple zone length and crash box force must be positive engineering values".to_string());
    }

    let g_accel = 9.80665;
    let v_0 = problem.impact_velocity_km_h * (1000.0 / 3600.0); // m/s
    let e_k_j = 0.5 * problem.vehicle_mass_kg * v_0 * v_0;
    let e_k_kj = e_k_j / 1000.0;

    // Structural effective stiffness and crush force accounting for scenario offset and deformable barrier energy absorption:
    let (barrier_absorption_ratio, scenario_force_factor) = match problem.scenario {
        CrashScenario::OffsetDeformableBarrier40Pct => (0.25, 1.10), // barrier absorbs 25% of KE
        CrashScenario::FullWidthRigidBarrier => (0.00, 1.40),      // rigid wall, both rails engage
        CrashScenario::SmallOverlapRigidBarrier25Pct => (0.00, 0.70), // narrow offset engagement
    };

    let e_vehicle_j = e_k_j * (1.0 - barrier_absorption_ratio);
    let f_mean_n = problem.crash_box_mean_force_kn * 1000.0 * scenario_force_factor;
    let eta_struct = problem.structural_energy_efficiency.clamp(0.50, 0.90);

    // Dynamic crush distance: E_vehicle = eta * F_mean * s_crush
    let s_crush_m = e_vehicle_j / (eta_struct * f_mean_n);
    let s_crush_mm = s_crush_m * 1000.0;

    // Haversine/half-sine deceleration pulse model:
    // Mean deceleration: a_mean = v_0^2 / (2 * s_crush)
    let a_mean_m_s2 = (v_0 * v_0) / (2.0 * s_crush_m);
    let a_mean_g = a_mean_m_s2 / g_accel;

    // Pulse duration for half-sine deceleration:
    // v(t) = v_0 - \int a_peak * sin(pi * t / T) dt = 0 at t = T => a_peak = (pi / 2) * a_mean
    // s_crush = v_0 * T / 2 => T = (2 * s_crush) / v_0 * (pi / 2) ?
    // Exact for a(t) = a_peak * sin(pi * t / T):
    // integral_0^T a(t) dt = 2 a_peak T / pi = v_0 => a_peak = pi * v_0 / (2 T)
    // integral_0^T v(t) dt = v_0 * T / 2 = s_crush => T = 2 * s_crush / v_0
    let t_pulse_s = (2.0 * s_crush_m) / v_0;
    let t_pulse_ms = t_pulse_s * 1000.0;
    let a_peak_m_s2 = (std::f64::consts::PI * v_0) / (2.0 * t_pulse_s);
    let a_peak_g = a_peak_m_s2 / g_accel;

    // Occupant restraint coupling:
    // Restraint belt load limiter cap and airbag cushioning:
    let (belt_load_limit_kn, chest_coupling_factor, head_accel_factor): (f64, f64, f64) = match problem.restraint {
        RestraintSystemType::AdvancedAirbagAndAdaptiveBelt => (3.5, 0.65, 0.70),
        RestraintSystemType::BeltPretensionerWithLoadLimiter => (4.0, 0.75, 0.85),
        RestraintSystemType::ConventionalThreePointBelt => (6.5, 1.10, 1.35),
    };

    let f_belt_kn = belt_load_limit_kn.min(problem.occupant_mass_kg * a_peak_m_s2 / 1000.0);

    // Occupant chest acceleration (3 ms clip):
    let a_chest_g = (a_mean_g * chest_coupling_factor + (f_belt_kn * 1000.0 / problem.occupant_mass_kg) / g_accel * 0.5)
        .clamp(15.0, 75.0);

    // Chest deflection (mm) based on chest force and sternal stiffness (~ 0.25 kN/mm):
    let sternal_stiffness_kn_per_mm = 0.18;
    let d_chest_mm = (f_belt_kn / sternal_stiffness_kn_per_mm).clamp(10.0, 65.0);

    // Head acceleration pulse peak:
    let a_head_peak_g = (a_peak_g * head_accel_factor).clamp(25.0, 110.0);

    // Head Injury Criterion over 15 ms window (HIC15):
    // HIC15 = (t2 - t1) * [1 / (t2 - t1) * \int a(t) dt]^2.5
    // For a centered 15 ms window over the peak of a half-sine wave of duration T_pulse:
    // If T_pulse ~ 80-120 ms, delta_t = 0.015 s.
    // The average acceleration over 15 ms centered at the peak is ~ 0.95 * a_head_peak_g:
    let dt_hic = 0.015; // 15 ms in seconds
    let a_head_avg_15ms_g = a_head_peak_g * 0.94;
    let hic15 = dt_hic * a_head_avg_15ms_g.powf(2.5);

    // Prasad-Mertz AIS 4+ (Severe Brain Injury) risk curve:
    // Logistic function: P(AIS 4+) = 1 / [1 + exp(4.39 - 0.0051 * HIC15)] * 100%
    let logit = 4.39 - 0.0051 * hic15;
    let p_ais4_pct = (1.0 / (1.0 + logit.exp())) * 100.0;

    let is_crumple_ok = s_crush_m <= problem.crumple_zone_length_m;
    let is_hic_ok = hic15 <= 700.0; // FMVSS 208 / Euro NCAP threshold
    let is_chest_ok = d_chest_mm <= 42.0; // ECE R94 limit

    Ok(CrashDecelerationResult {
        problem_id: problem.problem_id.clone(),
        initial_kinetic_energy_kj: e_k_kj,
        dynamic_crush_distance_mm: s_crush_mm,
        crash_pulse_duration_ms: t_pulse_ms,
        mean_vehicle_deceleration_g: a_mean_g,
        peak_vehicle_deceleration_g: a_peak_g,
        peak_seatbelt_force_kn: f_belt_kn,
        occupant_chest_deceleration_3ms_g: a_chest_g,
        occupant_chest_deflection_mm: d_chest_mm,
        head_injury_criterion_hic15: hic15,
        ais4_plus_injury_probability_pct: p_ais4_pct,
        is_crumple_zone_sufficient: is_crumple_ok,
        is_hic15_compliant: is_hic_ok,
        is_chest_deflection_compliant: is_chest_ok,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_offset_deformable_barrier_crash() -> Result<(), String> {
        let prob = CrashDecelerationProblem {
            problem_id: "euro_ncap_sedan_64kph".to_string(),
            scenario: CrashScenario::OffsetDeformableBarrier40Pct,
            restraint: RestraintSystemType::AdvancedAirbagAndAdaptiveBelt,
            vehicle_mass_kg: 1650.0,
            impact_velocity_km_h: 64.0,
            crumple_zone_length_m: 0.75,
            crash_box_mean_force_kn: 320.0,
            structural_energy_efficiency: 0.78,
            occupant_mass_kg: 78.0,
        };

        let res = solve_crash_deceleration(&prob)?;
        assert!(res.initial_kinetic_energy_kj > 250.0 && res.initial_kinetic_energy_kj < 300.0);
        assert!(res.dynamic_crush_distance_mm > 500.0 && res.dynamic_crush_distance_mm < 750.0);
        assert!(res.crash_pulse_duration_ms > 60.0 && res.crash_pulse_duration_ms < 110.0);
        assert!(res.mean_vehicle_deceleration_g > 15.0 && res.mean_vehicle_deceleration_g < 35.0);
        assert!(res.peak_vehicle_deceleration_g > 25.0 && res.peak_vehicle_deceleration_g < 55.0);
        assert!(res.head_injury_criterion_hic15 < 700.0);
        assert!(res.occupant_chest_deflection_mm < 42.0);
        assert!(res.is_crumple_zone_sufficient);
        assert!(res.is_hic15_compliant);
        assert!(res.is_chest_deflection_compliant);
        assert!(res.ais4_plus_injury_probability_pct < 10.0);
        Ok(())
    }
}
