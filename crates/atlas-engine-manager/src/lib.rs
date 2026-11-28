//! # atlas-engine-manager
//!
//! Unified multi-engine orchestration, discovery, health testing, resource ceilings,
//! and job dispatching across 24 specialist open-source engineering engines.
//!
//! Five engines have real, process-isolated adapters that run an installed solver and parse its
//! own output: CalculiX (and the document solve built on the Gmsh mesh), ngspice, Gmsh,
//! pandapower and Cantera. The rest are registered and refuse execution, which is what the HTTP
//! layer reports.

pub mod adapters;
pub mod calculix;
pub mod cantera;
pub mod capability;
pub mod configuration;
pub mod discovery;
pub mod gmsh;
pub mod health;
pub mod installation;
pub mod ngspice;
pub mod pandapower;
mod process;
pub mod python;
pub mod registry;
pub mod versioning;

pub mod prelude {
    pub use crate::adapters::EngineAdapter;
    pub use crate::capability::SolverCapability;
    pub use crate::configuration::EngineConfig;
    pub use crate::discovery::{EngineDiscovery, EngineDiscoveryReport};
    pub use crate::installation::InstallationGuide;
    pub use crate::registry::EngineManager;
    pub use cad_engineering::engine::{
        EngineCapability, EngineDiscipline, EngineHealthReport, EngineId, EngineInfo,
        EngineJobRequest, EngineJobResult, ExecutionModel, LicenseTier,
    };
}
