//! Directed Acyclic Graph (DAG) task scheduling and dependency management.

use crate::error::OrchestratorError;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Pending,
    Running,
    Completed,
    Failed,
    WaitingApproval,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineeringTask {
    pub id: String,
    pub name: String,
    pub assigned_agent: String,
    pub command_id: String,
    pub input_args: serde_json::Value,
    pub dependencies: Vec<String>,
    pub status: TaskStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub execution_duration_ms: u64,
}

impl EngineeringTask {
    #[must_use]
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        assigned_agent: impl Into<String>,
        command_id: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            assigned_agent: assigned_agent.into(),
            command_id: command_id.into(),
            input_args: serde_json::json!({}),
            dependencies: Vec::new(),
            status: TaskStatus::Pending,
            result: None,
            error: None,
            execution_duration_ms: 0,
        }
    }

    #[must_use]
    pub fn with_args(mut self, args: serde_json::Value) -> Self {
        self.input_args = args;
        self
    }

    #[must_use]
    pub fn with_dependency(mut self, dep_task_id: impl Into<String>) -> Self {
        self.dependencies.push(dep_task_id.into());
        self
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TaskGraph {
    pub tasks: BTreeMap<String, EngineeringTask>,
}

impl TaskGraph {
    #[must_use]
    pub fn new() -> Self {
        Self {
            tasks: BTreeMap::new(),
        }
    }

    pub fn add_task(&mut self, task: EngineeringTask) -> Result<(), OrchestratorError> {
        let id = task.id.clone();
        if self.tasks.contains_key(&id) {
            return Err(OrchestratorError::Internal(format!(
                "Task with id '{id}' already exists in graph"
            )));
        }
        self.tasks.insert(id, task);
        self.validate_no_cycles()?;
        Ok(())
    }

    #[must_use]
    pub fn get_task(&self, id: &str) -> Option<&EngineeringTask> {
        self.tasks.get(id)
    }

    #[must_use]
    pub fn get_task_mut(&mut self, id: &str) -> Option<&mut EngineeringTask> {
        self.tasks.get_mut(id)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.tasks.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tasks.is_empty()
    }

    /// Returns list of task IDs that are in `Pending` state and whose dependencies are all `Completed`.
    #[must_use]
    pub fn get_ready_tasks(&self) -> Vec<String> {
        self.tasks
            .values()
            .filter(|t| {
                if t.status != TaskStatus::Pending {
                    return false;
                }
                // All dependencies must be completed
                t.dependencies.iter().all(|dep_id| {
                    self.tasks
                        .get(dep_id)
                        .is_some_and(|dep| dep.status == TaskStatus::Completed)
                })
            })
            .map(|t| t.id.clone())
            .collect()
    }

    pub fn mark_completed(
        &mut self,
        task_id: &str,
        result: serde_json::Value,
        duration_ms: u64,
    ) -> Result<(), OrchestratorError> {
        let task = self
            .tasks
            .get_mut(task_id)
            .ok_or_else(|| OrchestratorError::TaskNotFound(task_id.to_string()))?;
        task.status = TaskStatus::Completed;
        task.result = Some(result);
        task.execution_duration_ms = duration_ms;
        Ok(())
    }

    pub fn mark_failed(&mut self, task_id: &str, err: String) -> Result<(), OrchestratorError> {
        let task = self
            .tasks
            .get_mut(task_id)
            .ok_or_else(|| OrchestratorError::TaskNotFound(task_id.to_string()))?;
        task.status = TaskStatus::Failed;
        task.error = Some(err);
        Ok(())
    }

    #[must_use]
    pub fn is_all_completed(&self) -> bool {
        !self.tasks.is_empty()
            && self
                .tasks
                .values()
                .all(|t| t.status == TaskStatus::Completed)
    }

    #[must_use]
    pub fn has_failures(&self) -> bool {
        self.tasks.values().any(|t| t.status == TaskStatus::Failed)
    }

    /// Computes topological ordering using Kahn's algorithm; fails if cycle is found.
    pub fn topological_order(&self) -> Result<Vec<String>, OrchestratorError> {
        let mut in_degrees: BTreeMap<String, usize> = BTreeMap::new();
        let mut adj: BTreeMap<String, Vec<String>> = BTreeMap::new();

        for (id, task) in &self.tasks {
            in_degrees.entry(id.clone()).or_insert(0);
            for dep in &task.dependencies {
                adj.entry(dep.clone()).or_default().push(id.clone());
                *in_degrees.entry(id.clone()).or_default() += 1;
            }
        }

        let mut queue: VecDeque<String> = in_degrees
            .iter()
            .filter(|(_, deg)| **deg == 0)
            .map(|(id, _)| id.clone())
            .collect();

        let mut order = Vec::with_capacity(self.tasks.len());

        while let Some(u) = queue.pop_front() {
            order.push(u.clone());
            if let Some(neighbors) = adj.get(&u) {
                for v in neighbors {
                    if let Some(deg) = in_degrees.get_mut(v) {
                        *deg -= 1;
                        if *deg == 0 {
                            queue.push_back(v.clone());
                        }
                    }
                }
            }
        }

        if order.len() == self.tasks.len() {
            Ok(order)
        } else {
            Err(OrchestratorError::DependencyCycleDetected)
        }
    }

    pub fn validate_no_cycles(&self) -> Result<(), OrchestratorError> {
        // Check for dangling dependency references
        for task in self.tasks.values() {
            for dep in &task.dependencies {
                if !self.tasks.contains_key(dep) {
                    return Err(OrchestratorError::TaskNotFound(dep.clone()));
                }
            }
        }
        let _ = self.topological_order()?;
        Ok(())
    }
}
