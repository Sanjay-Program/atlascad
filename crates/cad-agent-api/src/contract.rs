//! The agent-side contract types: who an agent is, what it may do, what it accepts and produces,
//! and under which budgets it runs.
//!
//! Everything here is declarative and serialisable. The *enforcement* lives in
//! `cad-agent-runtime`; this crate is what both sides agree on, versioned by
//! [`AGENT_CONTRACT_VERSION`].

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::error::AgentError;
use crate::permission::{OwnerDecision, Permission, PermissionSet};

/// The agent-contract version this workspace implements.
///
/// A contract whose `api_version` differs is refused before anything else happens: an agent
/// written against a different major contract would otherwise be interpreted under rules it was
/// never checked against.
pub const AGENT_CONTRACT_VERSION: u32 = 1;

/// An agent identifier: non-empty, ASCII, `A-Za-z0-9._-`, at most 64 characters.
///
/// The restricted character set keeps ids safe as filesystem fragments, log keys and MCP routing
/// segments without escaping.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AgentId(String);

impl AgentId {
    /// The longest accepted id, in bytes.
    pub const MAX_LEN: usize = 64;

    /// Validate and build an id.
    pub fn new(raw: impl Into<String>) -> Result<Self, AgentError> {
        let raw = raw.into();
        if raw.is_empty() {
            return Err(AgentError::InvalidContract(
                "agent id must not be empty".to_string(),
            ));
        }
        if raw.len() > Self::MAX_LEN {
            return Err(AgentError::InvalidContract(format!(
                "agent id {raw:?} is {} bytes; the limit is {}",
                raw.len(),
                Self::MAX_LEN
            )));
        }
        if !raw
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'))
        {
            return Err(AgentError::InvalidContract(format!(
                "agent id {raw:?} contains characters outside A-Za-z0-9._-"
            )));
        }
        Ok(Self(raw))
    }

    /// The id as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Does this (possibly deserialised, therefore unvalidated) id satisfy the syntax rules?
    ///
    /// `AgentContract::validate` re-checks, so an id smuggled in through `Deserialize` is still
    /// refused before the agent can be registered.
    pub fn is_valid(&self) -> bool {
        Self::new(&self.0).is_ok()
    }
}

impl core::fmt::Display for AgentId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Identity and provenance of an agent, independent of what it is allowed to do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentMeta {
    pub id: AgentId,
    /// The agent's own version (its release cadence, not the contract's).
    pub version: String,
    pub name: String,
    /// One paragraph: what this agent is for. Shown to an operator before granting permissions.
    pub purpose: String,
}

/// Per-invocation resource caps. Every field must be positive — zero would make the agent
/// unrunnable, which is a configuration mistake, not a policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Budgets {
    /// Maximum number of `run` calls in this agent's lifetime at this registry.
    pub max_calls: u64,
    /// Maximum serialized output bytes for one call.
    pub max_output_bytes: usize,
    /// Maximum wall-clock time for one call, in milliseconds.
    pub max_wall_time_ms: u64,
}

impl Default for Budgets {
    fn default() -> Self {
        Budgets {
            max_calls: 1_000,
            max_output_bytes: 1 << 20,
            max_wall_time_ms: 30_000,
        }
    }
}

impl Budgets {
    /// Reject non-positive budgets and an output cap above 64 MiB (a bigger reply is a bug, not
    /// a workload).
    pub fn validate(&self) -> Result<(), AgentError> {
        const MAX_OUTPUT_BYTES: usize = 64 << 20;
        if self.max_calls == 0 {
            return Err(AgentError::InvalidContract(
                "budget max_calls must be positive".to_string(),
            ));
        }
        if self.max_output_bytes == 0 || self.max_output_bytes > MAX_OUTPUT_BYTES {
            return Err(AgentError::InvalidContract(format!(
                "budget max_output_bytes must be in 1..={MAX_OUTPUT_BYTES}, got {}",
                self.max_output_bytes
            )));
        }
        if self.max_wall_time_ms == 0 {
            return Err(AgentError::InvalidContract(
                "budget max_wall_time_ms must be positive".to_string(),
            ));
        }
        Ok(())
    }
}

/// A complete, self-describing agent declaration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentContract {
    /// The contract version this agent was written against.
    pub api_version: u32,
    pub meta: AgentMeta,
    /// Capabilities the agent asks for. Asking is not granting: the runtime intersects this with
    /// the owner's decision (see [`OwnerDecision`]).
    pub permissions: PermissionSet,
    /// JSON Schema for the `run` input.
    pub input_schema: serde_json::Value,
    /// JSON Schema for a successful `run` output.
    pub output_schema: serde_json::Value,
    pub budgets: Budgets,
}

impl AgentContract {
    /// Check everything that can be checked without running the agent.
    ///
    /// The runtime calls this before an agent may leave the `discovered` state, so an
    /// ill-formed declaration never becomes loadable.
    pub fn validate(&self) -> Result<(), AgentError> {
        if !self.meta.id.is_valid() {
            return Err(AgentError::InvalidContract(format!(
                "agent id {:?} violates the syntax rules",
                self.meta.id.as_str()
            )));
        }
        if self.api_version != AGENT_CONTRACT_VERSION {
            return Err(AgentError::IncompatibleContract {
                id: self.meta.id.clone(),
                expected: AGENT_CONTRACT_VERSION,
                got: self.api_version,
            });
        }
        if self.meta.version.is_empty() {
            return Err(AgentError::InvalidContract(format!(
                "agent {} has an empty version",
                self.meta.id
            )));
        }
        if self.meta.name.is_empty() {
            return Err(AgentError::InvalidContract(format!(
                "agent {} has an empty name",
                self.meta.id
            )));
        }
        if !matches!(
            self.input_schema.get("type"),
            Some(serde_json::Value::String(_))
        ) {
            return Err(AgentError::InvalidContract(format!(
                "agent {} input_schema must be an object schema with a \"type\"",
                self.meta.id
            )));
        }
        if !matches!(
            self.output_schema.get("type"),
            Some(serde_json::Value::String(_))
        ) {
            return Err(AgentError::InvalidContract(format!(
                "agent {} output_schema must be an object schema with a \"type\"",
                self.meta.id
            )));
        }
        self.budgets.validate()?;
        Ok(())
    }

    /// Permissions this contract asks for that the owner has *not* granted.
    ///
    /// An empty result means the runtime may grant the declared set as-is.
    pub fn ungranted(&self, decision: &OwnerDecision) -> Vec<Permission> {
        self.permissions
            .iter()
            .filter(|p| !decision.allows(*p))
            .collect()
    }

    /// The set of permissions the owner is willing to grant, intersected with what the agent
    /// asked for — the grant is never wider than the request.
    pub fn effective_permissions(&self, decision: &OwnerDecision) -> PermissionSet {
        PermissionSet::from_iter(self.permissions.iter().filter(|p| decision.allows(*p)))
    }
}

/// Convenience: a set with no members, for agents that need no permissions.
pub fn no_permissions() -> BTreeSet<Permission> {
    BTreeSet::new()
}

#[cfg(test)]
mod tests {
    use super::{AGENT_CONTRACT_VERSION, AgentContract, AgentId, AgentMeta, Budgets};
    use crate::error::AgentError;
    use crate::permission::{OwnerDecision, Permission, PermissionSet};

    type TestResult = core::result::Result<(), AgentError>;

    fn contract() -> Result<AgentContract, AgentError> {
        Ok(AgentContract {
            api_version: AGENT_CONTRACT_VERSION,
            meta: AgentMeta {
                id: AgentId::new("test.geometry")?,
                version: "0.1.0".to_string(),
                name: "test agent".to_string(),
                purpose: "exercise the runtime".to_string(),
            },
            permissions: PermissionSet::from_iter([Permission::ReadWorkspace]),
            input_schema: serde_json::json!({"type": "object"}),
            output_schema: serde_json::json!({"type": "object"}),
            budgets: Budgets::default(),
        })
    }

    #[test]
    fn ids_reject_empty_overlong_and_unsafe() -> TestResult {
        assert!(AgentId::new("").is_err());
        assert!(AgentId::new("a".repeat(65)).is_err());
        assert!(AgentId::new("has space").is_err());
        assert!(AgentId::new("path/../escape").is_err());
        match AgentId::new("good.Id_1") {
            Ok(id) => assert_eq!(id.as_str(), "good.Id_1"),
            Err(err) => return Err(err),
        }
        Ok(())
    }

    #[test]
    fn a_foreign_contract_version_is_incompatible() -> TestResult {
        let mut c = contract()?;
        c.api_version = AGENT_CONTRACT_VERSION + 1;
        let err = match c.validate() {
            Ok(()) => return Err(AgentError::InvalidContract("should have failed".into())),
            Err(err) => err,
        };
        assert!(matches!(err, AgentError::IncompatibleContract { .. }));
        Ok(())
    }

    /// An id smuggled through the `Deserialize` path (which cannot fail mid-parse) is still
    /// caught by `validate`.
    #[test]
    fn a_deserialised_bad_id_is_caught_by_validate() -> TestResult {
        let mut c = contract()?;
        c.meta.id = AgentId("bad id".to_string());
        assert!(c.validate().is_err());
        assert!(!c.meta.id.is_valid());
        Ok(())
    }

    #[test]
    fn grant_is_never_wider_than_the_request() -> TestResult {
        let mut c = contract()?;
        c.permissions = PermissionSet::from_iter([Permission::ReadWorkspace, Permission::Network]);
        let decision = OwnerDecision::default()
            .grant(Permission::ReadWorkspace)
            .grant(Permission::Network);
        let effective = c.effective_permissions(&decision);
        assert!(effective.contains(Permission::ReadWorkspace));
        assert!(effective.contains(Permission::Network));

        // Owner refuses network: effective shrinks to the read permission only.
        let partial = OwnerDecision::default().grant(Permission::ReadWorkspace);
        let effective = c.effective_permissions(&partial);
        assert!(effective.contains(Permission::ReadWorkspace));
        assert!(!effective.contains(Permission::Network));
        assert_eq!(c.ungranted(&partial), vec![Permission::Network]);
        Ok(())
    }

    #[test]
    fn zero_budgets_are_a_configuration_error() -> TestResult {
        let mut c = contract()?;
        c.budgets.max_calls = 0;
        assert!(c.validate().is_err());
        c.budgets = Budgets::default();
        c.budgets.max_wall_time_ms = 0;
        assert!(c.validate().is_err());
        c.budgets = Budgets::default();
        c.budgets.max_output_bytes = 128 << 20;
        assert!(c.validate().is_err());
        assert!(contract()?.validate().is_ok());
        Ok(())
    }
}
