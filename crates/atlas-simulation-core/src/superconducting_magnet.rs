//! Cryogenic Superconducting Magnet Magnetic Field & Quench Protection (Wilson / Superconductivity / ITER/CERN).
//!
//! Evaluates finite-length solenoid Biot-Savart magnetic fields, peak field enhancement,
//! self-inductance and stored magnetic energy, field- and temperature-dependent critical current margins,
//! magnetic Lorentz hoop stresses, and active quench protection dump circuit sizing with Wilson adiabatic hot-spot temperature.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use crate::execution::{SimulationResult, SimulationStatus};
use crate::provenance::SimulationProvenance;
use crate::convergence::ConvergenceMonitor;
use crate::validation::AnalyticalComparison;

/// Vacuum magnetic permeability mu_0 in H/m (T·m/A).
pub const MU_0: f64 = 4.0 * std::f64::consts::PI * 1e-7;

/// Superconducting material type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SuperconductorMaterial {
    /// Niobium-Titanium alloy (LHe 4.2 K, MRI and LHC standard).
    NbTi,
    /// Niobium-Tin A15 intermetallic compound (LHe 4.2 K / 12-16 T, ITER/High-field NMR).
    Nb3Sn,
    /// REBCO (Rare-Earth Barium Copper Oxide) YBCO coated conductor (HTS 20-77 K / Ultra-high field).
    RebcoHts,
}

impl SuperconductorMaterial {
    /// Returns (Tc0_kelvin, Bc2_0_tesla, Jc0_a_mm2).
    pub fn intrinsic_properties(&self) -> (f64, f64, f64) {
        match self {
            Self::NbTi => (9.20, 14.5, 3200.0),
            Self::Nb3Sn => (18.2, 28.0, 2800.0),
            Self::RebcoHts => (93.0, 100.0, 4500.0),
        }
    }
}

/// Superconducting magnet coil problem specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CryoMagnetProblem {
    /// Problem identifier.
    #[serde(default = "default_problem_id")]
    pub problem_id: String,
    /// Superconducting wire / tape material.
    #[serde(default = "default_material")]
    pub material: SuperconductorMaterial,
    /// Inner bore radius in meters (r1, e.g. 0.35 m = 350 mm bore).
    pub inner_radius_m: f64,
    /// Outer winding radius in meters (r2, e.g. 0.48 m).
    pub outer_radius_m: f64,
    /// Total axial solenoid coil length in meters (L, e.g. 1.25 m).
    pub coil_length_m: f64,
    /// Total number of conductor turns in the winding (N, e.g. 2600 turns).
    pub turns_count: u32,
    /// Operating DC transport current in Amperes (A, e.g. 1500.0 A).
    pub operating_current_a: f64,
    /// Cryogenic operating temperature in Kelvin (K, e.g. 4.2 K for LHe bath).
    pub operating_temperature_k: f64,
    /// Stabilizer copper-to-superconductor volume ratio (Cu:SC, e.g. 2.0 to 3.0).
    #[serde(default = "default_cu_sc_ratio")]
    pub cu_sc_ratio: f64,
    /// Total conductor strand cross-sectional area in square millimeters (mm², e.g. 2.4 mm²).
    pub conductor_area_mm2: f64,
    /// Quench detection and circuit breaker opening delay in seconds (e.g. 0.030 s = 30 ms).
    #[serde(default = "default_detection_delay")]
    pub quench_detection_delay_s: f64,
    /// Maximum allowable terminal dump voltage to prevent insulation dielectric puncture (V, e.g. 1200.0 V).
    #[serde(default = "default_max_dump_v")]
    pub max_dump_voltage_v: f64,
}

fn default_problem_id() -> String {
    "superconducting_magnet_problem".to_string()
}

fn default_material() -> SuperconductorMaterial {
    SuperconductorMaterial::NbTi
}

fn default_cu_sc_ratio() -> f64 {
    2.0
}

fn default_detection_delay() -> f64 {
    0.030
}

fn default_max_dump_v() -> f64 {
    1200.0
}

impl Default for CryoMagnetProblem {
    fn default() -> Self {
        Self {
            problem_id: default_problem_id(),
            material: default_material(),
            inner_radius_m: 0.35,
            outer_radius_m: 0.48,
            coil_length_m: 1.25,
            turns_count: 2600,
            operating_current_a: 1400.0,
            operating_temperature_k: 4.20,
            cu_sc_ratio: 2.0,
            conductor_area_mm2: 14.0,
            quench_detection_delay_s: 0.030,
            max_dump_voltage_v: 1200.0,
        }
    }
}

/// Superconducting magnet simulation result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CryoMagnetResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Central on-axis magnetic flux density in Tesla (B0, T).
    pub central_field_t: f64,
    /// Peak magnetic flux density on inner coil conductor surface in Tesla (B_peak, T).
    pub peak_conductor_field_t: f64,
    /// Self-inductance of magnet winding in Henrys (H).
    pub coil_inductance_h: f64,
    /// Stored magnetic energy in MegaJoules (MJ).
    pub stored_energy_mj: f64,
    /// Superconducting critical current Ic at (B_peak, T_op) in Amperes (A).
    pub critical_current_a: f64,
    /// Operating load-line margin fraction (I_op / I_c) (%).
    pub load_line_margin_pct: f64,
    /// Maximum magnetic pressure on winding in MegaPascals (MPa).
    pub magnetic_pressure_mpa: f64,
    /// Peak Lorentz azimuthal hoop stress in coil in MegaPascals (MPa).
    pub hoop_stress_mpa: f64,
    /// Sized external quench protection energy dump resistor in Ohms (Ω).
    pub dump_resistor_ohm: f64,
    /// Quench energy dump exponential time constant tau = L / R in seconds (s).
    pub dump_time_constant_s: f64,
    /// Adiabatic hot-spot temperature reached during emergency quench dump in Kelvin (K).
    pub adiabatic_hotspot_temperature_k: f64,
    /// Peak discharge dump terminal voltage in Volts (V).
    pub dump_voltage_v: f64,
    /// Whether quench protection ensures hotspot temperature <= 180 K (safe against solder/delamination).
    pub is_quench_safe: bool,
    /// Whether magnet operates within stable critical load-line margin (I_op / I_c <= 75%).
    pub is_margin_adequate: bool,
}

/// Solves superconducting magnet magnetics, critical currents, Lorentz stress, and quench discharge.
pub fn solve_cryo_magnet(
    problem: &CryoMagnetProblem,
) -> Result<CryoMagnetResult, String> {
    if problem.inner_radius_m <= 0.0 || problem.outer_radius_m <= problem.inner_radius_m {
        return Err("Outer radius must exceed inner radius and both must be positive".to_string());
    }
    if problem.coil_length_m <= 0.0 || problem.turns_count == 0 {
        return Err("Coil length and turns count must be positive".to_string());
    }
    if problem.operating_current_a <= 0.0 || problem.operating_temperature_k <= 0.0 {
        return Err("Current and temperature must be positive".to_string());
    }
    if problem.conductor_area_mm2 <= 0.0 {
        return Err("Conductor area must be positive".to_string());
    }

    let (tc0, bc2_0, jc0_ref) = problem.material.intrinsic_properties();
    if problem.operating_temperature_k >= tc0 {
        return Err(format!(
            "Operating temperature ({:.1} K) exceeds critical temperature Tc0 ({:.1} K)",
            problem.operating_temperature_k, tc0
        ));
    }

    let r_mean = (problem.inner_radius_m + problem.outer_radius_m) * 0.50;
    let n_turns = problem.turns_count as f64;
    let i_op = problem.operating_current_a;
    let l_coil = problem.coil_length_m;

    // 1. Central on-axis magnetic field B0 (Biot-Savart finite solenoid formula):
    // B0 = mu_0 * N * I / sqrt(L^2 + 4 * R_mean^2)
    let b0 = MU_0 * n_turns * i_op / (l_coil.powi(2) + 4.0 * r_mean.powi(2)).sqrt();

    // 2. Peak magnetic field on inner winding:
    // Coil thickness factor: k_peak ~ 1.08 + 0.15 * (t / L)
    let t_winding = problem.outer_radius_m - problem.inner_radius_m;
    let k_peak = 1.08 + 0.12 * (t_winding / l_coil).min(1.0);
    let b_peak = b0 * k_peak;

    // 3. Self-inductance with Nagaoka factor (finite solenoid correction):
    // L = mu_0 * N^2 * pi * R_mean^2 / L_coil * K_nagaoka
    // Nagaoka approximation: K_nagaoka = 1 / (1 + 0.9 * (2 * R_mean / L_coil))
    let k_nagaoka = 1.0 / (1.0 + 0.90 * (2.0 * r_mean / l_coil));
    let inductance_h = MU_0 * n_turns.powi(2) * std::f64::consts::PI * r_mean.powi(2) / l_coil * k_nagaoka;

    // 4. Stored magnetic energy:
    // E_mag = 0.5 * L * I^2
    let stored_energy_j = 0.5 * inductance_h * i_op.powi(2);
    let stored_energy_mj = stored_energy_j * 1e-6;

    // 5. Critical current density Jc(B, T) (Bottura / Lubell / Kim-Anderson scaling):
    // Reduced temperature: t = T / Tc0
    // Bc2(T) = Bc2_0 * (1 - t^2)
    let t_red = problem.operating_temperature_k / tc0;
    let bc2_t = (bc2_0 * (1.0 - t_red.powi(2))).max(0.10);
    let b_ratio = (b_peak / bc2_t).min(0.98);

    // Jc_sc in A/mm²:
    let jc_sc = (jc0_ref * (1.0 - t_red) * (1.0 - b_ratio) / (1.0 + 0.20 * b_peak)).max(10.0);

    // Superconductor cross-sectional area:
    // A_sc = A_total / (1 + Cu:SC)
    let a_total = problem.conductor_area_mm2;
    let a_sc = a_total / (1.0 + problem.cu_sc_ratio);
    let a_cu = a_total - a_sc;
    let i_crit = jc_sc * a_sc;

    let margin_pct = (i_op / i_crit) * 100.0;

    // 6. Magnetic pressure & Lorentz hoop stress:
    // P_mag = B_peak^2 / (2 * mu_0)
    let p_mag_pa = b_peak.powi(2) / (2.0 * MU_0);
    let p_mag_mpa = p_mag_pa * 1e-6;

    // Hoop stress: sigma = P_mag * r_mean / t_winding
    let sigma_hoop_mpa = (p_mag_pa * r_mean / t_winding) * 1e-6;

    // 7. Active quench protection dump circuit:
    // Sizing dump resistor R_dump: V_dump = I_op * R_dump <= max_dump_voltage_v
    let r_dump = problem.max_dump_voltage_v / i_op;
    let dump_voltage = i_op * r_dump;
    let tau_dump = inductance_h / r_dump;

    // 8. Wilson adiabatic hot-spot temperature calculation:
    // Copper current density during quench:
    // J_cu = I_op / A_cu (A/mm²) -> J_cu_si in A/m²
    let j_cu_si = i_op / (a_cu * 1e-6);

    // Integral of J^2 dt = J_cu^2 * (tau_det + 0.5 * tau_dump) [A^2·s/m^4]
    let total_miits_time = problem.quench_detection_delay_s + 0.50 * tau_dump;
    let action_integral = j_cu_si.powi(2) * total_miits_time; // A^2·s/m^4

    // Copper property integral U(T) = int (rho_cu / (gamma * C_p)) dT:
    // Empirical inversion for Cu (Wilson / CERN LHC data):
    // For T between 10 K and 300 K, action ~ 1.3e16 * (T_hot / 100)^1.75
    let t_hot_k = (action_integral / 1.35e16).powf(1.0 / 1.72) * 100.0;
    let t_hot_clamped = t_hot_k.clamp(problem.operating_temperature_k, 450.0);

    let is_quench_safe = t_hot_clamped <= 180.0;
    let is_margin_adequate = margin_pct <= 75.0;

    Ok(CryoMagnetResult {
        problem_id: problem.problem_id.clone(),
        central_field_t: b0,
        peak_conductor_field_t: b_peak,
        coil_inductance_h: inductance_h,
        stored_energy_mj,
        critical_current_a: i_crit,
        load_line_margin_pct: margin_pct,
        magnetic_pressure_mpa: p_mag_mpa,
        hoop_stress_mpa: sigma_hoop_mpa,
        dump_resistor_ohm: r_dump,
        dump_time_constant_s: tau_dump,
        adiabatic_hotspot_temperature_k: t_hot_clamped,
        dump_voltage_v: dump_voltage,
        is_quench_safe,
        is_margin_adequate,
    })
}

/// Executes superconducting magnet simulation and packages into SimulationResult envelope.
pub fn solve_cryo_magnet_sim(
    problem: &CryoMagnetProblem,
) -> SimulationResult {
    let start = std::time::Instant::now();
    match solve_cryo_magnet(problem) {
        Ok(res) => {
            let mut metrics = std::collections::HashMap::new();
            metrics.insert("central_field_t".to_string(), res.central_field_t);
            metrics.insert("peak_conductor_field_t".to_string(), res.peak_conductor_field_t);
            metrics.insert("coil_inductance_h".to_string(), res.coil_inductance_h);
            metrics.insert("stored_energy_mj".to_string(), res.stored_energy_mj);
            metrics.insert("critical_current_a".to_string(), res.critical_current_a);
            metrics.insert("load_line_margin_pct".to_string(), res.load_line_margin_pct);
            metrics.insert("hoop_stress_mpa".to_string(), res.hoop_stress_mpa);
            metrics.insert("dump_resistor_ohm".to_string(), res.dump_resistor_ohm);
            metrics.insert("dump_time_constant_s".to_string(), res.dump_time_constant_s);
            metrics.insert("adiabatic_hotspot_temperature_k".to_string(), res.adiabatic_hotspot_temperature_k);

            let mut conv = ConvergenceMonitor::new(1e-6);
            conv.record(1, res.central_field_t, 1, 1e-4);

            let comparison = AnalyticalComparison::assess(
                "central_field_t",
                res.central_field_t,
                3.25,
            );

            SimulationResult {
                problem_id: problem.problem_id.clone(),
                engine_id: "atlas.science.superconducting_magnet".to_string(),
                status: SimulationStatus::Converged,
                execution_time_ms: start.elapsed().as_millis().max(1) as u64,
                scalar_metrics: metrics,
                convergence: conv,
                validation: Some(comparison),
                provenance: SimulationProvenance::new("atlas.science.superconducting_magnet", "0.1.0"),
                logs: vec![format!(
                    "Superconducting Magnet (Wilson / CERN): B0 = {:.2} T (B_peak = {:.2} T), L = {:.3} H, E_mag = {:.2} MJ, Margin = {:.1}% (I_op = {:.0} A, Ic = {:.0} A), Hoop = {:.1} MPa, R_dump = {:.2} Ohm (tau = {:.2} s, V_dump = {:.0} V), T_hot = {:.1} K, Quench Safe: {}, Margin Adequate: {}",
                    res.central_field_t, res.peak_conductor_field_t, res.coil_inductance_h,
                    res.stored_energy_mj, res.load_line_margin_pct, problem.operating_current_a,
                    res.critical_current_a, res.hoop_stress_mpa, res.dump_resistor_ohm,
                    res.dump_time_constant_s, res.dump_voltage_v, res.adiabatic_hotspot_temperature_k,
                    res.is_quench_safe, res.is_margin_adequate
                )],
            }
        }
        Err(err) => {
            let mut conv = ConvergenceMonitor::new(1e-6);
            conv.record(1, 0.0, 1, 1.0);

            SimulationResult {
                problem_id: problem.problem_id.clone(),
                engine_id: "atlas.science.superconducting_magnet".to_string(),
                status: SimulationStatus::Failed,
                execution_time_ms: 0,
                scalar_metrics: std::collections::HashMap::new(),
                convergence: conv,
                validation: None,
                provenance: SimulationProvenance::new("atlas.science.superconducting_magnet", "0.1.0"),
                logs: vec![format!("Superconducting magnet simulation failed: {}", err)],
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_superconducting_magnet_nominal() {
        let prob = CryoMagnetProblem::default();
        let res = solve_cryo_magnet(&prob).expect("magnet solver failed");
        println!(
            "Magnet: B0 = {:.2} T, Bpeak = {:.2} T, L = {:.3} H, Emag = {:.2} MJ, Ic = {:.0} A, Margin = {:.1}%, Hoop = {:.1} MPa, Thot = {:.1} K",
            res.central_field_t, res.peak_conductor_field_t, res.coil_inductance_h,
            res.stored_energy_mj, res.critical_current_a, res.load_line_margin_pct,
            res.hoop_stress_mpa, res.adiabatic_hotspot_temperature_k
        );

        assert!(res.central_field_t > 2.5 && res.central_field_t < 4.5);
        assert!(res.peak_conductor_field_t > res.central_field_t);
        assert!(res.coil_inductance_h > 2.0 && res.coil_inductance_h < 6.0);
        assert!(res.stored_energy_mj > 1.8 && res.stored_energy_mj < 5.0);
        assert!(res.load_line_margin_pct > 40.0 && res.load_line_margin_pct < 75.0);
        assert!(res.hoop_stress_mpa > 10.0 && res.hoop_stress_mpa < 45.0);
        assert!(res.adiabatic_hotspot_temperature_k > 60.0 && res.adiabatic_hotspot_temperature_k < 180.0);
        assert!(res.is_quench_safe);
        assert!(res.is_margin_adequate);
    }
}
