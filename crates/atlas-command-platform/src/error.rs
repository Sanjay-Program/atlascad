//! Typed command error definitions for the AtlasCAD universal command platform.

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Error)]
pub enum CommandError {
    #[error("Command not found: '{0}'")]
    CommandNotFound(String),

    #[error("Invalid argument for '{name}': {reason}")]
    InvalidArgument { name: String, reason: String },

    #[error("Missing required argument: '{0}'")]
    MissingRequiredArgument(String),

    #[error("Permission denied: required '{required}', held '{held}'")]
    PermissionDenied { required: String, held: String },

    #[error("Command validation failed: {0}")]
    ValidationFailed(String),

    #[error("Command execution failed: {0}")]
    ExecutionFailed(String),

    #[error(
        "Action requires human engineering approval: command '{command_id}', risk level '{risk_level}'"
    )]
    RequiresApproval {
        command_id: String,
        risk_level: String,
    },

    #[error("Command execution cancelled")]
    Cancelled,

    #[error("Command execution timed out")]
    Timeout,

    #[error("Internal command platform error: {0}")]
    Internal(String),
}
