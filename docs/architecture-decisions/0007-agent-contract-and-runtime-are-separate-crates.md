# ADR-0007: The agent contract is a separate crate from the runtime, and an agent never depends on the runtime

- **Status:** Accepted
- **Date:** 2026-10-22
- **Deciders:** engineering (this cycle)
- **Evidence:** `crates/cad-agent-api/src/contract.rs`, `crates/cad-agent-runtime/src/registry.rs`,
  `crates/cad-agent-geometry/src/lib.rs`, `crates/cad-agent-geometry/tests/integration.rs`,
  `engineering/quality/tests.json#t-cad-agent-api`, `#t-cad-agent-runtime`, `#t-cad-agent-geometry`

## Context

The brief asks for a pluggable agent runtime: agents can be registered, tested, replaced and removed
without redesigning the application, and a newly added agent must not be automatically trusted.

Two structural questions had to be answered before writing any of it:

1. Where does the agreement between an agent and its host live?
2. In which direction does the dependency run between the two?

The obvious layout — one crate holding the trait, the registry and the enforcement — makes every
agent depend on the registry, because the trait's `AgentContext` carries the runtime's types. That
looks harmless and is not: an agent that imports the runtime cannot be run by a different host, and
the runtime can never be replaced or forked without touching every agent.

A second, quieter problem: an agent that is *capable* of doing something is not the same as an agent
that is *allowed* to do it. If the trait and the enforcement live together, the enforcement tends to
be advisory, and "the agent asked for it" quietly becomes "the agent may".

## Options considered

| Option | Assessment |
|---|---|
| One crate: trait, registry, enforcement, agents | Every agent drags in the registry; the runtime cannot be replaced; the enforcement and the capability live in the same place, which invites the advisory failure mode. |
| Trait in a contract crate, runtime in a host crate, agents depend on the contract only | Chosen. The dependency runs agent → contract and host → runtime, so the runtime is replaceable and an agent is host-agnostic. |
| Agents as external processes speaking JSON-RPC | The strongest isolation, and the right answer for third-party native code. It also costs a process boundary, a transport and a schema in two languages before a single agent exists. |

The third option is not rejected — it is the answer for *third-party* agents, and it is recorded in
the manifest as the isolation strategy for code the project does not own. It is simply premature for
the first-party agent, where the process boundary would buy isolation from a bug the project can fix
by fixing the bug.

## Decision

Three crates, and the dependency graph follows the vocabulary:

```
cad-agent-api        the contract: Agent, AgentContract, AgentContext, Budgets,
                     Permission, PermissionSet, OwnerDecision, AgentError
      ▲                    ▲
      │                    └── cad-agent-geometry  (the agent; depends on the contract
      │                        and on the code it computes with, never on the runtime)
      └── cad-agent-runtime    (the host: registry, lifecycle, grants, budgets, schema check)
```

* **`cad-agent-api`** holds only the agreement. It depends on serde and thiserror and nothing else.
  It contains no registry, no scheduler and no enforcement, so an agent implementation never drags
  in the machinery that will host it.
* **`cad-agent-runtime`** enforces the contract. It knows the contract and knows nothing about any
  particular agent. `cad-agent-runtime` is a **dev-dependency** of `cad-agent-geometry` and
  nothing else: the edge in the graph is the test harness, not the agent.
* **`cad-agent-geometry`** is the first agent. It depends on the contract, on `cad-contract` and on
  `cad-kernel-truck`, and its `Cargo.toml` says so.

### The capability/authority split

The contract carries two different things that are easy to conflate:

* the **declaration** — what the agent says it needs;
* the **decision** — what the owner allows.

The effective grant is the *intersection*, computed by the runtime, and it is the only thing the
agent ever sees. `shell` and `secrets` are owner-gated: an agent that declares them cannot be
activated until an owner has decided, and the registry refuses to activate it otherwise. A
`read_workspace` grant is ordinary configuration and may be absent — the agent then still activates
and still answers every operation that needs no capability, while the one operation that does is
refused with `PermissionDenied`.

The geometry agent exercises exactly this: it declares `read_workspace`, uses it for exactly one
operation, and works under every other grant.

### Trust is a sequence, not a flag

An agent reaches `ready` only after it has been discovered, its contract validated, its schemas
checked for keywords the runtime can actually enforce, its permissions checked against the owner's
decision, and a contract-test report supplied. An agent whose contract tests failed stays
`registered` and refuses to run; a panicking agent is quarantined rather than taking the host down.

## Consequences

* **Good:** the runtime can be replaced — by a scheduler, by a different process boundary, by a
  WASM host — without touching an agent. The agent's contract is the whole interface.
* **Good:** the capability model is testable end to end, and it is tested end to end. The
  integration suite asserts that the same call succeeds or fails depending only on the grant.
* **Good:** a third-party agent can be isolated later without changing the first-party ones, because
  isolation is a property of the *host*, not of the contract.
* **Bad:** three crates and a `dev-dependency` edge is more ceremony than one crate. It is worth it
  only because the alternative is a platform whose agents cannot be moved.
* **Bad:** the runtime is synchronous and single-owner. A long operation blocks the calling thread.
  This is stated in the manifest's known-limitations field and in `docs/architecture.md` §6 rather
  than left to be discovered.

## Alternatives kept open

* **Process isolation for third-party agents.** The right answer for code the project does not own;
  deferred until a second-party agent actually exists. The manifest already names it as the
  isolation strategy for such code.
* **A scheduled registry with bounded concurrency.** Deferred: scheduling one in-process agent buys
  nothing, and the brief is explicit that a distributed backend should follow a demonstrated need.
* **A version negotiation handshake.** The contract version is a single integer today and a
  mismatched version is refused up front. A handshake would be needed if agents were loaded from
  separate processes at different times.
