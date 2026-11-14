//! Engine execution configuration and resource ceilings.

use serde::{Deserialize, Serialize};

/// Per-engine execution controls and resource ceilings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineConfig {
    pub max_threads: usize,
    pub max_memory_mb: u64,
    pub timeout_seconds: u64,
    pub scratch_dir: Option<String>,
    pub allow_gpu: bool,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            max_threads: 4,
            max_memory_mb: 8192,
            timeout_seconds: 120,
            scratch_dir: None,
            allow_gpu: false,
        }
    }
}
