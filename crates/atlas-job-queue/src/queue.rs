//! Thread-safe priority simulation job queue.

use crate::priority::JobPriority;
use crate::record::SimulationJobRecord;
use crate::status::JobStatus;
use std::collections::HashMap;

/// In-memory asynchronous job dispatcher and scheduler.
#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct JobQueueManager {
    jobs: HashMap<String, SimulationJobRecord>,
    ordered_ids: Vec<String>,
}

impl JobQueueManager {
    pub fn new() -> Self {
        Self {
            jobs: HashMap::new(),
            ordered_ids: Vec::new(),
        }
    }

    /// Submit a new simulation job to the queue.
    pub fn submit(&mut self, record: SimulationJobRecord) -> String {
        let id = record.job_id.clone();
        self.jobs.insert(id.clone(), record);
        self.ordered_ids.push(id.clone());
        self.sort_by_priority();
        id
    }

    /// Retrieve a job by ID.
    pub fn get(&self, job_id: &str) -> Option<&SimulationJobRecord> {
        self.jobs.get(job_id)
    }

    /// Retrieve a mutable reference to a job.
    pub fn get_mut(&mut self, job_id: &str) -> Option<&mut SimulationJobRecord> {
        self.jobs.get_mut(job_id)
    }

    /// Mark a job as running.
    pub fn mark_running(&mut self, job_id: &str, started_at_unix: u64) -> bool {
        if let Some(job) = self.jobs.get_mut(job_id) {
            job.status = JobStatus::Running;
            job.started_at_unix = Some(started_at_unix);
            job.progress_percent = 10.0;
            return true;
        }
        false
    }

    /// Mark a job as successfully completed with results.
    pub fn mark_completed(
        &mut self,
        job_id: &str,
        completed_at_unix: u64,
        elapsed_ms: u64,
        results: serde_json::Value,
        provenance_hash: impl Into<String>,
    ) -> bool {
        if let Some(job) = self.jobs.get_mut(job_id) {
            job.status = JobStatus::Completed;
            job.completed_at_unix = Some(completed_at_unix);
            job.elapsed_ms = elapsed_ms;
            job.progress_percent = 100.0;
            job.result_summary = Some(results);
            job.provenance_hash = provenance_hash.into();
            return true;
        }
        false
    }

    /// Cancel a queued or running job.
    pub fn cancel(&mut self, job_id: &str) -> bool {
        match self.jobs.get_mut(job_id) {
            Some(job) if !job.status.is_terminal() => {
                job.status = JobStatus::Cancelled;
                true
            }
            _ => false,
        }
    }

    /// List all jobs, highest priority first.
    pub fn list(&self) -> Vec<&SimulationJobRecord> {
        let mut list: Vec<&SimulationJobRecord> = self.jobs.values().collect();
        list.sort_by_key(|b| std::cmp::Reverse(b.priority));
        list
    }

    /// Active queued and running count.
    pub fn active_count(&self) -> usize {
        self.jobs
            .values()
            .filter(|j| matches!(j.status, JobStatus::Queued | JobStatus::Running))
            .count()
    }

    fn sort_by_priority(&mut self) {
        let jobs = &self.jobs;
        self.ordered_ids.sort_by(|a, b| {
            let p_a = jobs
                .get(a)
                .map(|j| j.priority)
                .unwrap_or(JobPriority::BatchLow);
            let p_b = jobs
                .get(b)
                .map(|j| j.priority)
                .unwrap_or(JobPriority::BatchLow);
            p_b.cmp(&p_a)
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simulation_queue_workflow() {
        let mut queue = JobQueueManager::new();

        let j1 = SimulationJobRecord::new(
            "job-01",
            "calculix",
            "static_fea",
            JobPriority::InteractiveNormal,
            "engineer@atlascad.com",
            1700000000,
            serde_json::json!({}),
        );

        let j2 = SimulationJobRecord::new(
            "job-02",
            "openfoam",
            "cfd_incompressible",
            JobPriority::CriticalHpc,
            "aerodynamicist@atlascad.com",
            1700000001,
            serde_json::json!({}),
        );

        queue.submit(j1);
        queue.submit(j2);

        assert_eq!(queue.active_count(), 2);
        let list = queue.list();
        assert_eq!(list[0].job_id, "job-02"); // High priority first

        assert!(queue.mark_running("job-02", 1700000002));
        assert!(queue.mark_completed(
            "job-02",
            1700000015,
            13000,
            serde_json::json!({"drag_cd": 0.031}),
            "hash_123"
        ));

        assert_eq!(queue.active_count(), 1);
        let job = queue.get("job-02");
        assert!(matches!(job, Some(j) if j.status == JobStatus::Completed));
    }
}
