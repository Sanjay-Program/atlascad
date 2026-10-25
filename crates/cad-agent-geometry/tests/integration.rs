//! The geometry-ops agent running inside a real [`Registry`].
//!
//! These are the tests that make "there is an agent runtime" and "there is a working agent" the
//! same statement. Every assertion here is about the *interaction*: the registry enforces the
//! contract, the agent obeys the grant it is handed, the kernel's cancellation reaches the agent,
//! and a failure leaves both the agent and the registry in a state the next call can use.
//!
//! The agent is deliberately registered the way a host would register it — discovered, registered,
//! activated with a contract-test report — because that sequence is where the runtime's guarantees
//! live. A test that called `Agent::run` directly would prove nothing about the runtime.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use cad_agent_api::error::AgentError;
use cad_agent_api::permission::{OwnerDecision, Permission, PermissionSet};
use cad_agent_geometry::GeometryAgent;
use cad_agent_runtime::registry::{ContractTestReport, Lifecycle, Registry};
use cad_kernel_truck::CancellationToken;
use serde_json::{Value, json};

/// A scratch directory that removes itself when the test ends.
struct Scratch {
    path: PathBuf,
}

impl Scratch {
    /// The agent id used in the errors this helper raises.
    fn agent_id() -> Result<cad_agent_api::AgentId, AgentError> {
        cad_agent_api::AgentId::new("test.scratch")
    }

    fn new(tag: &str) -> Result<Self, AgentError> {
        let agent = Self::agent_id()?;
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let path = std::env::temp_dir().join(format!(
            "atlas-agent-{tag}-{}-{nanos}",
            COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::create_dir_all(&path).map_err(|error| AgentError::Failed {
            agent,
            message: format!("cannot create {}: {error}", path.display()),
        })?;
        Ok(Scratch { path })
    }

    fn write(&self, name: &str, contents: &str) -> Result<PathBuf, AgentError> {
        let agent = Self::agent_id()?;
        let path = self.path.join(name);
        std::fs::write(&path, contents).map_err(|error| AgentError::Failed {
            agent,
            message: format!("cannot write {}: {error}", path.display()),
        })?;
        Ok(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// A registry with the geometry agent discovered, registered and activated.
///
/// `grants` decides what the owner allows; the agent's own declaration is fixed.
fn ready_registry(
    grants: PermissionSet,
    workspace: Option<&Path>,
) -> Result<(Registry, cad_agent_api::AgentId), AgentError> {
    let token = CancellationToken::new();
    let mut agent = GeometryAgent::new(token)?;
    if let Some(root) = workspace {
        agent = agent.with_workspace(root)?;
    }
    let id = agent.contract().meta.id.clone();
    // The owner allows exactly what the test asks for; the agent only ever sees the
    // intersection with its own declaration.
    let mut owner = OwnerDecision::default();
    for permission in grants.iter() {
        owner = owner.grant(permission);
    }
    let mut registry = Registry::new(owner);
    registry.discover(Box::new(agent))?;
    registry.register(&id)?;
    registry.activate(&id, ContractTestReport::pass())?;
    Ok((registry, id))
}

/// A decision that grants only what the test asks for.
fn decision(grants: &[Permission]) -> OwnerDecision {
    let mut decision = OwnerDecision::default();
    for permission in grants {
        decision = decision.grant(*permission);
    }
    decision
}

/// Budgets with one field changed from the defaults.
fn budgets_with(
    max_calls: u64,
    max_output_bytes: usize,
    max_wall_time_ms: u64,
) -> cad_agent_api::Budgets {
    cad_agent_api::Budgets {
        max_calls,
        max_output_bytes,
        max_wall_time_ms,
    }
}

/// The error of a call that must fail.
///
/// `expect_err` is denied by the workspace lints, and it would panic rather than report: a test
/// that turns a surprise `Ok` into a panic message loses the value it was about to inspect. This
/// returns the error as a `Result`, so the test fails with the offending value in the message.
fn want_err<T: std::fmt::Debug>(
    result: Result<T, AgentError>,
    what: &str,
) -> Result<AgentError, AgentError> {
    match result {
        Ok(value) => Err(AgentError::InvalidContract(format!(
            "expected {what} to fail, but it returned {value:?}"
        ))),
        Err(error) => Ok(error),
    }
}

/// The headline workflow: extrude, measure, tessellate — through the registry, with the contract's
/// schemas and budgets in force.
#[test]
fn a_geometry_workflow_runs_through_the_registry() -> Result<(), AgentError> {
    let mut registry = Registry::new(OwnerDecision::default());
    let agent = GeometryAgent::new(CancellationToken::new())?;
    let id = agent.contract().meta.id.clone();
    registry.discover(Box::new(agent))?;
    registry.register(&id)?;
    registry.activate(&id, ContractTestReport::pass())?;

    // Extrude a 10 x 10 x 5 mm box.
    let extruded = registry.run(
        &id,
        json!({
            "op": "extrude",
            "profile": [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]],
            "height_mm": 5.0
        }),
    )?;
    let solid = extruded["solid"].as_u64().unwrap_or(0);
    assert!(solid > 0, "a handle was issued");

    // The analytic volume is 500 mm³.
    let volume = registry.run(&id, json!({"op": "volume", "solid": solid}))?;
    let measured = volume["volume_mm3"].as_f64().unwrap_or(f64::NAN);
    assert!(
        (measured - 500.0).abs() < 1e-6,
        "expected 500 mm³, got {measured}"
    );

    // The tessellation is a closed, outward-oriented mesh of 12 triangles.
    let mesh = registry.run(&id, json!({"op": "tessellate", "solid": solid}))?;
    assert_eq!(mesh["triangles"], json!(12));
    assert_eq!(mesh["vertices"], json!(8));
    assert_eq!(mesh["open_edges"], json!(0));
    assert_eq!(mesh["min"], json!([0.0, 0.0, 0.0]));
    assert_eq!(mesh["max"], json!([10.0, 10.0, 5.0]));

    // The agent ran three times and stayed ready.
    let registration = registry.registration(&id)?;
    assert_eq!(registration.lifecycle, Lifecycle::Ready);
    assert_eq!(registration.calls_completed, 3);
    assert_eq!(registration.calls_refused, 0);
    Ok(())
}

/// A boolean between two solids, with the analytic volume checked.
#[test]
fn a_boolean_produces_the_analytic_volume() -> Result<(), AgentError> {
    let (mut registry, id) = ready_registry(PermissionSet::new(), None)?;

    let a = registry.run(
        &id,
        json!({
            "op": "extrude",
            "profile": [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]],
            "height_mm": 10.0
        }),
    )?;
    let b = registry.run(
        &id,
        json!({
            "op": "extrude",
            "profile": [[8.0, 8.0], [12.0, 8.0], [12.0, 12.0], [8.0, 12.0]],
            "height_mm": 10.0
        }),
    )?;
    let joined = registry.run(
        &id,
        json!({
            "op": "boolean",
            "operation": "union",
            "a": a["solid"],
            "b": b["solid"]
        }),
    )?;
    let volume = registry.run(&id, json!({"op": "volume", "solid": joined["solid"]}))?;
    let measured = volume["volume_mm3"].as_f64().unwrap_or(f64::NAN);
    // 1000 + 160 - 40 = 1120 mm³.
    assert!(
        (measured - 1120.0).abs() < 1.0,
        "expected 1120 mm³, got {measured}"
    );
    Ok(())
}

/// The declared input schema is enforced by the runtime, before the agent is called.
#[test]
fn the_declared_schema_is_enforced_by_the_registry() -> Result<(), AgentError> {
    let (mut registry, id) = ready_registry(PermissionSet::new(), None)?;

    // An operation that does not exist.
    let error = want_err(
        registry.run(&id, json!({"op": "teleport"})),
        "an unknown op must be refused",
    )?;
    assert!(matches!(error, AgentError::InvalidInput(_)), "{error}");

    // A height above the declared maximum.
    let error = want_err(
        registry.run(
            &id,
            json!({
                "op": "extrude",
                "profile": [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0]],
                "height_mm": 1.0e9
            }),
        ),
        "a height above the maximum must be refused",
    )?;
    assert!(matches!(error, AgentError::InvalidInput(_)), "{error}");

    // A profile with two points: below the declared minimum.
    let error = want_err(
        registry.run(
            &id,
            json!({"op": "extrude", "profile": [[0.0, 0.0], [1.0, 0.0]], "height_mm": 5.0}),
        ),
        "a two-point profile must be refused",
    )?;
    assert!(matches!(error, AgentError::InvalidInput(_)), "{error}");

    // None of the refused calls reached the agent.
    assert_eq!(registry.registration(&id)?.calls_completed, 0);
    assert_eq!(registry.registration(&id)?.calls_refused, 3);
    Ok(())
}

/// Capability-based access control, end to end: without the grant the file-reading operation is
/// refused; with it, the same call succeeds.
#[test]
fn the_read_workspace_grant_decides_the_file_operation() -> Result<(), AgentError> {
    let scratch = Scratch::new("grant")?;
    scratch.write("bracket.json", "[[0,0],[10,0],[10,10],[0,10]]")?;

    // The agent declares `read_workspace`; the owner has not granted it.
    let mut registry = Registry::new(OwnerDecision::default());
    let agent = GeometryAgent::new(CancellationToken::new())?.with_workspace(&scratch.path)?;
    let id = agent.contract().meta.id.clone();
    registry.discover(Box::new(agent))?;
    registry.register(&id)?;

    // Activation still succeeds: `read_workspace` is not owner-gated, so the agent is simply
    // activated with a reduced grant.
    registry.activate(&id, ContractTestReport::pass())?;
    let effective = registry.effective_permissions(&id)?;
    assert!(!effective.contains(Permission::ReadWorkspace));

    let error = want_err(registry
        .run(
            &id,
            json!({"op": "extrude_from_workspace", "profile_file": "bracket.json", "height_mm": 5.0}),
        ), "reading a file without the grant must be refused")?;
    assert!(
        matches!(error, AgentError::PermissionDenied { .. }),
        "{error}"
    );

    // Every other operation still works.
    let capabilities = registry.run(&id, json!({"op": "capabilities"}))?;
    assert_eq!(capabilities["kernel"], json!("truck"));

    // With the grant, the same call succeeds.
    let mut granted = Registry::new(decision(&[Permission::ReadWorkspace]));
    let agent = GeometryAgent::new(CancellationToken::new())?.with_workspace(&scratch.path)?;
    let granted_id = agent.contract().meta.id.clone();
    granted.discover(Box::new(agent))?;
    granted.register(&granted_id)?;
    granted.activate(&granted_id, ContractTestReport::pass())?;
    assert!(
        granted
            .effective_permissions(&granted_id)?
            .contains(Permission::ReadWorkspace)
    );
    let extruded = granted.run(
        &granted_id,
        json!({"op": "extrude_from_workspace", "profile_file": "bracket.json", "height_mm": 5.0}),
    )?;
    assert!(extruded["solid"].as_u64().unwrap_or(0) > 0);
    Ok(())
}

/// A path that leaves the workspace is refused, whatever the grant says.
#[test]
fn a_traversing_profile_name_is_refused() -> Result<(), AgentError> {
    let scratch = Scratch::new("traversal")?;
    let (mut registry, id) = ready_registry(
        PermissionSet::new().with(Permission::ReadWorkspace),
        Some(&scratch.path),
    )?;
    for name in ["../etc/passwd", "/etc/passwd", "sub/other.json", ".."] {
        let error = want_err(
            registry.run(
                &id,
                json!({"op": "extrude_from_workspace", "profile_file": name, "height_mm": 5.0}),
            ),
            "a traversing name must be refused",
        )?;
        assert!(
            matches!(
                error,
                AgentError::InvalidInput(_) | AgentError::PermissionDenied { .. }
            ),
            "{name}: {error}"
        );
    }
    Ok(())
}

/// The agent's contract must be validated before it may run: a failed contract-test report keeps
/// it out of `ready`.
#[test]
fn an_agent_whose_contract_tests_failed_never_runs() -> Result<(), AgentError> {
    let mut registry = Registry::new(OwnerDecision::default());
    let agent = GeometryAgent::new(CancellationToken::new())?;
    let id = agent.contract().meta.id.clone();
    registry.discover(Box::new(agent))?;
    registry.register(&id)?;

    let error = want_err(
        registry.activate(
            &id,
            ContractTestReport::fail(vec!["mesh orientation".into()]),
        ),
        "activation must be refused",
    )?;
    assert!(matches!(error, AgentError::NotRunnable { .. }), "{error}");
    assert_eq!(registry.registration(&id)?.lifecycle, Lifecycle::Registered);

    let error = want_err(
        registry.run(&id, json!({"op": "capabilities"})),
        "an agent that is not ready must not run",
    )?;
    assert!(matches!(error, AgentError::NotRunnable { .. }), "{error}");
    Ok(())
}

/// The caller's cancellation token reaches the kernel, and the agent survives the cancellation.
#[test]
fn a_cancelled_call_is_reported_as_cancelled_and_the_agent_survives() -> Result<(), AgentError> {
    let token = CancellationToken::new();
    let mut registry = Registry::new(OwnerDecision::default());
    let agent = GeometryAgent::new(token.clone())?;
    let id = agent.contract().meta.id.clone();
    registry.discover(Box::new(agent))?;
    registry.register(&id)?;
    registry.activate(&id, ContractTestReport::pass())?;

    // Cancel before the call: the kernel's budget refuses at its first checkpoint.
    token.cancel();
    let error = want_err(
        registry.run(
            &id,
            json!({
                "op": "extrude",
                "profile": [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]],
                "height_mm": 5.0
            }),
        ),
        "a cancelled call must not succeed",
    )?;
    assert!(
        matches!(error, AgentError::Cancelled),
        "expected Cancelled, got {error}"
    );

    // The host resets the token — the documented rule for a shared token — and the agent works.
    token.reset();
    let reply = registry.run(
        &id,
        json!({
            "op": "extrude",
            "profile": [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]],
            "height_mm": 5.0
        }),
    )?;
    assert!(reply["solid"].as_u64().unwrap_or(0) > 0);

    // The agent is still ready and its state is intact.
    assert_eq!(registry.registration(&id)?.lifecycle, Lifecycle::Ready);
    Ok(())
}

/// The runtime's deadline reaches the kernel: a call that outlives its wall-clock budget is a
/// timeout, not a hang.
///
/// The budget is measured from the moment the call starts — the runtime computes the deadline when
/// it hands the context to the agent — so the test needs an operation that genuinely takes longer
/// than the budget allows. A one-millisecond budget and a profile with thousands of points
/// provide both.
#[test]
fn an_expired_deadline_is_a_timeout() -> Result<(), AgentError> {
    let token = CancellationToken::new();
    let mut registry = Registry::new(OwnerDecision::default());
    let agent = GeometryAgent::new(token)?.with_budgets(budgets_with(10, 8 << 10, 1))?;
    let id = agent.contract().meta.id.clone();
    registry.discover(Box::new(agent))?;
    registry.register(&id)?;
    registry.activate(&id, ContractTestReport::pass())?;

    // A profile with thousands of points: the extrusion cannot finish inside one millisecond.
    let mut profile = Vec::new();
    for index in 0..4000 {
        let angle = 2.0 * std::f64::consts::PI * (index as f64) / 4000.0;
        profile.push(json!([50.0 * angle.cos(), 50.0 * angle.sin()]));
    }

    let error = want_err(
        registry.run(
            &id,
            json!({"op": "extrude", "profile": profile, "height_mm": 2.0}),
        ),
        "a call over its wall-clock budget must not succeed",
    )?;
    assert!(
        matches!(error, AgentError::Timeout(_)),
        "expected Timeout, got {error}"
    );
    Ok(())
}

/// The output budget is the registry's to enforce: an agent cannot answer with more than the
/// contract allows.
#[test]
fn the_output_budget_is_enforced_by_the_registry() -> Result<(), AgentError> {
    let token = CancellationToken::new();
    let mut registry = Registry::new(OwnerDecision::default());
    let agent = GeometryAgent::new(token)?.with_budgets(budgets_with(10, 8, 30_000))?;
    let id = agent.contract().meta.id.clone();
    registry.discover(Box::new(agent))?;
    registry.register(&id)?;
    registry.activate(&id, ContractTestReport::pass())?;

    let error = want_err(
        registry.run(&id, json!({"op": "capabilities"})),
        "a reply over the output budget must be refused",
    )?;
    assert!(
        matches!(error, AgentError::BudgetExhausted(_)),
        "expected BudgetExhausted, got {error}"
    );
    Ok(())
}

/// The call budget is the registry's to enforce too.
#[test]
fn the_call_budget_is_enforced_by_the_registry() -> Result<(), AgentError> {
    let token = CancellationToken::new();
    let mut registry = Registry::new(OwnerDecision::default());
    let agent = GeometryAgent::new(token)?.with_budgets(budgets_with(2, 8 << 10, 30_000))?;
    let id = agent.contract().meta.id.clone();
    registry.discover(Box::new(agent))?;
    registry.register(&id)?;
    registry.activate(&id, ContractTestReport::pass())?;

    registry.run(&id, json!({"op": "capabilities"}))?;
    registry.run(&id, json!({"op": "capabilities"}))?;
    let error = want_err(
        registry.run(&id, json!({"op": "capabilities"})),
        "the third call must exceed the budget",
    )?;
    assert!(
        matches!(error, AgentError::BudgetExhausted(_)),
        "expected BudgetExhausted, got {error}"
    );
    Ok(())
}

/// A mid-flight cancellation: the host cancels while the agent is inside the kernel.
#[test]
fn a_cancellation_observed_mid_flight_stops_the_call() -> Result<(), AgentError> {
    let token = CancellationToken::new();
    let mut registry = Registry::new(OwnerDecision::default());
    let agent = GeometryAgent::new(token.clone())?;
    let id = agent.contract().meta.id.clone();

    // A profile with many points, so the tessellation is long enough to be interrupted.
    let mut profile = Vec::new();
    for index in 0..4000 {
        let angle = 2.0 * std::f64::consts::PI * (index as f64) / 4000.0;
        profile.push(json!([50.0 * angle.cos(), 50.0 * angle.sin()]));
    }

    // Build the solid first, without a deadline pressure, so the timing is about the
    // tessellation rather than the extrusion.
    registry.discover(Box::new(agent))?;
    registry.register(&id)?;
    registry.activate(&id, ContractTestReport::pass())?;
    let extruded = registry.run(
        &id,
        json!({"op": "extrude", "profile": profile, "height_mm": 2.0}),
    )?;
    let solid = extruded["solid"].as_u64().unwrap_or(0);

    // Now time a tessellation to find out how long the workload really takes.
    let started = Instant::now();
    let reply = registry.run(&id, json!({"op": "tessellate", "solid": solid}))?;
    let elapsed = started.elapsed();
    assert!(reply["triangles"].as_u64().unwrap_or(0) > 0);

    // The cancellation has to be observed *while* the kernel is working, so the workload must be
    // long enough to be interrupted. If it is not, the test is skipped rather than made flaky.
    if elapsed < Duration::from_millis(50) {
        eprintln!("skipped: the tessellation took only {elapsed:?}, too short to interrupt");
        return Ok(());
    }

    // A fresh agent and a fresh registry, so the tessellation starts cold.
    let token = CancellationToken::new();
    let mut registry = Registry::new(OwnerDecision::default());
    let agent = GeometryAgent::new(token.clone())?;
    let id = agent.contract().meta.id.clone();
    registry.discover(Box::new(agent))?;
    registry.register(&id)?;
    registry.activate(&id, ContractTestReport::pass())?;
    let extruded = registry.run(
        &id,
        json!({"op": "extrude", "profile": profile, "height_mm": 2.0}),
    )?;
    let solid = extruded["solid"].as_u64().unwrap_or(0);

    // Cancel from another thread while the tessellation runs.
    let canceller = token.clone();
    let delay = elapsed / 2;
    std::thread::spawn(move || {
        std::thread::sleep(delay);
        canceller.cancel();
    });

    let error = want_err(
        registry.run(&id, json!({"op": "tessellate", "solid": solid})),
        "a cancelled tessellation must not succeed",
    )?;
    assert!(
        matches!(error, AgentError::Cancelled),
        "expected Cancelled, got {error}"
    );

    // The agent survives: the registry is ready again and the next call works.
    assert_eq!(registry.registration(&id)?.lifecycle, Lifecycle::Ready);
    token.reset();
    let reply = registry.run(&id, json!({"op": "volume", "solid": solid}))?;
    assert!(reply["volume_mm3"].as_f64().unwrap_or(0.0) > 0.0);
    Ok(())
}

/// A reply the agent produced still satisfies the contract's output schema — checked by the
/// registry itself, on every call.
#[test]
fn every_reply_satisfies_the_declared_output_schema() -> Result<(), AgentError> {
    let (mut registry, id) = ready_registry(PermissionSet::new(), None)?;
    let replies: Vec<Value> = vec![
        registry.run(&id, json!({"op": "capabilities"}))?,
        registry.run(
            &id,
            json!({
                "op": "extrude",
                "profile": [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]],
                "height_mm": 5.0
            }),
        )?,
    ];
    for reply in &replies {
        assert_eq!(reply["ok"], json!(true));
        assert!(reply["op"].is_string());
    }
    // The registry would have rejected a reply that broke its own schema, so reaching this point
    // is the assertion. The explicit check below documents the shape for a reader.
    let schema = GeometryAgent::new(CancellationToken::new())?
        .contract()
        .output_schema
        .clone();
    for reply in &replies {
        cad_agent_runtime::validate_against(&schema, reply, "")
            .map_err(|error| AgentError::InvalidInput(error.to_string()))?;
    }
    Ok(())
}
