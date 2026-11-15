//! Typed error definitions for the multi-agent orchestration engine.

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Error)]
pub enum OrchestratorError {
    #[error("Agent not found: '{0}'")]
    AgentNotFound(String),

    #[error("Task not found: '{0}'")]
    TaskNotFound(String),

    #[error("Dependency cycle detected in engineering task graph")]
    DependencyCycleDetected,

    #[error("Missing required engineering constraints: {0:?}")]
    MissingRequiredConstraints(Vec<String>),

    #[error("Action blocked awaiting human engineering approval (approval ID: '{0}')")]
    ApprovalPending(String),

    #[error("Execution refused: {0}")]
    ExecutionRefused(String),

    #[error("MCP protocol error ({code}): {message}")]
    McpError { code: i64, message: String },

    #[error("Command execution error: {0}")]
    Command(String),

    #[error("Internal orchestrator error: {0}")]
    Internal(String),
}
