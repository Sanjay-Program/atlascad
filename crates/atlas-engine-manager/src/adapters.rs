//! Execution adapters bridging engines to AtlasCAD.

use atlas_core::error::CoreError;
use cad_engineering::engine::{EngineHealthReport, EngineInfo, EngineJobRequest, EngineJobResult};
pub use cad_engineering::engine::{ExecutionModel, LicenseTier};

/// Standard trait implemented by engine adapters.
pub trait EngineAdapter: Send + Sync {
    /// Information and capabilities of the engine.
    fn engine_info(&self) -> &EngineInfo;

    /// Test execution health.
    fn test_health(&self) -> EngineHealthReport;

    /// Execute a computation job.
    fn execute_job(&self, req: &EngineJobRequest) -> Result<EngineJobResult, CoreError>;
}
