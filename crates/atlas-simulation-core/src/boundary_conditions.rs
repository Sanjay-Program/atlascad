//! Multidisciplinary boundary conditions.

use serde::{Deserialize, Serialize};

/// Discrete boundary conditions for physics simulations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BoundaryCondition {
    /// Fully constrained displacement (u_x = u_y = u_z = 0).
    FixedSupport { surface_id: String },

    /// Prescribed surface pressure in Pascals.
    PressureLoad {
        surface_id: String,
        pressure_pa: f64,
    },

    /// Concentrated force vector in Newtons.
    ForceLoad { node_id: String, force_n: [f64; 3] },

    /// Constant surface temperature in Kelvin.
    FixedTemperature {
        surface_id: String,
        temperature_k: f64,
    },

    /// Constant surface heat flux in Watts per square meter.
    HeatFlux { surface_id: String, flux_w_m2: f64 },

    /// Fluid inlet velocity vector in meters per second.
    VelocityInlet {
        surface_id: String,
        velocity_m_s: [f64; 3],
    },

    /// Fluid static pressure outlet in Pascals.
    PressureOutlet { surface_id: String, static_pa: f64 },

    /// Electrical ground node (0 V).
    GroundNode { node_id: String },

    /// Constant DC voltage source in Volts.
    VoltageSource { node_id: String, volts: f64 },

    /// Hydraulic pressurized reservoir head in meters.
    ReservoirHead { junction_id: String, head_m: f64 },
}
