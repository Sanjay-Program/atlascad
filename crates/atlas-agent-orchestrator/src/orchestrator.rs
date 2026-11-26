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
    /// Document revision the current run is bound to. Every task result records it so a result
    /// can never be mistaken for one computed against a different model state.
    #[serde(default)]
    pub document_revision: Option<u64>,
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
            document_revision: None,
        }
    }

    /// Bind subsequent runs to a document revision. Results record the revision they were
    /// computed against alongside their input source.
    pub fn bind_document_revision(&mut self, revision: u64) {
        self.document_revision = Some(revision);
    }

    fn now_epoch_secs() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or(0)
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

            // Execute the registered analytical solver. The elapsed time is measured, not invented,
            // and nothing is marked complete when the command has no implementation in this build.
            let started = std::time::Instant::now();
            let mut execution_result = self.execute_task_payload(&task);
            let duration_ms = started.elapsed().as_millis() as u64;

            if let Some(object) = execution_result.as_object_mut() {
                object.insert(
                    "document_revision".into(),
                    serde_json::json!(self.document_revision),
                );
                object.insert(
                    "input_source".into(),
                    serde_json::json!("fixed_case_parameters"),
                );
            }

            let unavailable = execution_result
                .get("status")
                .and_then(|status| status.as_str())
                == Some("Unavailable");

            if unavailable {
                let reason = execution_result
                    .get("error")
                    .and_then(|error| error.as_str())
                    .unwrap_or("no implementation in this build")
                    .to_string();
                self.active_graph.mark_failed(&task_id, reason.clone())?;
                self.log(format!("Task '{task_id}' unavailable: {reason}"));
            } else {
                self.active_graph
                    .mark_completed(&task_id, execution_result, duration_ms)?;
                self.log(format!("Task '{task_id}' completed in {duration_ms} ms"));
            }

            if let Some(agent_mut) = self.registry.get_mut(&task.assigned_agent) {
                agent_mut.status = AgentStatus::Idle;
            }
        }

        Ok(self.summary())
    }

    /// A refused task: no fabricated numbers, and a reason the caller can show.
    fn unavailable(task: &crate::task_graph::EngineeringTask, reason: &str) -> serde_json::Value {
        serde_json::json!({
            "status": "Unavailable",
            "task_id": task.id,
            "command": task.command_id,
            "agent": task.assigned_agent,
            "output": serde_json::Value::Null,
            "error": reason,
        })
    }

    /// Dispatches actual physical solvers, CAD generators, CAM toolpaths, or export bridges.
    #[must_use]
    pub fn execute_task_payload(
        &self,
        task: &crate::task_graph::EngineeringTask,
    ) -> serde_json::Value {
        match task.command_id.as_str() {
            "project.inspect" => {
                // Real document facts, bound by the caller before execution. Nothing here is
                // invented: when no facts were bound the map is empty and that is visible.
                serde_json::json!({
                    "status": "Success",
                    "task_id": task.id,
                    "command": task.command_id,
                    "agent": task.assigned_agent,
                    "inspection": {
                        "project_name": self.context.project_name,
                        "document_revision": self.document_revision,
                        "facts": self.context.facts,
                    }
                })
            }
            "sim.hypersonic.plasma" => {
                let res = atlas_simulation_core::hypersonic::plasma::evaluate_plasma_sheath(
                    11.0, 32000.0, 228.65, 0.0132, 0.05,
                );
                serde_json::json!({
                    "status": "Success",
                    "task_id": task.id,
                    "command": task.command_id,
                    "agent": task.assigned_agent,
                    "plasma": res,
                })
            }
            "sim.hypersonic.scramjet" => {
                let res = atlas_simulation_core::hypersonic::scramjet::evaluate_scramjet_cycle(
                    11.0, 3333.3, 228.65, 868.0, 0.0132, 1.45, 1.0,
                );
                serde_json::json!({
                    "status": "Success",
                    "task_id": task.id,
                    "command": task.command_id,
                    "agent": task.assigned_agent,
                    "scramjet": res,
                })
            }
            "sim.hypersonic.thermal" => {
                let res = atlas_simulation_core::hypersonic::thermal::evaluate_active_cooling(
                    1.45, 12.5, 4.93, 20.0, 4.5,
                );
                serde_json::json!({
                    "status": "Success",
                    "task_id": task.id,
                    "command": task.command_id,
                    "agent": task.assigned_agent,
                    "active_cooling": res,
                })
            }
            "sim.hypersonic.creep" => {
                let res = atlas_simulation_core::hypersonic::thermal::evaluate_high_temp_creep(
                    2545.0, 45.0, 1.6,
                );
                serde_json::json!({
                    "status": "Success",
                    "task_id": task.id,
                    "command": task.command_id,
                    "agent": task.assigned_agent,
                    "creep_fatigue": res,
                })
            }
            "sim.hypersonic.solve" => {
                let prob = atlas_simulation_core::hypersonic::HypersonicAirplaneProblem::default();
                let res = atlas_simulation_core::hypersonic::solve_hypersonic_airplane(&prob);
                serde_json::json!({
                    "status": "Success",
                    "task_id": task.id,
                    "command": task.command_id,
                    "agent": task.assigned_agent,
                    "simulation": res.ok(),
                })
            }
            "sim.hypersonic.bom" => {
                let res = atlas_simulation_core::hypersonic::bom::build_hypersonic_bom();
                serde_json::json!({
                    "status": "Success",
                    "task_id": task.id,
                    "command": task.command_id,
                    "agent": task.assigned_agent,
                    "bom": res,
                })
            }
            "sim.hypersonic.flutter" => {
                let res =
                    atlas_simulation_core::hypersonic::trajectory::evaluate_hypersonic_flutter(
                        73.5, 11.0, 0.80, 0.0035, 2272.0, 480.0, 0.15, 6080.0, 0.0132, 3333.3,
                    );
                serde_json::json!({
                    "status": "Success",
                    "task_id": task.id,
                    "command": task.command_id,
                    "agent": task.assigned_agent,
                    "flutter": res,
                })
            }
            "sim.hypersonic.6dof" => {
                let res = atlas_simulation_core::hypersonic::trajectory::evaluate_6dof_btt_maneuver(
                    3333.3, 32000.0, 48000.0, 30.0, 55.0, 770_000.0,
                );
                serde_json::json!({
                    "status": "Success",
                    "task_id": task.id,
                    "command": task.command_id,
                    "agent": task.assigned_agent,
                    "maneuver_6dof": res,
                })
            }
            // These commands have no implementation wired into the orchestrator. Returning a
            // plausible geometry, toolpath count, rendered-master state or certificate hash would
            // be a fabricated engineering claim, so the task is refused instead.
            "cad.solid.extrude"
            | "cad.solid.box"
            | "cam.toolpaths.generate"
            | "cam.gcode.emit"
            | "export.unity.engine"
            | "export.davinci.resolve"
            | "eco.submit" => Self::unavailable(
                task,
                "no CAD/CAM/export/certification adapter is wired into the agent orchestrator in this \
                 build; use the corresponding API endpoint, or implement the adapter",
            ),
            _ => Self::unavailable(
                task,
                "no implementation is registered for this command in this build; no result was \
                 produced",
            ),
        }
    }

    /// Executes all tasks in the active task graph topologically until completion or unapproved gates.
    pub fn execute_all(
        &mut self,
        auto_approve_sensitive: bool,
    ) -> Result<WorkflowExecutionSummary, OrchestratorError> {
        let mut loop_count = 0;
        let max_loops = 50;

        while !self.active_graph.is_all_completed() && loop_count < max_loops {
            loop_count += 1;
            let ready_tasks = self.active_graph.get_ready_tasks();

            if ready_tasks.is_empty() {
                // If there are pending approvals, check if we should auto-approve
                if auto_approve_sensitive && !self.pending_approvals.is_empty() {
                    let pending_ids: Vec<String> = self
                        .pending_approvals
                        .iter()
                        .filter(|(_, req)| req.status == ApprovalStatus::Pending)
                        .map(|(id, _)| id.clone())
                        .collect();

                    if pending_ids.is_empty() {
                        break;
                    }

                    for appr_id in pending_ids {
                        self.respond_to_approval(
                            &appr_id,
                            true,
                            "Chief_Autonomous_Engineer",
                            Some("Autonomous verified workflow approval gate passed".into()),
                            Self::now_epoch_secs(),
                        )?;
                    }
                    continue;
                }
                break;
            }

            self.execute_step()?;
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
