//! The versioned CadAgent contract.
//!
//! [`cad-agent-runtime`](https://example.invalid) enforces this contract; this crate is only the
//! agreement both sides are checked against, so an agent implementation depends on the contract
//! without dragging in the registry, the scheduler or any permission machinery.
//!
//! # What is in the contract
//!
//! * [`AgentContract`] — metadata, declared [`Permission`][`permission::Permission`]s, JSON
//!   Schemas for input and output, and [`Budgets`], all validated by
//!   [`AgentContract::validate`] and versioned by [`AGENT_CONTRACT_VERSION`].
//! * [`Agent`] — the callable interface: one method, JSON in, JSON out, structured errors.
//! * [`AgentContext`] — what the runtime grants for one call and when the call must stop. It has
//!   no setters: an agent cannot widen its own permissions or budget.
//! * [`AgentError`] — one structured taxonomy, with [`AgentError::kind`] for branching and
//!   [`AgentError::is_retryable`] for policy.
//!
//! # Versioning
//!
//! [`AGENT_CONTRACT_VERSION`] is a single integer. A contract that declares a different value is
//! refused up front (`AgentError::IncompatibleContract`) rather than half-interpreted, which is
//! the behaviour the registry's compatibility test pins.
//!
//! # Deny-by-default
//!
//! Permissions are capabilities an agent *asks* for and an owner *grants*. [`OwnerDecision`][`permission::OwnerDecision`]
//! starts empty, `shell` and `secrets` are owner-gated, and the effective grant is always the
//! intersection of the two — never the declaration alone.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

pub mod agent;
pub mod contract;
pub mod error;
pub mod permission;

pub use agent::{Agent, AgentContext, AgentStatus};
pub use contract::{AGENT_CONTRACT_VERSION, AgentContract, AgentId, AgentMeta, Budgets};
pub use error::{AgentError, ErrorKind, Result};
pub use permission::{OwnerDecision, Permission, PermissionSet};

#[cfg(test)]
mod tests {
    use super::{
        AGENT_CONTRACT_VERSION, AgentContract, AgentId, AgentMeta, Budgets, OwnerDecision,
        Permission, PermissionSet,
    };
    use crate::error::AgentError;

    type TestResult = core::result::Result<(), AgentError>;

    fn contract_with(permissions: PermissionSet) -> Result<AgentContract, AgentError> {
        Ok(AgentContract {
            api_version: AGENT_CONTRACT_VERSION,
            meta: AgentMeta {
                id: AgentId::new("round.trip")?,
                version: "1.0.0".to_string(),
                name: "round trip".to_string(),
                purpose: "json round trip".to_string(),
            },
            permissions,
            input_schema: serde_json::json!({"type": "object"}),
            output_schema: serde_json::json!({"type": "object"}),
            budgets: Budgets::default(),
        })
    }

    /// A contract survives JSON, and the surviving copy still validates — the registry stores and
    /// reloads contracts from disk, so this is the wire-level guarantee that path is lossless.
    #[test]
    fn contract_round_trips_through_json() -> TestResult {
        let original = contract_with(
            PermissionSet::new()
                .with(Permission::ReadWorkspace)
                .with(Permission::WriteWorkspace),
        )?;
        let agent = AgentId::new("round.trip")?;
        let json = serde_json::to_string(&original).map_err(|e| AgentError::Failed {
            agent: agent.clone(),
            message: e.to_string(),
        })?;
        let back: AgentContract = serde_json::from_str(&json).map_err(|e| AgentError::Failed {
            agent,
            message: e.to_string(),
        })?;
        assert_eq!(back, original);
        back.validate()?;
        Ok(())
    }

    /// The owner-decision intersection is the whole security story; this pins it end-to-end at
    /// the crate boundary.
    #[test]
    fn effective_permissions_are_request_intersect_decision() -> TestResult {
        let declared = PermissionSet::new().with(Permission::Network);
        let contract = contract_with(declared)?;
        let grant_nothing = OwnerDecision::default();
        let effective = contract.effective_permissions(&grant_nothing);
        assert!(effective.is_empty(), "no decision, no capability");

        let grant_network = OwnerDecision::default().grant(Permission::Network);
        let effective = contract.effective_permissions(&grant_network);
        assert!(effective.contains(Permission::Network));

        // Even a decision that allows *everything* cannot widen the request: shell is not asked
        // for here, so it is not granted.
        let grant_all = OwnerDecision::default()
            .grant(Permission::Network)
            .grant(Permission::Shell)
            .grant(Permission::Secrets)
            .grant(Permission::ReadWorkspace)
            .grant(Permission::WriteWorkspace);
        let effective = contract.effective_permissions(&grant_all);
        assert!(!effective.contains(Permission::Shell));
        assert_eq!(effective.len(), 1);
        Ok(())
    }
}
