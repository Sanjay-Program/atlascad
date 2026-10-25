//! The [`Agent`] trait and the per-call [`AgentContext`].
//!
//! The trait is deliberately tiny: an agent receives a context (what it may do, when it must stop)
//! and JSON that should match its declared input schema, and returns JSON or a structured error.
//! The *registry* in `cad-agent-runtime` owns registration, lifecycle and budget accounting — an
//! agent never counts its own calls or grants itself its own permissions.

use serde::{Deserialize, Serialize};

use crate::contract::AgentContract;
use crate::error::Result;
use crate::permission::PermissionSet;

/// What the runtime hands an agent for the duration of one call.
///
/// The agent cannot widen any of it: there is no setter, and the registry constructs the context
/// fresh for every call from the granted [`PermissionSet`] and the contract's budgets.
#[derive(Debug, Clone)]
pub struct AgentContext {
    /// The effective grant (contract ∩ owner decision). An agent that needs a capability must
    /// find it here; asking again is not possible.
    granted: PermissionSet,
    /// Wall-clock deadline for this call, as milliseconds since the Unix epoch. Using an absolute
    /// instant rather than a `Duration` keeps the context serialisable and immune to clock jumps
    /// mid-call.
    deadline_ms: u64,
    /// The call's output cap, so an agent can chunk instead of blowing the limit at the end.
    max_output_bytes: usize,
}

impl AgentContext {
    /// Constructed only by the runtime; agents receive one, they do not build one.
    pub fn new(granted: PermissionSet, deadline_ms: u64, max_output_bytes: usize) -> Self {
        Self {
            granted,
            deadline_ms,
            max_output_bytes,
        }
    }

    /// Does the agent hold this capability for this call?
    pub fn allows(&self, permission: crate::permission::Permission) -> bool {
        self.granted.contains(permission)
    }

    /// The whole effective grant.
    pub fn granted(&self) -> &PermissionSet {
        &self.granted
    }

    /// Absolute deadline in milliseconds since the Unix epoch.
    pub fn deadline_ms(&self) -> u64 {
        self.deadline_ms
    }

    /// Maximum serialized output bytes for this call.
    pub fn max_output_bytes(&self) -> usize {
        self.max_output_bytes
    }
}

/// One callable agent.
///
/// Implementors provide the declaration ([`Agent::contract`]) and the behaviour
/// ([`Agent::run`]). Both are contract-checked: the registry validates the declaration before the
/// agent may become ready, and enforces the declaration's schemas, permissions and budgets
/// around every `run`.
pub trait Agent: Send + Sync {
    /// The immutable declaration this agent runs under.
    fn contract(&self) -> &AgentContract;

    /// Execute one call.
    ///
    /// `input` should satisfy the contract's input schema; the registry validates it before the
    /// call. The output must satisfy the output schema and fit the context's output budget. An
    /// agent that needs a permission must check `context.allows(..)` and return
    /// [`AgentError::PermissionDenied`](crate::error::AgentError::PermissionDenied) itself for
    /// best-effort cases; the registry rejects outright violations regardless.
    fn run(&self, context: &AgentContext, input: serde_json::Value) -> Result<serde_json::Value>;
}

/// A serialisable snapshot of one registered agent, for status endpoints and logs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentStatus {
    pub id: String,
    pub version: String,
    pub lifecycle: String,
    pub calls_completed: u64,
    pub calls_refused: u64,
}

#[cfg(test)]
mod tests {
    use super::{Agent, AgentContext};
    use crate::contract::{AGENT_CONTRACT_VERSION, AgentContract, AgentId, AgentMeta, Budgets};
    use crate::error::AgentError;
    use crate::permission::{Permission, PermissionSet};

    type TestResult = core::result::Result<(), AgentError>;

    /// An agent that echoes its input and reports whether it holds a permission.
    struct Echo {
        contract: AgentContract,
    }

    impl Echo {
        fn new() -> Result<Self, AgentError> {
            Ok(Echo {
                contract: AgentContract {
                    api_version: AGENT_CONTRACT_VERSION,
                    meta: AgentMeta {
                        id: AgentId::new("test.echo")?,
                        version: "0.0.1".to_string(),
                        name: "echo".to_string(),
                        purpose: "testing".to_string(),
                    },
                    permissions: PermissionSet::new().with(Permission::ReadWorkspace),
                    input_schema: serde_json::json!({"type": "object"}),
                    output_schema: serde_json::json!({"type": "object"}),
                    budgets: Budgets::default(),
                },
            })
        }
    }

    impl Agent for Echo {
        fn contract(&self) -> &AgentContract {
            &self.contract
        }

        fn run(
            &self,
            context: &AgentContext,
            input: serde_json::Value,
        ) -> Result<serde_json::Value, AgentError> {
            let can_read = context.allows(Permission::ReadWorkspace);
            Ok(serde_json::json!({ "echo": input, "can_read": can_read }))
        }
    }

    #[test]
    fn context_is_constructed_by_the_runtime_and_is_read_only() -> TestResult {
        let granted = PermissionSet::new().with(Permission::ReadWorkspace);
        let ctx = AgentContext::new(granted, 1_700_000_000_000, 4096);
        assert!(ctx.allows(Permission::ReadWorkspace));
        assert!(!ctx.allows(Permission::Shell));
        assert_eq!(ctx.max_output_bytes(), 4096);
        assert_eq!(ctx.deadline_ms(), 1_700_000_000_000);
        Ok(())
    }

    #[test]
    fn an_echo_agent_sees_its_grant() -> TestResult {
        let agent = Echo::new()?;
        agent.contract().validate()?;
        let ctx = AgentContext::new(PermissionSet::new(), 0, 1024);
        let out = agent.run(&ctx, serde_json::json!({"x": 1}))?;
        assert_eq!(out["can_read"], serde_json::json!(false));
        Ok(())
    }
}
