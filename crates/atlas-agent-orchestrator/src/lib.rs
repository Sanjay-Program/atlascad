//! AtlasCAD Multi-Agent Engineering Orchestrator
//!
//! Provides a 44-specialist agent ecosystem (28 engineering + 16 creative, real-time, and media specialists), DAG task graph scheduling,
//! requirement-to-engineering execution, closed-loop iterative design optimization,
//! human safety approval gates, structured agent memory, and standard MCP JSON-RPC 2.0 endpoints.

#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

pub mod agents;
pub mod approval;
pub mod error;
pub mod mcp;
pub mod memory;
pub mod model;
pub mod orchestrator;
pub mod pipeline;
pub mod task_graph;
pub mod workflows;

pub use agents::{AgentManifest, AgentRegistry, AgentStatus, create_default_agent_ecosystem};
pub use approval::{ApprovalRequest, ApprovalStatus, RiskLevel};
pub use error::OrchestratorError;
pub use mcp::{McpRequest, McpResponse, McpServer, McpToolDefinition};
pub use memory::{
    ConstraintRecord, ConstraintStatus, DesignDecisionRecord, DesignRevisionRecord, ProjectContext,
};
pub use model::{DeterministicEngineeringPlanner, ModelConfig, ModelProvider};
pub use orchestrator::{EngineeringOrchestrator, WorkflowExecutionSummary};
pub use pipeline::{
    EngineeringPipeline, EngineeringSpecification, IterativeOptimizationResult, MissingConstraint,
    RequirementAnalysis,
};
pub use task_graph::{EngineeringTask, TaskGraph, TaskStatus};
pub use workflows::{WorkflowRegistry, WorkflowTemplateInfo};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_ecosystem_has_44_specialists() {
        let registry = create_default_agent_ecosystem();
        assert_eq!(
            registry.len(),
            44,
            "Exactly 44 specialized engineering, creative, real-time, and media agents must be registered"
        );

        // Verify engineering agents exist
        assert!(registry.get("chief_engineer").is_some());
        assert!(registry.get("cad_design").is_some());
        assert!(registry.get("structural_sim").is_some());
        assert!(registry.get("cfd_specialist").is_some());
        assert!(registry.get("multiphysics").is_some());
        assert!(registry.get("cnc_machinist").is_some());
        assert!(registry.get("additive_specialist").is_some());
        assert!(registry.get("verification_engineer").is_some());
        assert!(registry.get("quality_assurance").is_some());

        // Verify creative, real-time, and media agents exist
        assert!(registry.get("creative_3d_modeling").is_some());
        assert!(registry.get("creative_sculpting").is_some());
        assert!(registry.get("creative_material").is_some());
        assert!(registry.get("creative_texture").is_some());
        assert!(registry.get("creative_lighting").is_some());
        assert!(registry.get("creative_animation").is_some());
        assert!(registry.get("creative_rendering").is_some());
        assert!(registry.get("creative_vfx").is_some());
        assert!(registry.get("realtime_scene").is_some());
        assert!(registry.get("realtime_game_dev").is_some());
        assert!(registry.get("digital_twin").is_some());
        assert!(registry.get("media_video_editing").is_some());
        assert!(registry.get("media_color_grading").is_some());
        assert!(registry.get("media_audio").is_some());
        assert!(registry.get("media_production").is_some());
        assert!(registry.get("creative_qa").is_some());
    }

    #[test]
    fn test_task_graph_topological_sort_and_cycle_detection() -> Result<(), OrchestratorError> {
        let mut graph = TaskGraph::new();

        let t1 = EngineeringTask::new("t1", "Sketch", "cad_design", "cad.sketch.create");
        let t2 = EngineeringTask::new("t2", "Extrude", "cad_design", "cad.solid.extrude")
            .with_dependency("t1");
        let t3 = EngineeringTask::new("t3", "Simulate", "structural_sim", "sim.fea.solve")
            .with_dependency("t2");

        graph.add_task(t1)?;
        graph.add_task(t2)?;
        graph.add_task(t3)?;

        let order = graph.topological_order()?;
        assert_eq!(order, vec!["t1", "t2", "t3"]);

        // Ready tasks should only be t1 at start
        let ready = graph.get_ready_tasks();
        assert_eq!(ready, vec!["t1"]);

        Ok(())
    }

    #[test]
    fn test_requirement_analysis_detects_missing_constraints() {
        // Incomplete prompt: no material, no manufacturing, no load
        let prompt = "Build an aircraft bracket";
        let analysis = EngineeringPipeline::analyze_requirement(prompt);

        assert!(
            !analysis.is_ready_for_execution,
            "Should identify missing constraints instead of hallucinating"
        );
        assert!(
            analysis
                .missing_constraints
                .iter()
                .any(|c| c.key == "material")
        );
        assert!(
            analysis
                .missing_constraints
                .iter()
                .any(|c| c.key == "manufacturing_process")
        );
        assert!(
            analysis
                .missing_constraints
                .iter()
                .any(|c| c.key == "design_load")
        );

        // Complete prompt
        let complete_prompt =
            "Design a titanium wing bracket for 45 kN load with 5-axis CNC machining";
        let complete_analysis = EngineeringPipeline::analyze_requirement(complete_prompt);
        assert_eq!(complete_analysis.primary_discipline, "aerospace");
        assert!(complete_analysis.is_ready_for_execution);
    }

    #[test]
    fn test_workflow_template_instantiation() -> Result<(), OrchestratorError> {
        let templates = WorkflowRegistry::list_templates();
        assert_eq!(
            templates.len(),
            16,
            "Must offer 16 standard engineering, creative, real-time, and hypersonic workflow templates"
        );

        let graph = WorkflowRegistry::instantiate_template("cad_to_fea")?;
        assert_eq!(graph.len(), 4);
        assert!(graph.validate_no_cycles().is_ok());

        Ok(())
    }

    #[test]
    fn test_mcp_server_protocol() -> Result<(), OrchestratorError> {
        // Test initialize
        let init_req = McpRequest {
            jsonrpc: "2.0".into(),
            id: serde_json::json!(1),
            method: "initialize".into(),
            params: serde_json::json!({}),
        };
        let init_resp = McpServer::handle_rpc(&init_req)?;
        assert_eq!(init_resp.jsonrpc, "2.0");
        assert!(init_resp.result.is_some());

        // Test tools/list
        let tools_req = McpRequest {
            jsonrpc: "2.0".into(),
            id: serde_json::json!(2),
            method: "tools/list".into(),
            params: serde_json::json!({}),
        };
        let tools_resp = McpServer::handle_rpc(&tools_req)?;
        let tools = tools_resp
            .result
            .ok_or_else(|| OrchestratorError::TaskNotFound("missing result".into()))?;
        let tools_arr = tools
            .get("tools")
            .and_then(|t| t.as_array())
            .ok_or_else(|| OrchestratorError::TaskNotFound("missing tools array".into()))?;
        assert!(tools_arr.len() >= 12);

        Ok(())
    }

    #[test]
    fn test_hypersonic_master_workflow_closed_loop_execution() -> Result<(), OrchestratorError> {
        let mut orch = EngineeringOrchestrator::new("Hypersonic_Mach11_Flagship");
        let graph = WorkflowRegistry::instantiate_template("hypersonic_full_vehicle_master")?;
        assert_eq!(graph.len(), 11);

        orch.load_task_graph(graph)?;
        let summary = orch.execute_all(true)?;

        assert_eq!(summary.total_tasks, 11);
        assert_eq!(summary.completed_tasks, 11);
        assert_eq!(summary.failed_tasks, 0);
        assert_eq!(summary.execution_status, "Completed");

        // Verify task output artifacts
        let plasma_task = orch
            .active_graph
            .get_task("t3_aero_plasma")
            .ok_or_else(|| OrchestratorError::TaskNotFound("t3_aero_plasma missing".into()))?;
        assert!(plasma_task.result.is_some());

        let scramjet_task = orch
            .active_graph
            .get_task("t4_scramjet_cycle")
            .ok_or_else(|| OrchestratorError::TaskNotFound("t4_scramjet_cycle missing".into()))?;
        assert!(scramjet_task.result.is_some());

        let cert_task = orch
            .active_graph
            .get_task("t11_as9100_cert")
            .ok_or_else(|| OrchestratorError::TaskNotFound("t11_as9100_cert missing".into()))?;
        assert!(cert_task.result.is_some());

        Ok(())
    }
}
