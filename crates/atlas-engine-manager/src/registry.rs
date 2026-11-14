//! High-level engine manager delegating to the unified registry.

use crate::configuration::EngineConfig;
use atlas_core::error::CoreError;
use cad_engineering::engine::{
    EngineHealthReport, EngineId, EngineInfo, EngineJobRequest, EngineJobResult, EngineRegistry,
};
use std::collections::HashMap;

/// Global orchestrator managing all 24 engineering engines.
#[derive(Debug, Clone)]
pub struct EngineManager {
    registry: EngineRegistry,
    configs: HashMap<EngineId, EngineConfig>,
}

impl Default for EngineManager {
    fn default() -> Self {
        Self::new()
    }
}

impl EngineManager {
    pub fn new() -> Self {
        Self {
            registry: EngineRegistry::new(),
            configs: HashMap::new(),
        }
    }

    /// List all registered engines.
    pub fn list_engines(&self) -> Vec<EngineInfo> {
        self.registry.list()
    }

    /// Retrieve an engine by ID.
    pub fn get_engine(&self, id: &EngineId) -> Option<&EngineInfo> {
        self.registry.get(id)
    }

    /// Test diagnostic health of an engine.
    pub fn test_health(&self, id: &EngineId) -> EngineHealthReport {
        self.registry.test_health(id)
    }

    /// Execute a solver job.
    pub fn execute_job(&self, req: &EngineJobRequest) -> Result<EngineJobResult, CoreError> {
        let result = self.registry.execute_job(req);
        match (&result.success, &result.error_message) {
            (false, Some(err)) => Err(CoreError::OperationFailed(err.clone())),
            _ => Ok(result),
        }
    }

    /// Configure resource ceilings for an engine.
    pub fn configure_engine(&mut self, id: EngineId, config: EngineConfig) {
        self.configs.insert(id, config);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_engine_manager_orchestration() {
        let mgr = EngineManager::new();
        let engines = mgr.list_engines();
        assert_eq!(engines.len(), 24);

        let ccx_id = EngineId::new("calculix");
        let health = mgr.test_health(&ccx_id);
        assert!(health.healthy);

        let req = EngineJobRequest {
            job_id: "test-job-1".to_string(),
            engine_id: ccx_id,
            job_type: Some("static_fea".to_string()),
            model_revision: 1,
            parameters: serde_json::json!({}),
            timeout_seconds: 30,
        };
        let res = mgr.execute_job(&req);
        assert!(res.is_ok());
    }
}
