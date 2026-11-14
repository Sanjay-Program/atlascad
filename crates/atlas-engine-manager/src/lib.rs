//! # atlas-engine-manager
//!
//! Unified multi-engine orchestration, discovery, health testing, resource ceilings,
//! and job dispatching across 24 specialist open-source engineering engines.

pub mod adapters;
pub mod capability;
pub mod configuration;
pub mod discovery;
pub mod health;
pub mod installation;
pub mod registry;
pub mod versioning;

pub mod prelude {
    pub use crate::adapters::EngineAdapter;
    pub use crate::capability::SolverCapability;
    pub use crate::configuration::EngineConfig;
    pub use crate::discovery::EngineDiscovery;
    pub use crate::installation::InstallationGuide;
    pub use crate::registry::EngineManager;
    pub use cad_engineering::engine::{
        EngineCapability, EngineDiscipline, EngineHealthReport, EngineId, EngineInfo,
        EngineJobRequest, EngineJobResult, ExecutionModel, LicenseTier,
    };
}
