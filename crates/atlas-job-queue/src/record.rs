//! Comprehensive execution record for background simulation jobs.

use crate::priority::JobPriority;
use crate::status::JobStatus;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationJobRecord {
    pub job_id: String,
    pub engine_id: String,
    pub job_type: String,
    pub priority: JobPriority,
    pub status: JobStatus,
    pub submitted_by: String,
    pub submitted_at_unix: u64,
    pub started_at_unix: Option<u64>,
    pub completed_at_unix: Option<u64>,
    pub elapsed_ms: u64,
    pub cpu_cores_allocated: u32,
    pub memory_mb_allocated: u64,
    pub progress_percent: f32,
    pub parameters: serde_json::Value,
    pub result_summary: Option<serde_json::Value>,
    pub error_message: Option<String>,
    pub provenance_hash: String,
}

impl SimulationJobRecord {
    pub fn new(
        job_id: impl Into<String>,
        engine_id: impl Into<String>,
        job_type: impl Into<String>,
        priority: JobPriority,
        submitted_by: impl Into<String>,
        submitted_at_unix: u64,
        parameters: serde_json::Value,
    ) -> Self {
        Self {
            job_id: job_id.into(),
            engine_id: engine_id.into(),
            job_type: job_type.into(),
            priority,
            status: JobStatus::Queued,
            submitted_by: submitted_by.into(),
            submitted_at_unix,
            started_at_unix: None,
            completed_at_unix: None,
            elapsed_ms: 0,
            cpu_cores_allocated: 4,
            memory_mb_allocated: 4096,
            progress_percent: 0.0,
            parameters,
            result_summary: None,
            error_message: None,
            provenance_hash: "0000000000000000".to_string(),
        }
    }
}
