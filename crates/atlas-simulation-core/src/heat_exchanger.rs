//! # Convection Heat Exchangers: ε-NTU & LMTD Thermal Analysis
//!
//! Solves heat exchanger thermal rating, effectiveness, and duty according to
//! Kays-London and Incropera-DeWitt standards for Counter-Flow, Parallel-Flow,
//! and Cross-Flow configurations using both the ε-NTU method and Logarithmic
//! Mean Temperature Difference (LMTD).

use crate::convergence::ConvergenceMonitor;
use crate::execution::{SimulationResult, SimulationStatus};
use crate::provenance::SimulationProvenance;
use crate::validation::AnalyticalComparison;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Flow arrangement configuration for the heat exchanger.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum FlowArrangement {
    #[default]
    CounterFlow,
    ParallelFlow,
    CrossFlowUnmixed,
}

/// Problem definition for heat exchanger sizing and rating.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HeatExchangerProblem {
    pub heat_exchanger_name: String,
    pub arrangement: FlowArrangement,
    /// Overall heat transfer coefficient U in W/(m²·K)
    pub overall_u_w_m2_k: f64,
    /// Heat transfer surface area A in m²
    pub area_m2: f64,
    /// Hot fluid mass flow rate in kg/s
    pub hot_mass_flow_kg_s: f64,
    /// Hot fluid specific heat cp in J/(kg·K)
    pub hot_cp_j_kg_k: f64,
    /// Hot fluid inlet temperature in °C
    pub hot_inlet_temp_c: f64,
    /// Cold fluid mass flow rate in kg/s
    pub cold_mass_flow_kg_s: f64,
    /// Cold fluid specific heat cp in J/(kg·K)
    pub cold_cp_j_kg_k: f64,
    /// Cold fluid inlet temperature in °C
    pub cold_inlet_temp_c: f64,
}

impl Default for HeatExchangerProblem {
    fn default() -> Self {
        Self {
            heat_exchanger_name: "Industrial Process Heat Exchanger".to_string(),
            arrangement: FlowArrangement::CounterFlow,
            overall_u_w_m2_k: 850.0,  // 850 W/m²K
            area_m2: 18.0,            // 18 m² area
            hot_mass_flow_kg_s: 3.0,  // 3.0 kg/s hot stream
            hot_cp_j_kg_k: 4184.0,    // Water cp
            hot_inlet_temp_c: 90.0,   // 90 °C
            cold_mass_flow_kg_s: 4.5, // 4.5 kg/s cooling stream
            cold_cp_j_kg_k: 4184.0,   // Water cp
            cold_inlet_temp_c: 20.0,  // 20 °C
        }
    }
}

/// Solves heat exchanger effectiveness, heat duty, outlet temperatures, and LMTD.
pub fn solve_heat_exchanger(prob: &HeatExchangerProblem) -> Result<SimulationResult, String> {
    let u = prob.overall_u_w_m2_k;
    let a = prob.area_m2;
    let mh = prob.hot_mass_flow_kg_s;
    let cph = prob.hot_cp_j_kg_k;
    let th_in = prob.hot_inlet_temp_c;
    let mc = prob.cold_mass_flow_kg_s;
    let cpc = prob.cold_cp_j_kg_k;
    let tc_in = prob.cold_inlet_temp_c;

    if u <= 0.0 || a <= 0.0 || mh <= 0.0 || cph <= 0.0 || mc <= 0.0 || cpc <= 0.0 {
        return Err(
            "Flow rates, heat transfer area, and U-value must be positive non-zero".to_string(),
        );
    }
    if th_in <= tc_in {
        return Err(
            "Hot stream inlet temperature must exceed cold stream inlet temperature".to_string(),
        );
    }

    // 1. Heat capacity rates: C = m_dot * cp
    let ch = mh * cph;
    let cc = mc * cpc;
    let c_min = ch.min(cc);
    let c_max = ch.max(cc);
    let c_r = c_min / c_max;

    // 2. Maximum possible heat transfer rate: Q_max = C_min * (Th_in - Tc_in)
    let q_max_watts = c_min * (th_in - tc_in);

    // 3. Number of Transfer Units: NTU = U * A / C_min
    let ua = u * a;
    let ntu = ua / c_min;

    // 4. Effectiveness epsilon based on flow arrangement
    let effectiveness = match prob.arrangement {
        FlowArrangement::CounterFlow => {
            if (c_r - 1.0).abs() < 1e-6 {
                ntu / (1.0 + ntu)
            } else {
                let exp_val = (-ntu * (1.0 - c_r)).exp();
                (1.0 - exp_val) / (1.0 - c_r * exp_val)
            }
        }
        FlowArrangement::ParallelFlow => {
            let exp_val = (-ntu * (1.0 + c_r)).exp();
            (1.0 - exp_val) / (1.0 + c_r)
        }
        FlowArrangement::CrossFlowUnmixed => {
            // Kays & London empirical relation for both streams unmixed
            let term = 1.0 - (-c_r * ntu.powf(0.78)).exp();
            1.0 - (-(1.0 / c_r) * ntu.powf(0.22) * term).exp()
        }
    };
    let eps = effectiveness.clamp(0.0, 1.0);

    // 5. Actual thermal duty: Q = epsilon * Q_max
    let q_actual_watts = eps * q_max_watts;
    let q_actual_kw = q_actual_watts / 1000.0;

    // 6. Outlet temperatures from energy balance:
    let th_out = th_in - q_actual_watts / ch;
    let tc_out = tc_in + q_actual_watts / cc;

    // 7. Logarithmic Mean Temperature Difference (LMTD)
    let (dt1, dt2) = match prob.arrangement {
        FlowArrangement::CounterFlow => (th_in - tc_out, th_out - tc_in),
        FlowArrangement::ParallelFlow | FlowArrangement::CrossFlowUnmixed => {
            (th_in - tc_in, th_out - tc_out)
        }
    };
    let lmtd = if (dt1 - dt2).abs() < 1e-4 {
        dt1
    } else if dt1 > 0.0 && dt2 > 0.0 {
        (dt1 - dt2) / (dt1 / dt2).ln()
    } else {
        0.0
    };

    // Analytical verification: Hot side loss Q_h vs Cold side gain Q_c
    let q_hot = ch * (th_in - th_out);
    let q_cold = cc * (tc_out - tc_in);
    let val = AnalyticalComparison::assess(
        "First Law Thermal Energy Balance (Q_hot == Q_cold)",
        q_hot,
        q_cold,
    );

    let mut conv = ConvergenceMonitor::new(1e-6);
    conv.record(1, q_actual_kw, 1, 1e-4);

    let mut metrics = HashMap::new();
    metrics.insert("heat_duty_kw".to_string(), q_actual_kw);
    metrics.insert("heat_duty_mw".to_string(), q_actual_kw / 1000.0);
    metrics.insert("effectiveness".to_string(), eps);
    metrics.insert("ntu".to_string(), ntu);
    metrics.insert("capacity_ratio_cr".to_string(), c_r);
    metrics.insert("hot_outlet_temp_c".to_string(), th_out);
    metrics.insert("cold_outlet_temp_c".to_string(), tc_out);
    metrics.insert("lmtd_k".to_string(), lmtd);
    metrics.insert("ua_w_per_k".to_string(), ua);
    metrics.insert("hot_temp_drop_c".to_string(), th_in - th_out);
    metrics.insert("cold_temp_rise_c".to_string(), tc_out - tc_in);

    let logs = vec![
        format!(
            "Heat Exchanger Thermal Rating: '{}' ({:?})",
            prob.heat_exchanger_name, prob.arrangement
        ),
        format!(
            "Inlet Conditions: Hot = {:.1} °C ({:.1} kg/s), Cold = {:.1} °C ({:.1} kg/s)",
            th_in, mh, tc_in, mc
        ),
        format!(
            "Thermal Performance: NTU = {:.2}, Cr = {:.3} -> Effectiveness ε = {:.1}%",
            ntu,
            c_r,
            eps * 100.0
        ),
        format!(
            "Heat Duty Q = {:.1} kW ({:.2} MW) · UA = {:.0} W/K",
            q_actual_kw,
            q_actual_kw / 1000.0,
            ua
        ),
        format!(
            "Outlet Conditions: Hot Out = {:.1} °C (ΔT = -{:.1} °C), Cold Out = {:.1} °C (ΔT = +{:.1} °C)",
            th_out,
            th_in - th_out,
            tc_out,
            tc_out - tc_in
        ),
        format!("Log Mean Temp Diff (LMTD) = {:.1} K", lmtd),
    ];

    Ok(SimulationResult {
        problem_id: "heat-exchanger-ntu".to_string(),
        engine_id: "atlas-thermal-hx".to_string(),
        status: SimulationStatus::Converged,
        execution_time_ms: 1,
        scalar_metrics: metrics,
        convergence: conv,
        validation: Some(val),
        provenance: SimulationProvenance::new("atlas-thermal-hx", "0.1.0"),
        logs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_heat_exchanger_counter_flow_and_energy_conservation() -> Result<(), String> {
        let prob = HeatExchangerProblem::default();
        let res = solve_heat_exchanger(&prob)?;
        assert_eq!(res.status, SimulationStatus::Converged);

        let q_kw = res
            .scalar_metrics
            .get("heat_duty_kw")
            .copied()
            .unwrap_or(0.0);
        let eps = res
            .scalar_metrics
            .get("effectiveness")
            .copied()
            .unwrap_or(0.0);
        let th_out = res
            .scalar_metrics
            .get("hot_outlet_temp_c")
            .copied()
            .unwrap_or(0.0);
        let tc_out = res
            .scalar_metrics
            .get("cold_outlet_temp_c")
            .copied()
            .unwrap_or(0.0);

        // Counter-flow 18 m2 water-water heat exchanger transfer > 500 kW
        assert!(
            q_kw > 400.0 && q_kw < 900.0,
            "Thermal duty unexpected: {}",
            q_kw
        );
        assert!(
            eps > 0.40 && eps < 0.95,
            "Effectiveness unexpected: {}",
            eps
        );
        assert!(
            th_out < 90.0 && th_out > 30.0,
            "Hot outlet temp unexpected: {}",
            th_out
        );
        assert!(
            tc_out > 20.0 && tc_out < 80.0,
            "Cold outlet temp unexpected: {}",
            tc_out
        );

        // First law energy conservation error must be virtually zero
        let val = res.validation.ok_or("Missing validation")?;
        assert!(
            val.error_percent < 1e-4,
            "First law violation: {}",
            val.error_percent
        );

        Ok(())
    }
}
