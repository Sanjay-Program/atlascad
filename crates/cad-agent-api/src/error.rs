//! The structured error taxonomy for agent calls.
//!
//! Every failure an agent invocation can produce lands in one of these variants, so callers branch
//! on a typed [`AgentError::kind`] rather than on message strings, and so the registry can tell a
//! *rejected declaration* (configuration, fixable) apart from a *misbehaving agent* (runtime,
//! grounds for demotion) apart from *the caller's own cancellation* (nobody's fault).

use serde::{Deserialize, Serialize};

use crate::contract::AgentId;

/// A structured agent error.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum AgentError {
    /// The contract's `api_version` is not the one this runtime implements.
    #[error("agent {id} declares contract v{got}; this runtime implements v{expected}")]
    IncompatibleContract {
        id: AgentId,
        expected: u32,
        got: u32,
    },

    /// The contract is structurally invalid (bad id, empty metadata, non-positive budget…).
    #[error("invalid agent contract: {0}")]
    InvalidContract(String),

    /// The agent asked for a capability the owner did not grant.
    #[error("permission {permission} denied for agent {agent}")]
    PermissionDenied { agent: AgentId, permission: String },

    /// The agent requested more resources than its budget allows.
    #[error("budget exceeded: {0}")]
    BudgetExhausted(String),

    /// The input does not satisfy the agent's declared input schema (the agent's own verdict).
    #[error("invalid input: {0}")]
    InvalidInput(String),

    /// The call was cancelled by the caller. Not a failure of the agent.
    #[error("cancelled")]
    Cancelled,

    /// The call exceeded the budget's wall-clock limit.
    #[error("timed out after {0:?}")]
    Timeout(core::time::Duration),

    /// The agent is not in a state that allows running (wrong lifecycle state, quota spent).
    #[error("agent {agent} cannot run: {reason}")]
    NotRunnable { agent: AgentId, reason: String },

    /// The agent ran and failed. Carries the agent's own message.
    #[error("agent {agent} failed: {message}")]
    Failed { agent: AgentId, message: String },
}

/// The coarse category of an [`AgentError`], for UI and retry policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ErrorKind {
    Incompatible,
    InvalidContract,
    PermissionDenied,
    BudgetExhausted,
    InvalidInput,
    Cancelled,
    Timeout,
    NotRunnable,
    Failed,
}

impl AgentError {
    /// The category.
    pub const fn kind(&self) -> ErrorKind {
        match self {
            AgentError::IncompatibleContract { .. } => ErrorKind::Incompatible,
            AgentError::InvalidContract(_) => ErrorKind::InvalidContract,
            AgentError::PermissionDenied { .. } => ErrorKind::PermissionDenied,
            AgentError::BudgetExhausted(_) => ErrorKind::BudgetExhausted,
            AgentError::InvalidInput(_) => ErrorKind::InvalidInput,
            AgentError::Cancelled => ErrorKind::Cancelled,
            AgentError::Timeout(_) => ErrorKind::Timeout,
            AgentError::NotRunnable { .. } => ErrorKind::NotRunnable,
            AgentError::Failed { .. } => ErrorKind::Failed,
        }
    }

    /// Should the caller offer a retry of the same call, unchanged?
    ///
    /// Only a timeout and a mid-run infrastructure failure are worth retrying as-is; a permission
    /// or budget problem needs a decision, and an incompatible contract needs a rebuild.
    pub const fn is_retryable(&self) -> bool {
        matches!(self.kind(), ErrorKind::Timeout | ErrorKind::Failed)
    }
}

/// The result type every agent call returns.
pub type Result<T> = core::result::Result<T, AgentError>;

#[cfg(test)]
mod tests {
    use super::{AgentError, ErrorKind};
    use crate::contract::AgentId;

    type TestResult = core::result::Result<(), AgentError>;

    #[test]
    fn kinds_and_retryability() -> TestResult {
        let id = AgentId::new("t.a")?;
        let cases: [(AgentError, ErrorKind, bool); 5] = [
            (
                AgentError::IncompatibleContract {
                    id: id.clone(),
                    expected: 1,
                    got: 2,
                },
                ErrorKind::Incompatible,
                false,
            ),
            (
                AgentError::PermissionDenied {
                    agent: id.clone(),
                    permission: "shell".into(),
                },
                ErrorKind::PermissionDenied,
                false,
            ),
            (AgentError::Cancelled, ErrorKind::Cancelled, false),
            (
                AgentError::Timeout(core::time::Duration::from_secs(1)),
                ErrorKind::Timeout,
                true,
            ),
            (
                AgentError::Failed {
                    agent: id.clone(),
                    message: "boom".into(),
                },
                ErrorKind::Failed,
                true,
            ),
        ];
        for (error, kind, retryable) in cases {
            assert_eq!(error.kind(), kind);
            assert_eq!(error.is_retryable(), retryable, "{error}");
        }
        // Not a flat string taxonomy: the messages carry the agent id.
        let msg = AgentError::Failed {
            agent: id,
            message: "boom".into(),
        }
        .to_string();
        assert!(msg.contains("t.a"), "{msg}");
        Ok(())
    }
}
