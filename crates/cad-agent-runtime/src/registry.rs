//! The registry: lifecycle, grants, budgets and execution.
//!
//! # Lifecycle
//!
//! ```text
//! Discovered --register--> Registered --activate--> Ready --run--> Running --+--> Ready
//!                                ^                                           |
//!                                +--------------- Quarantined <--------------+  (panic)
//! ```
//!
//! * **Discovered** — the runtime knows the agent's contract exists (an entry, nothing more).
//! * **Registered** — the contract validated: right version, sane id, object schemas, positive
//!   budgets. Not runnable yet: nobody has vouched for its behaviour.
//! * **Ready** — activated *with a passing contract-test report* and an owner decision that covers
//!   every owner-gated permission the contract asks for. Activation is the only place `Ready` can
//!   be reached, so "contract tests never ran" and "agent runs" cannot both be true.
//! * **Running** — a call is in flight for this agent.
//! * **Quarantined** — the agent's own code panicked across the call boundary. Nothing runs it
//!   again until it is re-registered; a crash is an infrastructure failure, not a retryable error.
//!
//! The registry is a plain `struct` with interior-free `&mut self` methods: one owner at a time,
//! no hidden global state, and every transition is a checked method rather than a field write.
//!
//! # Grants
//!
//! The effective grant is `contract.permissions ∩ owner_decision`, computed once at activation
//! and passed per call in an [`AgentContext`] with no setters. Owner-gated permissions
//! (`shell`, `secrets`) that are *declared but not granted* refuse activation outright: an agent
//! whose whole purpose needs the shell must not limp along half-privileged, and an agent that
//! quietly loses `secrets` is as dangerous as one that quietly gains it.

use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use cad_agent_api::AgentContext;
use cad_agent_api::agent::Agent;
use cad_agent_api::contract::{AgentContract, AgentId};
use cad_agent_api::error::AgentError;
use cad_agent_api::permission::{OwnerDecision, Permission, PermissionSet};

use crate::schema;

/// Where an agent is in its lifecycle. See the module docs for the legal transitions.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum Lifecycle {
    Discovered,
    Registered,
    Ready,
    Running,
    Quarantined,
}

impl core::fmt::Display for Lifecycle {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let name = match self {
            Lifecycle::Discovered => "discovered",
            Lifecycle::Registered => "registered",
            Lifecycle::Ready => "ready",
            Lifecycle::Running => "running",
            Lifecycle::Quarantined => "quarantined",
        };
        f.write_str(name)
    }
}

/// The outcome of running an agent's declared contract tests.
///
/// The registry does not execute tests itself — it refuses to take anyone's word for it:
/// activation requires a report, and a report that did not pass cannot be turned into `Ready`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ContractTestReport {
    pub passed: bool,
    pub failures: Vec<String>,
}

impl ContractTestReport {
    /// A green run.
    pub fn pass() -> Self {
        Self {
            passed: true,
            failures: Vec::new(),
        }
    }

    /// A red run with reasons.
    pub fn fail(failures: Vec<String>) -> Self {
        Self {
            passed: false,
            failures,
        }
    }
}

/// One registered agent and everything the registry tracks about it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Registration {
    pub lifecycle: Lifecycle,
    pub calls_completed: u64,
    pub calls_refused: u64,
}

/// Which schema a validation failure was about, so errors name their side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallPhase {
    Input,
    Output,
}

/// A registry of agents: discovery, activation, and budgeted execution.
pub struct Registry {
    agents: BTreeMap<AgentId, Box<dyn Agent>>,
    registrations: BTreeMap<AgentId, Registration>,
    decision: OwnerDecision,
}

impl Registry {
    /// An empty registry under an owner decision. Every permission starts denied unless the
    /// decision grants it.
    pub fn new(decision: OwnerDecision) -> Self {
        Self {
            agents: BTreeMap::new(),
            registrations: BTreeMap::new(),
            decision,
        }
    }

    /// The owner decision in force.
    pub fn owner_decision(&self) -> &OwnerDecision {
        &self.decision
    }

    /// Discover an agent: accept its contract if structurally valid, in `discovered` state.
    ///
    /// An incompatible contract version is refused here, before anything is stored.
    pub fn discover(&mut self, agent: Box<dyn Agent>) -> Result<&Registration, AgentError> {
        let contract = agent.contract();
        contract.validate()?;
        // The contract's declared schemas must be schemas this runtime actually enforces, or the
        // agent's I/O checks would be decorative.
        schema::validate_declaration(&contract.input_schema)?;
        schema::validate_declaration(&contract.output_schema)?;
        let id = contract.meta.id.clone();
        // `contract` borrows `agent`; cloning the id is its last use, so the borrow ends here and
        // `agent` can move into the map below.
        if self.agents.contains_key(&id) {
            return Err(AgentError::NotRunnable {
                agent: id,
                reason: "already registered".to_string(),
            });
        }
        let registration = Registration {
            lifecycle: Lifecycle::Discovered,
            calls_completed: 0,
            calls_refused: 0,
        };
        self.registrations.insert(id.clone(), registration);
        self.agents.insert(id.clone(), agent);
        // Re-fetch by id: the contract's borrow of `agent` ended above, so the returned reference
        // is tied to `self`, not to the moved `agent`.
        let registration = self
            .registrations
            .get(&id)
            .ok_or_else(|| AgentError::NotRunnable {
                agent: id,
                reason: "registration vanished during discover".to_string(),
            })?;
        Ok(registration)
    }

    /// Validate and move `discovered -> registered`.
    ///
    /// Idempotent for an agent already registered or ready; a quarantined agent must be
    /// re-registered from scratch, which this supports by resetting the record.
    pub fn register(&mut self, id: &AgentId) -> Result<&Registration, AgentError> {
        let contract = self.contract(id)?.clone();
        contract.validate()?;
        let registration = self.mut_registration(id)?;
        if matches!(
            registration.lifecycle,
            Lifecycle::Discovered | Lifecycle::Registered | Lifecycle::Quarantined
        ) {
            registration.lifecycle = Lifecycle::Registered;
        }
        Ok(self.mut_registration(id)?)
    }

    /// Move `registered -> ready`, but only with a *passing* contract-test report and only if the
    /// owner decision covers every owner-gated permission the contract declares.
    pub fn activate(
        &mut self,
        id: &AgentId,
        report: ContractTestReport,
    ) -> Result<&Registration, AgentError> {
        let contract = self.contract(id)?.clone();
        if !report.passed {
            return Err(AgentError::NotRunnable {
                agent: id.clone(),
                reason: format!(
                    "contract tests did not pass: {}",
                    report.failures.join("; ")
                ),
            });
        }
        // Owner-gated permissions: declared but ungranted means activation is refused, not
        // silently degraded.
        for permission in contract.permissions.iter().filter(|p| p.is_owner_gated()) {
            if !self.decision.allows(permission) {
                return Err(AgentError::PermissionDenied {
                    agent: id.clone(),
                    permission: permission.label().to_string(),
                });
            }
        }
        let registration = self.mut_registration(id)?;
        match registration.lifecycle {
            Lifecycle::Registered | Lifecycle::Ready => {
                registration.lifecycle = Lifecycle::Ready;
                Ok(self.mut_registration(id)?)
            }
            state => Err(AgentError::NotRunnable {
                agent: id.clone(),
                reason: format!("cannot activate from lifecycle {state}"),
            }),
        }
    }

    /// The registered state of one agent.
    pub fn registration(&self, id: &AgentId) -> Result<&Registration, AgentError> {
        self.registrations.get(id).ok_or_else(|| self.unknown(id))
    }

    /// The declaration of a registered agent.
    pub fn contract(&self, id: &AgentId) -> Result<&AgentContract, AgentError> {
        self.agents
            .get(id)
            .ok_or_else(|| self.unknown(id))
            .map(|agent| agent.contract())
    }

    /// Every agent id, in stable order.
    pub fn ids(&self) -> impl Iterator<Item = &AgentId> {
        self.agents.keys()
    }

    /// The effective grant for `id`: contract ∩ owner decision.
    pub fn effective_permissions(&self, id: &AgentId) -> Result<PermissionSet, AgentError> {
        let contract = self.contract(id)?;
        Ok(contract.effective_permissions(&self.decision))
    }

    /// Execute one call under the agent's budgets.
    ///
    /// Enforced here, around the agent, never by the agent:
    ///
    /// 1. lifecycle must be `Ready` (an agent still running or quarantined cannot be called);
    /// 2. the call budget must have room (`max_calls`);
    /// 3. input is checked against the declared input schema;
    /// 4. the context carries the effective grant, an absolute deadline and the output cap;
    /// 5. the agent's own panic is caught and converted to `Failed` + `Quarantined`;
    /// 6. output is checked against the output schema and the output cap;
    /// 7. wall-clock overrun is reported as `Timeout`.
    pub fn run(
        &mut self,
        id: &AgentId,
        input: serde_json::Value,
    ) -> Result<serde_json::Value, AgentError> {
        self.check_runnable(id)?;

        let contract = self.contract(id)?.clone();
        if let Err(error) = schema::validate(&contract.input_schema, &input, "") {
            self.bump_refused(id);
            return Err(AgentError::InvalidInput(error.to_string()));
        }

        let wall = contract.budgets.max_wall_time_ms;
        let granted = contract.effective_permissions(&self.decision);
        let context =
            AgentContext::new(granted, now_ms() + wall, contract.budgets.max_output_bytes);

        self.set_lifecycle(id, Lifecycle::Running)?;
        let started = now_ms();
        let outcome = {
            let agent = self.agents.get(id).ok_or_else(|| self.unknown(id))?;
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| agent.run(&context, input)))
        };
        let elapsed_ms = now_ms().saturating_sub(started);

        let output = match outcome {
            // The agent's own code panicked: quarantine before it can damage anything else.
            Err(_panic) => {
                self.set_lifecycle(id, Lifecycle::Quarantined)?;
                self.bump_refused(id);
                return Err(AgentError::Failed {
                    agent: id.clone(),
                    message: "agent panicked; registration quarantined".to_string(),
                });
            }
            Ok(Ok(output)) => output,
            Ok(Err(error)) => {
                self.set_lifecycle(id, Lifecycle::Ready)?;
                self.bump_refused(id);
                return Err(error);
            }
        };

        if let Err(error) = schema::validate(&contract.output_schema, &output, "") {
            self.set_lifecycle(id, Lifecycle::Ready)?;
            self.bump_refused(id);
            return Err(AgentError::InvalidInput(format!(
                "agent violated its own output schema: {error}"
            )));
        }
        let encoded_len = serde_json::to_string(&output)
            .map_err(|e| AgentError::Failed {
                agent: id.clone(),
                message: format!("agent output is not serialisable: {e}"),
            })?
            .len();
        if encoded_len > contract.budgets.max_output_bytes {
            self.set_lifecycle(id, Lifecycle::Ready)?;
            self.bump_refused(id);
            return Err(AgentError::BudgetExhausted(format!(
                "output is {encoded_len} bytes; the budget is {}",
                contract.budgets.max_output_bytes
            )));
        }
        if elapsed_ms > wall {
            self.set_lifecycle(id, Lifecycle::Ready)?;
            self.bump_refused(id);
            return Err(AgentError::Timeout(core::time::Duration::from_millis(
                elapsed_ms,
            )));
        }

        let registration = self.mut_registration(id)?;
        registration.lifecycle = Lifecycle::Ready;
        registration.calls_completed = registration.calls_completed.saturating_add(1);
        Ok(output)
    }

    fn check_runnable(&mut self, id: &AgentId) -> Result<(), AgentError> {
        let contract = self.contract(id)?.clone();
        let registration = self.mut_registration(id)?;
        match registration.lifecycle {
            Lifecycle::Ready => {}
            state => {
                registration.calls_refused = registration.calls_refused.saturating_add(1);
                return Err(AgentError::NotRunnable {
                    agent: id.clone(),
                    reason: format!("lifecycle is {state}, not ready"),
                });
            }
        }
        if registration.calls_completed >= contract.budgets.max_calls {
            registration.calls_refused = registration.calls_refused.saturating_add(1);
            return Err(AgentError::BudgetExhausted(format!(
                "call budget of {} spent",
                contract.budgets.max_calls
            )));
        }
        Ok(())
    }

    fn bump_refused(&mut self, id: &AgentId) {
        if let Some(registration) = self.registrations.get_mut(id) {
            registration.calls_refused = registration.calls_refused.saturating_add(1);
        }
    }

    fn set_lifecycle(&mut self, id: &AgentId, lifecycle: Lifecycle) -> Result<(), AgentError> {
        let registration = self.mut_registration(id)?;
        registration.lifecycle = lifecycle;
        Ok(())
    }

    fn mut_registration(&mut self, id: &AgentId) -> Result<&mut Registration, AgentError> {
        let error = self.unknown(id);
        self.registrations.get_mut(id).ok_or(error)
    }

    fn unknown(&self, id: &AgentId) -> AgentError {
        AgentError::NotRunnable {
            agent: id.clone(),
            reason: "not registered in this runtime".to_string(),
        }
    }
}

/// Milliseconds since the Unix epoch, for deadlines and elapsed-time accounting.
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Did the agent declare `permission` (as opposed to merely not holding it)?
pub fn declared(contract: &AgentContract, permission: Permission) -> bool {
    contract.permissions.contains(permission)
}

#[cfg(test)]
mod tests {
    use super::{ContractTestReport, Lifecycle, Registry};
    use cad_agent_api::AgentContext;
    use cad_agent_api::agent::Agent;
    use cad_agent_api::contract::{
        AGENT_CONTRACT_VERSION, AgentContract, AgentId, AgentMeta, Budgets,
    };
    use cad_agent_api::error::AgentError;
    use cad_agent_api::permission::{OwnerDecision, Permission, PermissionSet};

    type TestResult = core::result::Result<(), AgentError>;

    /// A well-behaved agent: echoes input, obeys its grant, optionally panics on demand.
    struct TestAgent {
        contract: AgentContract,
    }

    impl TestAgent {
        fn new(id: &str, permissions: PermissionSet) -> Result<Self, AgentError> {
            Ok(TestAgent {
                contract: AgentContract {
                    api_version: AGENT_CONTRACT_VERSION,
                    meta: AgentMeta {
                        id: AgentId::new(id)?,
                        version: "0.1.0".to_string(),
                        name: id.to_string(),
                        purpose: "registry test".to_string(),
                    },
                    permissions,
                    input_schema: serde_json::json!({
                        "type": "object",
                        "required": ["value"],
                        "properties": {"value": {"type": "integer"}}
                    }),
                    output_schema: serde_json::json!({
                        "type": "object",
                        "required": ["ok"],
                        "properties": {"ok": {"type": "boolean"}}
                    }),
                    budgets: Budgets::default(),
                },
            })
        }
    }

    impl Agent for TestAgent {
        fn contract(&self) -> &AgentContract {
            &self.contract
        }

        fn run(
            &self,
            context: &AgentContext,
            input: serde_json::Value,
        ) -> Result<serde_json::Value, AgentError> {
            if input.get("value").and_then(serde_json::Value::as_i64) == Some(i64::MAX) {
                // This test's whole subject is an agent that panics across the call boundary;
                // the workspace denies `panic` in production code, which this is not.
                #[allow(clippy::panic)]
                {
                    panic!("agent-side panic, by request");
                }
            }
            Ok(serde_json::json!({
                "ok": true,
                "can_read": context.allows(Permission::ReadWorkspace),
                "echo": input,
            }))
        }
    }

    fn ready_registry(permissions: PermissionSet) -> Result<(Registry, AgentId), AgentError> {
        let mut registry = Registry::new(OwnerDecision::default().grant(Permission::ReadWorkspace));
        let id = {
            let agent = Box::new(TestAgent::new("t.agent", permissions)?);
            let id = agent.contract().meta.id.clone();
            registry.discover(agent)?;
            id
        };
        registry.register(&id)?;
        registry.activate(&id, ContractTestReport::pass())?;
        Ok((registry, id))
    }

    #[test]
    fn lifecycle_walks_discovered_registered_ready() -> TestResult {
        let mut registry = Registry::new(OwnerDecision::default());
        let agent = Box::new(TestAgent::new("t.life", PermissionSet::new())?);
        let id = agent.contract().meta.id.clone();
        let registration = registry.discover(agent)?;
        assert_eq!(registration.lifecycle, Lifecycle::Discovered);

        // Running before activation is refused, and the refusal counts.
        let early = registry.run(&id, serde_json::json!({"value": 1}));
        assert!(matches!(early, Err(AgentError::NotRunnable { .. })));
        assert_eq!(registry.registration(&id)?.calls_refused, 1);

        let registration = registry.register(&id)?;
        assert_eq!(registration.lifecycle, Lifecycle::Registered);
        registry.activate(&id, ContractTestReport::pass())?;
        assert_eq!(registry.registration(&id)?.lifecycle, Lifecycle::Ready);

        let output = registry.run(&id, serde_json::json!({"value": 7}))?;
        assert_eq!(output["ok"], serde_json::json!(true));
        assert_eq!(registry.registration(&id)?.calls_completed, 1);
        Ok(())
    }

    /// The headline acceptance criterion: contract tests that did not pass never reach `ready`.
    #[test]
    fn activation_refuses_agents_whose_contract_tests_failed() -> TestResult {
        let mut registry = Registry::new(OwnerDecision::default());
        let agent = Box::new(TestAgent::new("t.untested", PermissionSet::new())?);
        let id = agent.contract().meta.id.clone();
        registry.discover(agent)?;
        registry.register(&id)?;
        let report = ContractTestReport::fail(vec!["schema mismatch".to_string()]);
        let err = crate::want_err(registry.activate(&id, report), "activation")?;
        assert!(matches!(err, AgentError::NotRunnable { .. }));
        assert_eq!(registry.registration(&id)?.lifecycle, Lifecycle::Registered);
        Ok(())
    }

    /// An incompatible contract version is rejected at discovery, not stored half-interpreted.
    #[test]
    fn an_incompatible_contract_is_rejected_up_front() -> TestResult {
        struct Foreign {
            contract: AgentContract,
        }
        impl Foreign {
            fn build() -> Result<Box<dyn Agent>, AgentError> {
                Ok(Box::new(Foreign {
                    contract: AgentContract {
                        api_version: AGENT_CONTRACT_VERSION + 7,
                        meta: AgentMeta {
                            id: AgentId::new("t.foreign")?,
                            version: "9.9.9".into(),
                            name: "foreign".into(),
                            purpose: "declares a contract version we do not implement".into(),
                        },
                        permissions: PermissionSet::new(),
                        input_schema: serde_json::json!({"type": "object"}),
                        output_schema: serde_json::json!({"type": "object"}),
                        budgets: Budgets::default(),
                    },
                }))
            }
        }
        impl Agent for Foreign {
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
        let mut registry = Registry::new(OwnerDecision::default());
        let err = crate::want_err(registry.discover(Foreign::build()?), "foreign contract")?;
        assert!(matches!(
            err,
            AgentError::IncompatibleContract {
                expected: 1,
                got: 8,
                ..
            }
        ));
        Ok(())
    }

    /// Shell is owner-gated: declaring it without the owner's decision refuses activation.
    #[test]
    fn shell_requires_an_owner_decision() -> TestResult {
        let shell = PermissionSet::new().with(Permission::Shell);
        let mut undecided = Registry::new(OwnerDecision::default());
        let id = {
            let agent = Box::new(TestAgent::new("t.shell", shell.clone())?);
            let id = agent.contract().meta.id.clone();
            undecided.discover(agent)?;
            id
        };
        undecided.register(&id)?;
        let err = crate::want_err(
            undecided.activate(&id, ContractTestReport::pass()),
            "shell without owner decision",
        )?;
        assert!(matches!(err, AgentError::PermissionDenied { .. }));

        // With the decision, the same declaration activates.
        let mut granted = Registry::new(OwnerDecision::default().grant(Permission::Shell));
        let agent = Box::new(TestAgent::new("t.shell", shell)?);
        let id = agent.contract().meta.id.clone();
        granted.discover(agent)?;
        granted.register(&id)?;
        granted.activate(&id, ContractTestReport::pass())?;
        assert_eq!(granted.registration(&id)?.lifecycle, Lifecycle::Ready);
        Ok(())
    }

    /// The call budget is enforced by the registry, not by the agent.
    #[test]
    fn the_call_budget_is_enforced_from_outside() -> TestResult {
        let mut registry = Registry::new(OwnerDecision::default());
        let agent = {
            let inner = TestAgent::new("t.budget", PermissionSet::new())?;
            let mut contract = inner.contract.clone();
            contract.budgets.max_calls = 2;
            Box::new(Budgeted { contract })
        };
        let id = agent.contract().meta.id.clone();
        registry.discover(agent)?;
        registry.register(&id)?;
        registry.activate(&id, ContractTestReport::pass())?;

        registry.run(&id, serde_json::json!({"value": 1}))?;
        registry.run(&id, serde_json::json!({"value": 2}))?;
        let third = registry.run(&id, serde_json::json!({"value": 3}));
        assert!(matches!(third, Err(AgentError::BudgetExhausted(_))));
        assert_eq!(registry.registration(&id)?.calls_completed, 2);
        Ok(())
    }

    struct Budgeted {
        contract: AgentContract,
    }

    impl Agent for Budgeted {
        fn contract(&self) -> &AgentContract {
            &self.contract
        }
        fn run(
            &self,
            _: &AgentContext,
            _: serde_json::Value,
        ) -> Result<serde_json::Value, AgentError> {
            Ok(serde_json::json!({"ok": true}))
        }
    }

    /// Input that violates the declared schema never reaches the agent.
    #[test]
    fn invalid_input_never_reaches_the_agent() -> TestResult {
        let (mut registry, id) = ready_registry(PermissionSet::new())?;
        let err = crate::want_err(
            registry.run(&id, serde_json::json!({"value": "not a number"})),
            "schema-invalid input",
        )?;
        assert!(matches!(err, AgentError::InvalidInput(_)));
        assert_eq!(registry.registration(&id)?.calls_completed, 0);
        Ok(())
    }

    /// A panicking agent is caught, quarantined and never runs again.
    #[test]
    fn a_panicking_agent_is_quarantined() -> TestResult {
        let (mut registry, id) = ready_registry(PermissionSet::new())?;
        let boom = serde_json::json!({"value": i64::MAX});
        let err = crate::want_err(registry.run(&id, boom), "panicking agent")?;
        assert!(matches!(err, AgentError::Failed { .. }));
        assert_eq!(
            registry.registration(&id)?.lifecycle,
            Lifecycle::Quarantined
        );
        let again = registry.run(&id, serde_json::json!({"value": 1}));
        assert!(matches!(again, Err(AgentError::NotRunnable { .. })));
        Ok(())
    }

    /// The effective grant is the intersection, visible to the agent through its context.
    #[test]
    fn the_grant_reaches_the_agent_as_a_context() -> TestResult {
        let (mut registry, id) =
            ready_registry(PermissionSet::new().with(Permission::ReadWorkspace))?;
        let effective = registry.effective_permissions(&id)?;
        assert!(effective.contains(Permission::ReadWorkspace));
        assert!(!effective.contains(Permission::Network));
        let output = registry.run(&id, serde_json::json!({"value": 1}))?;
        assert_eq!(output["can_read"], serde_json::json!(true));
        Ok(())
    }
}
