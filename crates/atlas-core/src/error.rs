//! Core error definitions for the AtlasCAD platform.

use thiserror::Error;

/// Central error type across foundational AtlasCAD subsystems.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum CoreError {
    /// Requested resource was not found.
    #[error("Resource not found: {0}")]
    NotFound(String),

    /// Operation refused due to missing permissions or entitlements.
    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    /// Invalid configuration supplied.
    #[error("Configuration error: {0}")]
    ConfigurationError(String),

    /// Subsystem is in an invalid lifecycle state.
    #[error("Invalid state transition: current={current}, attempted={attempted}")]
    InvalidState {
        current: String,
        attempted: String,
    },

    /// Validation failure on input or model data.
    #[error("Validation failed: {0}")]
    ValidationFailed(String),

    /// Operation deadline exceeded.
    #[error("Operation timed out after {0} seconds")]
    Timeout(u64),

    /// Generic operational failure.
    #[error("Operational error: {0}")]
    OperationFailed(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_formatting() {
        let err = CoreError::NotFound("Document-42".to_string());
        assert_eq!(err.to_string(), "Resource not found: Document-42");

        let state_err = CoreError::InvalidState {
            current: "Suspended".to_string(),
            attempted: "Run".to_string(),
        };
        assert_eq!(
            state_err.to_string(),
            "Invalid state transition: current=Suspended, attempted=Run"
        );
    }
}
