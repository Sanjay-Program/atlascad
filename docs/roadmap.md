# Roadmap

**Principle:** each milestone must be *verifiable on this hardware*, must not break either shipping
application, and must end with a test that fails if the feature regresses.

Sizes are rough effort for one engineer: **S** ≤ 1 day, **M** ≤ 1 week, **L** > 1 week.

---

## Done in the 2026-10-25 cycle

| Item | Result | Evidence |
|---|---|---|
| Shared binary-STL codec (`cad-io`) | **done, 6 tests** — exact facet layout, validating reader, round trip; the CLI and app no longer carry separate writers | `crates/cad-io`, ADR-0005 |
| `.zcad` schema 2: circle and regular-polygon sketches, persisted profiles, schema-1 migration | **done, 27 document tests** — tolerance-bounded tessellation, analytic area/volume checks, migration on load | `crates/cad-document`, `engineering/quality/tests.json#t-cad-document-schema2` |
| Atlas Studio (`cad-studio`): local HTTP application, WebGL viewport, feature tree, parameter editor, picking, command palette, themes, save and STL export | **done, 10 tests + browser checks** — full create/edit/recompute/undo/save/reopen/export workflow; loopback-only security envelope | ADR-0008, `crates/cad-studio`, `docs/feature-matrix.md` §3c |
| Application security envelope | **done, tested** — loopback bind, loopback `Host`, same-origin `Origin` on POST, JSON content type, 16 KiB headers, 1 MiB bodies, 5 s/10 s timeouts, 64 connections, CSP and no-store headers | `crates/cad-studio/src/{http,lib}.rs`, `crates/cad-studio/tests/http_api.rs` |
| App-and-API regression suite | **done, 7 HTTP integration tests + 2 unit tests** — parameter bounds, profile errors, foreign Origin, oversized body, malformed JSON, STL parse-back, undo/redo/save/reopen | `t-cad-studio` |
| Measured application baseline | **done** — startup, state/save/export round trips and the absence of a GPU frame claim are recorded | `docs/performance-baseline.md` §7 |
| `cad-engineering` report totals | **fixed, 2 new tests** — superseded package records and workspace summaries are no longer double-counted | `crates/cad-engineering/src/report.rs` |
| Control plane refreshed | **done** — 24 repositories, 34 modules, 11 interfaces, 32 capabilities, rebuilt after the new crates | `engineering/reports/latest-integration-report.md` |

## Done in the 2026-10-22 cycle

| Item | Result | Evidence |
|---|---|---|
| Persistent integration control plane (manifests + Rust scanner/validator) | **done, 30 tests (current)** | `docs/engineering-control-plane.md`, ADR-0004 |
| Idempotent change tracking and task generation | **done** — two scans left the backlog byte-identical | `engineering/state/scan-history.jsonl` |
| First real contract consumer (`cad-cli`, headless, binary STL) | **done, 8 tests** | `crates/cad-cli/tests/end_to_end.rs` |
| Tessellation manifold defect (per-face vertices + inward cap) | **fixed, regression test** | ADR-0005 |
| Pinned-toolchain scanner defect (rustup download hang) | **fixed, regression test** | `docs/engineering-control-plane.md` §6 |
| Cancellation and deadlines in the kernel adapter (part of **M4**) | **done** — every operation has a `_with_budget` twin; timeout, cancellation and "the instance survives" all tested | `crates/cad-kernel-truck/src/lib.rs`, ADR-0006 |
| **M2** — Analytic boolean correctness corpus | **done** — 10 fixtures, per-kernel pass/fail table, exit code gates regressions | `crates/cad-kernel-truck/examples/corpus.rs` |
| Agent platform: contract, runtime and the first agent | **done, 53 tests** — `cad-agent-api`, `cad-agent-runtime`, `cad-agent-geometry` | ADR-0007 |
| Local parametric project workflow | **done for schema 2** — circle/polygon sketches, extrude, parameter edit, undo/redo, atomic save/load, schema-1 migration and STL export | `crates/cad-document`, `crates/cad-cli`, `crates/cad-studio` |

The control plane is now the source of truth for what is integrated; read
`engineering/reports/latest-integration-report.md` before planning work.

**What the corpus found.** The M2 corpus is not a formality: it records two *known failures* that
were previously unknown — `union_tangent_contact` and `intersection_large_coordinates` — alongside
the five fixtures that pass and the three difference fixtures the kernel does not advertise. The
known failures are a baseline, not a skip: the corpus prints them and the gate stays green, so a
future fix shows up as an unexpected pass rather than silently changing the answer.

---

## Next milestone (do this first)

### M11 — Stage A: a real feature graph with multi-feature join (M)

**Goal:** a project can contain more than one sketch and more than one extrusion, joined in a stated
order, and a parameter or origin edit rebuilds every dependent feature.

Why now: the application currently proves one feature. A CAD product's defining property is an
*editable history*, and the document model is the thing that is expensive to change later. Join is
the one boolean the kernel actually passes its corpus for; cut stays out until difference is fixed.

Steps:
1. Generalize `Project` from one sketch and one feature to ordered sketches and features with stable
   IDs and explicit dependency references; migrate `.zcad` schema 2.
2. Evaluate features in order; a feature input may only reference an earlier sketch or feature.
   Refuse cycles and forward references in `validate`, not at render time.
3. Implement extrude-join through `Kernel::boolean(Union)`. Expose it in the API and the feature tree;
   keep cut/revolve/loft visible as unavailable rather than fake.
4. On failure, keep the last valid evaluated mesh, mark the failing feature, and return its ID and
   reason. Undo restores history and geometry together.
5. Test analytic two-feature volumes, dependency refusal, failure propagation, parameter propagation,
   save/reopen, undo/redo, and schema-2 migration.

**Acceptance:** create two sketches, extrude both, join them; edit a shared parameter and confirm both
features rebuild; save, reopen and reproduce the same volume; an invalid feature leaves the prior
result intact and names the failing feature. `cargo test --offline --workspace` passes and the app
workflow is exercised in the browser.

**Out of scope:** cut (kernel difference is a recorded failure), revolve, loft, fillet, constraints.

---

## Then, in order

### M1 — Prove the contract against the real kernel (M) — needs a larger host

**Goal:** `solidcraft-kernel` implements `cad_contract::kernel::Kernel` behind a feature flag, and a
test asserts that the contract path and the existing kernel agree on volume for the same fixtures.

Why: it is additive, changes no behaviour, and converts "we have a contract" into "the contract
describes the kernel we ship". It also immediately exposes which contract methods the mature kernel can
implement better than the experimental adapter (difference, chains, fillets). It is not next because
its acceptance run needs a ≥16 GB RAM machine and the root application's history model has higher
user value per hour.

Steps:
1. Add `cad-contract` as an optional dependency of `apps/solidcraft/crates/kernel`, feature
   `cad-contract`.
2. Implement `Kernel` by delegating to the existing `Body` API; map `KernelError` → contract
   `KernelError` (the four categories already match).
3. Add a test that builds a box both ways and compares `volume()` to the existing kernel's own answer.
4. Do **not** change any existing public signature; the flag must be off by default.

**Acceptance:** `cargo test -p solidcraft-kernel --features cad-contract` passes on a machine with
≥16 GB RAM; no existing test changes.

### M2 — Correctness corpus for booleans (M) — highest engineering value — **done**

Build a fixture set the adapter and the real kernel both run: coplanar cut, piercing cut, contained
void, tangent contact, near-degenerate sliver, thin wall, large coordinates (1e6 mm), and a
non-axis-aligned box. Assert analytic volume/area for each, and record pass/fail per kernel.

**Delivered for the truck adapter.** The corpus runs ten fixtures with analytic volumes and areas
and a stated tolerance, and prints a pass/fail table whose exit code gates regressions. The real
kernel (`solidcraft-kernel`) has **not** been run against it yet — that needs the ≥16 GB RAM host,
so the "two kernels on one corpus" comparison the definition of done asks for is still open.

Why: the difference defect and the chain panic were found by accident. A corpus turns them into a
regression gate, and gives an honest answer to "which kernel is more reliable" that
`docs/kernel-comparison.md` currently cannot give.

### M3 — Wire units and IDs into CADCraft (M)

Replace the implicit `INSUNITS` handling and `Handle` with `cad-contract::Unit`/`EntityId`
(`EntityId` is already the same `u64`, so this is mostly a type alias plus conversions). Add round-trip
tests for DXF files with inch and millimetre units.

Why: it is the cheapest real adoption, and unit bugs are the most expensive class in CAD.

### M4 — Import budgets and cancellation (M) — security — **half done**

Add file-size, entity-count and 3MF zip-entry/uncompressed-size limits to SolidCraft's `crates/io`, and
a cancellable import.

**Done:** the adapter half. Every kernel operation now takes a `Budget` — a deadline plus a
cancellation token — and reports `Timeout`/`Cancelled` instead of hanging. What is left is the
import half: SolidCraft's IO crates still have no file-size or entity-count limits, which is
security finding **S-3** and the highest-severity item still open.

Why: security review S-3; the error variants already exist but nothing sets them.

### M5 — Dependency governance in CI (S)

`cargo-deny` with a `deny.toml` in both apps; a CI guard that fails if `opencascade*`/`cadrum` enter a
shipping graph; record the `craft-libs` licence and vendor the pinned commits.

Why: closes S-1, S-2 and S-5, the three findings that can block a commercial release.

### M6 — Benchmark the things that actually dominate a CAD session (M)

The current baseline measures geometry primitives. It does not measure what users feel: document load,
selection/snap on a large drawing, undo snapshot cost, constraint solve, parametric rebuild, STEP/DXF
round trip. Build those fixtures (10k–100k entities) and publish the table.

Why: without this, any later "optimisation" is unmoored.

### M7 — Persistent project format expansion (L) — **schema 2 shipped; schema 3 is M11**

`.zcad` schema 2 stores stable IDs, parameters, a rectangle/circle/polygon sketch, one extrude, and
bounded undo history, with schema-1 migration and verified atomic save/load. It still lacks a feature
graph, constraints, named revisions and recovery snapshots. M11 generalizes the payload to an ordered
feature list; this milestone continues afterward with named revisions, recovery snapshots and
additional sketch entities. Continue to refuse unknown fields and newer schemas rather than silently
discard data.

Why: collaboration, versioning and SaaS all depend on this, and it must not be invented twice.

### M8 — WebAssembly feasibility spike (M, time-boxed)

Install `wasm32-unknown-unknown` + `wasm-pack`, build `cad-contract` (no I/O, no threads) and measure
size/startup. Then try the truck adapter alone. Report numbers; do not promise a browser app.

Why: the brief asks, and the honest answer requires a measurement that has not been taken.

### M9 — Give the agent platform a second agent and a host (M)

The runtime and one agent exist. Two things would prove the platform is a platform rather than a
worked example:

1. **A second agent with a different permission profile.** `agent.cad.import-export` is the obvious
   candidate: it would need `read_workspace`, and its contract would exercise a *different* schema
   shape and a different permission set. Two agents with different profiles is the minimum evidence
   that the contract is not accidentally geometry-specific.
2. **A host that is not the test harness.** The registry today is driven by tests. It needs a small
   CLI (`atlas agent run <id> <input.json>`) so an agent can be exercised from a shell and from
   CI without writing Rust.

Why: an agent platform with one agent and no host is a library, not a platform. The brief asks for
agents that can be registered, tested, replaced and removed without redesigning the application.

### M10 — Replace the in-process registry with a scheduled one (L)

The registry is synchronous and single-owner: a long operation blocks the calling thread. A job
queue with bounded concurrency, cancellation across process boundaries and a persistent registry
would let the desktop and web clients share the same agents.

Why: defer until a second host exists (M9) — scheduling a single in-process agent is not worth the
complexity, and the brief is explicit that a distributed backend should not be built before the
product needs it.

---

## Explicitly not now (and why)

| Item | Why not | Revisit when |
|---|---|---|
| Collaboration (Yrs/Yjs) | Requires a broader persistent document model (M7) and semantic conflict recovery; the first local project slice is single-user | After M7 expansion |
| SaaS backend (Axum/Postgres/billing) | The engine must work without cloud; billing before persistence is backwards | After M7 + a real multi-user need |
| Second geometry kernel | No candidate has demonstrated an advantage; every extra kernel multiplies test surface | A corpus (M2) shows truck loses on a feature the product needs |
| DWG native work | Currently `acadrust` (MPL-2.0) used unmodified; no corpus to judge compatibility | A legally obtained DWG test set exists |
| Replacing egui with a custom renderer | No measured product benefit; both apps already use egui 0.36 + wgpu 30 | A rendering benchmark shows a user-visible problem |
| Merging CADCraft and SolidCraft into one crate tree | They have incompatible document models and different release cadences; the contract is the integration point | A shared shell is actually required by a product decision |

---

## Decision gates and current owner decisions

| Gate | Options | Consequence |
|---|---|---|
| **G1 — LGPL/OCCT** | (a) exclude OpenCASCADE entirely; (b) dynamic-link OCCT with relink rights; (c) relicense the product | Blocks any use of `opencascade-rs`/`cadrum`. Default today: **exclude**. |
| **G2 — Licence of the combined product** | Current root platform license is personal, non-commercial; imported components retain their own licenses | Commercial use of AtlasCAD root crates requires prior written permission. Imported application licenses do not change that requirement. |
| **G3 — Distribution model** | Personal/non-commercial distribution under the current root license; commercial service or product only after written permission | Determines whether the SaaS work (M7+) is permitted and on the critical path. |
| **G4 — Performance target** | e.g. "100k-entity drawing pans at 60 fps on integrated graphics" | Without a target, rendering work cannot be prioritised or judged. |

---

## Definition of done for the *next* report

1. M11 acceptance met: multi-feature join, dependency refusal, failure propagation, schema-2
   migration, and a browser workflow that survives save/reopen.
2. The new feature graph's load/recompute cost is measured on the recorded hardware, with correctness
   checked in the same run.
3. M5 CI gates green, or the missing tools reported as blockers rather than claimed.
4. M1 merged with the flag off by default and a passing agreement test on a ≥16 GB host.
5. Every remaining "Blocked" row in `docs/feature-matrix.md` either has an owner decision or a date.
6. The S-3 import budgets are closed in SolidCraft IO, so `M4` can be marked done rather than half
   done.
