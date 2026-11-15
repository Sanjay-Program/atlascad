//! Multi-Stage Gas Compression & Intercooling Thermodynamics (ASME PTC 10 / ISO 5389 / GPSA).
//!
//! Evaluates optimal stage pressure ratios, polytropic stage discharge temperatures,
//! intercooler heat rejection duties, total compressor brake horsepower,
//! and energy savings relative to uncooled single-stage compression.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

use crate::convergence::ConvergenceMonitor;
use crate::execution::{SimulationResult, SimulationStatus};
use crate::provenance::SimulationProvenance;
use crate::validation::AnalyticalComparison;

/// Process gas type.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ProcessGas {
    Air,
    MethaneNaturalGas,
    Nitrogen,
    Hydrogen,
    CarbonDioxide,
}

impl ProcessGas {
    /// Specific heat ratio k = Cp / Cv.
    pub fn isentropic_k(&self) -> f64 {
        match self {
            Self::Air => 1.40,
            Self::MethaneNaturalGas => 1.31,
            Self::Nitrogen => 1.40,
            Self::Hydrogen => 1.41,
            Self::CarbonDioxide => 1.29,
        }
    }

    /// Specific gas constant R (J/(kg·K)).
    pub fn gas_constant_r(&self) -> f64 {
        match self {
            Self::Air => 287.05,
            Self::MethaneNaturalGas => 518.28,
            Self::Nitrogen => 296.80,
            Self::Hydrogen => 4124.0,
            Self::CarbonDioxide => 188.92,
        }
    }

    /// Constant pressure specific heat Cp (J/(kg·K)).
    pub fn cp_j_kg_k(&self) -> f64 {
        match self {
            Self::Air => 1005.0,
            Self::MethaneNaturalGas => 2220.0,
            Self::Nitrogen => 1040.0,
            Self::Hydrogen => 14304.0,
            Self::CarbonDioxide => 846.0,
        }
    }
}

/// Compression stage summary data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionStageData {
    pub stage_number: usize,
    pub suction_pressure_bar: f64,
    pub discharge_pressure_bar: f64,
    pub suction_temp_c: f64,
    pub discharge_temp_c: f64,
    pub stage_power_kw: f64,
    pub intercooler_duty_kw: f64,
}

/// Gas compression problem specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GasCompressionProblem {
    /// Problem identifier.
    pub problem_id: String,
    /// Process gas.
    pub gas: ProcessGas,
    /// Gas mass flowrate (kg/s).
    pub mass_flow_kg_s: f64,
    /// Initial suction pressure (bar absolute).
    pub suction_pressure_bar: f64,
    /// Initial suction temperature (°C).
    pub suction_temp_c: f64,
    /// Final target discharge pressure (bar absolute).
    pub target_discharge_pressure_bar: f64,
    /// Number of compression stages (e.g. 1, 2, 3, 4).
    pub number_of_stages: usize,
    /// Polytropic efficiency eta_p (0.0 to 1.0, e.g. 0.82).
    pub polytropic_efficiency: f64,
    /// Mechanical drive efficiency eta_mech (e.g. 0.96).
    pub mechanical_efficiency: f64,
    /// Intercooler exit gas temperature (°C, defaults to suction temp).
    pub intercooler_temp_c: Option<f64>,
    /// Average compressibility factor Z.
    pub compressibility_factor_z: f64,
}

impl Default for GasCompressionProblem {
    fn default() -> Self {
        Self {
            problem_id: "industrial_multistage_air_compressor".to_string(),
            gas: ProcessGas::Air,
            mass_flow_kg_s: 4.5, // ~16,200 kg/hr
            suction_pressure_bar: 1.0, // Ambient suction
            suction_temp_c: 20.0,
            target_discharge_pressure_bar: 30.0, // 30 bar discharge
            number_of_stages: 3,
            polytropic_efficiency: 0.82,
            mechanical_efficiency: 0.96,
            intercooler_temp_c: Some(25.0),
            compressibility_factor_z: 1.0,
        }
    }
}

/// Comprehensive multi-stage gas compression results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GasCompressionResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Overall pressure ratio P_out / P_in.
    pub overall_pressure_ratio: f64,
    /// Equal pressure ratio per stage r_p.
    pub stage_pressure_ratio: f64,
    /// Polytropic exponent n.
    pub polytropic_exponent_n: f64,
    /// Stage discharge temperature (°C).
    pub stage_discharge_temp_c: f64,
    /// Total multi-stage compressor brake shaft power (kW).
    pub total_brake_power_kw: f64,
    /// Total heat removed by all intercoolers (kW).
    pub total_intercooler_duty_kw: f64,
    /// Equivalent single-stage uncooled adiabatic compression power (kW).
    pub single_stage_uncooled_power_kw: f64,
    /// Percentage energy savings achieved by multi-stage intercooling (%).
    pub energy_savings_pct: f64,
    /// Stage by stage detail breakdown.
    pub stages: Vec<CompressionStageData>,
}

/// Solves multi-stage gas compression with intercooling.
pub fn solve_gas_compression(prob: &GasCompressionProblem) -> GasCompressionResult {
    let p_in = prob.suction_pressure_bar.max(0.1);
    let p_out = prob.target_discharge_pressure_bar.max(p_in * 1.01);
    let n_stages = prob.number_of_stages.clamp(1, 8);

    let overall_rp = p_out / p_in;
    // Equal optimal pressure ratio per stage: r_p = overall_rp^(1 / n_stages)
    let rp_stage = overall_rp.powf(1.0 / n_stages as f64);

    let k = prob.gas.isentropic_k();
    let r_gas = prob.gas.gas_constant_r();
    let cp = prob.gas.cp_j_kg_k();
    let eta_p = prob.polytropic_efficiency.clamp(0.40, 0.98);
    let eta_m = prob.mechanical_efficiency.clamp(0.80, 0.99);

    // Polytropic exponent n:
    // (n - 1) / n = (k - 1) / (k * eta_p)
    let poly_ratio = (k - 1.0) / (k * eta_p);
    let poly_n = 1.0 / (1.0 - poly_ratio).max(1e-4);

    let t_in_k = prob.suction_temp_c + 273.15;
    let t_intercool_k = prob.intercooler_temp_c.unwrap_or(prob.suction_temp_c) + 273.15;

    let mut stages_vec = Vec::new();
    let mut total_power_kw = 0.0;
    let mut total_intercooler_kw = 0.0;

    let mut current_p = p_in;

    for i in 1..=n_stages {
        let p_suc = current_p;
        let p_dis = current_p * rp_stage;
        current_p = p_dis;

        let t_suc_k = if i == 1 { t_in_k } else { t_intercool_k };

        // T_dis = T_suc * (r_p)^((n-1)/n)
        let t_dis_k = t_suc_k * rp_stage.powf(poly_ratio);
        let t_dis_c = t_dis_k - 273.15;

        // Specific stage polytropic work:
        // w_stage = (n / (n - 1)) * Z * R * T_suc * [ (r_p)^((n-1)/n) - 1 ] in J/kg
        let w_stage_j_kg = (1.0 / poly_ratio) * prob.compressibility_factor_z * r_gas * t_suc_k * (rp_stage.powf(poly_ratio) - 1.0);
        let power_kw = (prob.mass_flow_kg_s * w_stage_j_kg / 1000.0) / eta_m;
        total_power_kw += power_kw;

        let ic_duty_kw = if i < n_stages {
            let q_ic = (prob.mass_flow_kg_s * cp * (t_dis_k - t_intercool_k).max(0.0)) / 1000.0;
            total_intercooler_kw += q_ic;
            q_ic
        } else {
            0.0
        };

        stages_vec.push(CompressionStageData {
            stage_number: i,
            suction_pressure_bar: p_suc,
            discharge_pressure_bar: p_dis,
            suction_temp_c: t_suc_k - 273.15,
            discharge_temp_c: t_dis_c,
            stage_power_kw: power_kw,
            intercooler_duty_kw: ic_duty_kw,
        });
    }

    // Equivalent single-stage uncooled adiabatic compression:
    let isentropic_ratio = (k - 1.0) / k;
    let single_stage_w_j_kg = (k / (k - 1.0)) * prob.compressibility_factor_z * r_gas * t_in_k * (overall_rp.powf(isentropic_ratio) - 1.0);
    let single_stage_power = (prob.mass_flow_kg_s * single_stage_w_j_kg / 1000.0) / (eta_p * eta_m);

    let energy_savings = ((single_stage_power - total_power_kw) / single_stage_power).max(0.0) * 100.0;

    GasCompressionResult {
        problem_id: prob.problem_id.clone(),
        overall_pressure_ratio: overall_rp,
        stage_pressure_ratio: rp_stage,
        polytropic_exponent_n: poly_n,
        stage_discharge_temp_c: stages_vec[0].discharge_temp_c,
        total_brake_power_kw: total_power_kw,
        total_intercooler_duty_kw: total_intercooler_kw,
        single_stage_uncooled_power_kw: single_stage_power,
        energy_savings_pct: energy_savings,
        stages: stages_vec,
    }
}

/// Simulation runner adapter for unified verification.
pub fn solve_gas_compression_sim(prob: &GasCompressionProblem) -> SimulationResult {
    let sol = solve_gas_compression(prob);
    let mut metrics = std::collections::HashMap::new();
    metrics.insert("total_brake_power_kw".to_string(), sol.total_brake_power_kw);
    metrics.insert("total_intercooler_duty_kw".to_string(), sol.total_intercooler_duty_kw);
    metrics.insert("energy_savings_pct".to_string(), sol.energy_savings_pct);
    metrics.insert("stage_discharge_temp_c".to_string(), sol.stage_discharge_temp_c);

    let mut conv = ConvergenceMonitor::new(1e-6);
    conv.record(1, sol.total_brake_power_kw, 1, 1e-4);

    let comparison = AnalyticalComparison::assess(
        "energy_savings_vs_single_stage",
        sol.energy_savings_pct,
        15.0,
    );

    SimulationResult {
        problem_id: prob.problem_id.clone(),
        engine_id: "atlas.thermal.gas_compression".to_string(),
        status: SimulationStatus::Converged,
        execution_time_ms: 1,
        scalar_metrics: metrics,
        convergence: conv,
        validation: Some(comparison),
        provenance: SimulationProvenance::new("atlas.thermal.gas_compression", "0.1.0"),
        logs: vec![format!("Overall ratio: {:.1}:1, 3-Stage Power: {:.1} kW (vs {:.1} kW single stage, Savings: {:.1}%)", sol.overall_pressure_ratio, sol.total_brake_power_kw, sol.single_stage_uncooled_power_kw, sol.energy_savings_pct)],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_multistage_air_compressor_power_and_intercooling() {
        let prob = GasCompressionProblem::default();
        let sol = solve_gas_compression(&prob);

        assert_eq!(sol.stages.len(), 3);
        // Stage pressure ratio for 30:1 in 3 stages is 30^(1/3) ~ 3.107
        assert!((sol.stage_pressure_ratio - 3.107).abs() < 1e-2);
        // Stage discharge temperature for 3.1 ratio is ~140 to 170 °C
        assert!(sol.stage_discharge_temp_c > 130.0 && sol.stage_discharge_temp_c < 180.0);
        // Multi-stage compression power is ~1300 to 1800 kW
        assert!(sol.total_brake_power_kw > 1200.0 && sol.total_brake_power_kw < 1900.0);
        // Single-stage uncooled would be ~2000 to 2500 kW
        assert!(sol.single_stage_uncooled_power_kw > sol.total_brake_power_kw);
        // Intercooling savings ~20% to 35%
        assert!(sol.energy_savings_pct > 18.0 && sol.energy_savings_pct < 40.0);
        assert!(sol.total_intercooler_duty_kw > 800.0);
    }
}
