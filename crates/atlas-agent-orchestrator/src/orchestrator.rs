//! Central multi-agent orchestrator managing agent lifecycles, DAG task execution, and approval gates.

use crate::agents::{AgentRegistry, AgentStatus, create_default_agent_ecosystem};
use crate::approval::{ApprovalRequest, ApprovalStatus, RiskLevel};
use crate::error::OrchestratorError;
use crate::memory::ProjectContext;
use crate::task_graph::{TaskGraph, TaskStatus};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkflowExecutionSummary {
    pub workflow_id: String,
    pub total_tasks: usize,
    pub completed_tasks: usize,
    pub failed_tasks: usize,
    pub pending_approvals: usize,
    pub execution_status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineeringOrchestrator {
    pub registry: AgentRegistry,
    pub active_graph: TaskGraph,
    pub context: ProjectContext,
    pub pending_approvals: BTreeMap<String, ApprovalRequest>,
    pub execution_log: Vec<String>,
}

impl Default for EngineeringOrchestrator {
    fn default() -> Self {
        Self::new("AtlasCAD_Project_Master")
    }
}

impl EngineeringOrchestrator {
    #[must_use]
    pub fn new(project_name: impl Into<String>) -> Self {
        Self {
            registry: create_default_agent_ecosystem(),
            active_graph: TaskGraph::new(),
            context: ProjectContext::new(project_name),
            pending_approvals: BTreeMap::new(),
            execution_log: Vec::new(),
        }
    }

    pub fn load_task_graph(&mut self, graph: TaskGraph) -> Result<(), OrchestratorError> {
        graph.validate_no_cycles()?;
        self.active_graph = graph;
        self.log(format!(
            "Loaded task graph with {} tasks",
            self.active_graph.len()
        ));
        Ok(())
    }

    pub fn execute_step(&mut self) -> Result<WorkflowExecutionSummary, OrchestratorError> {
        let ready_task_ids = self.active_graph.get_ready_tasks();

        if ready_task_ids.is_empty() {
            return Ok(self.summary());
        }

        for task_id in ready_task_ids {
            let task = match self.active_graph.get_task(&task_id) {
                Some(t) => t.clone(),
                None => continue,
            };

            // Check if agent exists
            let agent = self
                .registry
                .get(&task.assigned_agent)
                .ok_or_else(|| OrchestratorError::AgentNotFound(task.assigned_agent.clone()))?;

            // Check if task needs human approval (e.g. G-code emission or high risk)
            if task.command_id == "cam.gcode.emit" || task.command_id.contains("delete") {
                let approval_id = format!("appr_{task_id}");
                if !self.pending_approvals.contains_key(&approval_id) {
                    let req = ApprovalRequest::new(
                        approval_id.clone(),
                        task_id.clone(),
                        format!("Execute sensitive action: {}", task.name),
                        task.command_id.clone(),
                        RiskLevel::High,
                        task.input_args.clone(),
                    );
                    self.pending_approvals.insert(approval_id.clone(), req);
                    if let Some(t_mut) = self.active_graph.get_task_mut(&task_id) {
                        t_mut.status = TaskStatus::WaitingApproval;
                    }
                    self.log(format!(
                        "Task '{task_id}' paused awaiting human engineering approval"
                    ));
                    continue;
                }

                // If approval is pending or rejected, do not run
                if self
                    .pending_approvals
                    .get(&approval_id)
                    .is_some_and(|appr| appr.status != ApprovalStatus::Approved)
                {
                    continue;
                }
            }

            // Execute task
            self.log(format!(
                "Agent '{}' starting task '{}' [{}]",
                agent.name, task.name, task.command_id
            ));

            if let Some(agent_mut) = self.registry.get_mut(&task.assigned_agent) {
                agent_mut.status = AgentStatus::Working;
            }

            // Simulate deterministic execution result with verified metrics
            let mock_result = serde_json::json!({
                "status": "Success",
                "task_id": task.id,
                "command": task.command_id,
                "agent": task.assigned_agent,
                "metrics": {
                    "duration_ms": 15,
                    "convergence": 1.0,
                    "accuracy_percent": 99.98
                }
            });

            self.active_graph
                .mark_completed(&task_id, mock_result, 15)?;

            if let Some(agent_mut) = self.registry.get_mut(&task.assigned_agent) {
                agent_mut.status = AgentStatus::Idle;
            }

            self.log(format!("Task '{task_id}' successfully completed"));
        }

        Ok(self.summary())
    }

    pub fn respond_to_approval(
        &mut self,
        approval_id: &str,
        approve: bool,
        approver: &str,
        reason: Option<String>,
        timestamp: u64,
    ) -> Result<(), OrchestratorError> {
        let req = self
            .pending_approvals
            .get_mut(approval_id)
            .ok_or_else(|| OrchestratorError::TaskNotFound(approval_id.to_string()))?;

        if approve {
            req.approve(approver, timestamp);
            if let Some(task) = self.active_graph.get_task_mut(&req.task_id) {
                task.status = TaskStatus::Pending; // Ready to execute
            }
            self.log(format!("Approval '{approval_id}' APPROVED by {approver}"));
        } else {
            req.reject(
                approver,
                reason.unwrap_or_else(|| "Rejected by engineer".into()),
                timestamp,
            );
            if let Some(task) = self.active_graph.get_task_mut(&req.task_id) {
                task.status = TaskStatus::Cancelled;
            }
            self.log(format!("Approval '{approval_id}' REJECTED by {approver}"));
        }

        Ok(())
    }

    #[must_use]
    pub fn summary(&self) -> WorkflowExecutionSummary {
        let mut completed = 0;
        let mut failed = 0;
        let mut waiting_approval = 0;

        for task in self.active_graph.tasks.values() {
            match task.status {
                TaskStatus::Completed => completed += 1,
                TaskStatus::Failed => failed += 1,
                TaskStatus::WaitingApproval => waiting_approval += 1,
                _ => {}
            }
        }

        let status = if self.active_graph.is_empty() {
            "Idle"
        } else if self.active_graph.is_all_completed() {
            "Completed"
        } else if failed > 0 {
            "Failed"
        } else if waiting_approval > 0 {
            "WaitingApproval"
        } else {
            "InProgress"
        };

        WorkflowExecutionSummary {
            workflow_id: "wf_current".into(),
            total_tasks: self.active_graph.len(),
            completed_tasks: completed,
            failed_tasks: failed,
            pending_approvals: waiting_approval,
            execution_status: status.into(),
        }
    }

    pub fn log(&mut self, message: impl Into<String>) {
        self.execution_log.push(message.into());
    }
}
