//! The CadAgent runtime.
//!
//! [`cad-agent-api`] defines what an agent *is*; this crate is the process-side machinery that
//! decides whether it may run at all:
//!
//! * [`registry::Registry`] — the discovered → registered → ready lifecycle. An agent reaches
//!   `ready` only with a passing [`registry::ContractTestReport`] and an owner decision that
//!   covers every owner-gated permission it declares.
//! * [`registry::Lifecycle`] — the states, with the transitions as checked methods rather than
//!   field writes, so "contract tests never ran" and "the agent is running" cannot both hold.
//! * capability grants — the effective permission set is `contract ∩ owner_decision`, computed by
//!   the registry and handed to the agent in a read-only [`AgentContext`](cad_agent_api::AgentContext);
//!   `shell` and `secrets` refuse activation when declared but ungranted.
//! * budgets — call count, wall-clock deadline and output size are enforced *around* the agent;
//!   an agent's own panic is caught and quarantines it.
//! * [`schema`] — a strict subset checker for declared input/output schemas, which refuses
//!   keywords it does not enforce instead of pretending to.
//!
//! # Example
//!
//! ```text
//! let mut registry = Registry::new(OwnerDecision::default().grant(Permission::ReadWorkspace));
//! registry.discover(Box::new(MyAgent::new()?))?;
//! registry.register(&id)?;
//! registry.activate(&id, ContractTestReport::pass())?;
//! let output = registry.run(&id, serde_json::json!({"value": 1}))?;
//! ```

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

pub mod registry;
pub mod schema;

pub use registry::{ContractTestReport, Lifecycle, Registration, Registry, declared};
pub use schema::{SchemaError, validate as validate_against};

/// Test-only: assert a call failed, without `expect_err` (the workspace denies `expect_used`,
/// and a test that turns a surprise `Ok` into a panic message loses the actual value anyway).
#[cfg(test)]
pub(crate) fn want_err<T>(
    result: Result<T, cad_agent_api::AgentError>,
    what: &str,
) -> Result<cad_agent_api::AgentError, cad_agent_api::AgentError> {
    match result {
        Ok(_) => Err(cad_agent_api::AgentError::InvalidContract(format!(
            "expected {what} to fail"
        ))),
        Err(err) => Ok(err),
    }
}

#[cfg(test)]
mod tests {
    use super::{ContractTestReport, Lifecycle, Registry};
    use cad_agent_api::AgentContext;
    use cad_agent_api::agent::Agent;
    use cad_agent_api::contract::{AgentContract, AgentId, AgentMeta, Budgets};
    use cad_agent_api::error::AgentError;
    use cad_agent_api::permission::{OwnerDecision, Permission, PermissionSet};

    type TestResult = core::result::Result<(), AgentError>;

    /// A minimal agent whose contract carries object schemas with an `enum`, exercising the
    /// declaration-checking path end to end.
    struct Tiny {
        contract: AgentContract,
    }

    impl Tiny {
        fn build() -> Result<Tiny, AgentError> {
            Ok(Tiny {
                contract: AgentContract {
                    api_version: cad_agent_api::AGENT_CONTRACT_VERSION,
                    meta: AgentMeta {
                        id: AgentId::new("t.tiny")?,
                        version: "1.0.0".to_string(),
                        name: "tiny".to_string(),
                        purpose: "crate-level test".to_string(),
                    },
                    permissions: PermissionSet::new().with(Permission::ReadWorkspace),
                    input_schema: serde_json::json!({
                        "type": "object",
                        "required": ["op"],
                        "properties": {"op": {"type": "string", "enum": ["ping"]}}
                    }),
                    output_schema: serde_json::json!({
                        "type": "object",
                        "required": ["pong"],
                        "properties": {"pong": {"type": "boolean"}}
                    }),
                    budgets: Budgets::default(),
                },
            })
        }
    }

    impl Agent for Tiny {
        fn contract(&self) -> &AgentContract {
            &self.contract
        }
        fn run(
            &self,
            _: &AgentContext,
            input: serde_json::Value,
        ) -> Result<serde_json::Value, AgentError> {
            let pong = input.get("op").and_then(serde_json::Value::as_str) == Some("ping");
            Ok(serde_json::json!({"pong": pong}))
        }
    }

    /// The documented happy path, at the crate boundary: discover, register, activate, run.
    #[test]
    fn end_to_end_registration_and_run() -> TestResult {
        let mut registry = Registry::new(OwnerDecision::default().grant(Permission::ReadWorkspace));
        let agent = Box::new(Tiny::build()?);
        let id = agent.contract().meta.id.clone();
        registry.discover(agent)?;
        registry.register(&id)?;
        registry.activate(&id, ContractTestReport::pass())?;
        assert_eq!(registry.registration(&id)?.lifecycle, Lifecycle::Ready);

        let output = registry.run(&id, serde_json::json!({"op": "ping"}))?;
        assert_eq!(output["pong"], serde_json::json!(true));

        // The wrong enum value is caught by the schema, not passed through to the agent.
        let err = crate::want_err(
            registry.run(&id, serde_json::json!({"op": "explode"})),
            "enum violation",
        )?;
        assert!(matches!(err, AgentError::InvalidInput(_)));
        Ok(())
    }

    /// A contract whose I/O schema uses keywords the runtime does not enforce must fail at
    /// discovery — a declaration that promises more than the runtime checks is a lie.
    #[test]
    fn a_contract_promising_unenforceable_schemas_is_refused() -> TestResult {
        struct Bad {
            contract: AgentContract,
        }
        impl Agent for Bad {
            fn contract(&self) -> &AgentContract {
                &self.contract
            }
            fn run(
                &self,
                _: &AgentContext,
                _: serde_json::Value,
            ) -> Result<serde_json::Value, AgentError> {
                Err(AgentError::Cancelled)
            }
        }
        let mut contract = Tiny::build()?.contract;
        contract.input_schema = serde_json::json!({
            "type": "object",
            "additionalProperties": false
        });
        let mut registry = Registry::new(OwnerDecision::default());
        let err = crate::want_err(
            registry.discover(Box::new(Bad { contract })),
            "unenforceable schema",
        )?;
        assert!(matches!(err, AgentError::InvalidInput(_)));
        Ok(())
    }
}
