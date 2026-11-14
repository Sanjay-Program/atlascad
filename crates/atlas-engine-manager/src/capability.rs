//! Specialist engine analysis capabilities.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum SolverCapability {
    LinearStaticFea,
    NonlinearFea,
    ModalVibration,
    SteadyThermal,
    TransientThermal,
    IncompressibleCfd,
    CompressibleAero,
    TurbulenceModeling,
    MultiphaseFlow,
    AcPowerFlow,
    DcPowerFlow,
    ShortCircuitIec60909,
    SpiceAnalogTransient,
    SpiceAcFrequencySweep,
    ChemicalEquilibrium,
    CombustionKinetics,
    MultiJointDynamics,
    InverseKinematics,
    PressurizedWaterNetwork,
    UrbanDrainageRunoff,
}
