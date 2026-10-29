# Architecture

**Status:** the contract, the truck adapter, the engineering control plane, a headless consumer
(`cad-cli`), the project payload (`cad-document`), the shared STL codec (`cad-io`), the local
application (`cad-studio`) and the agent platform (`cad-agent-api`, `cad-agent-runtime`,
`cad-agent-geometry`) are **implemented, compiling and tested** (273 tests across eleven root
workspace packages, verified 2026-10-29; the project payload carries an ordered feature graph that
includes extrude, cut and full-turn revolve, ADR-0009). The two imported desktop applications (CADCraft and
SolidCraft) are **not yet wired to the contract**; the root workspace ships its own local
application instead, described in §5d.

---

## 1. What exists today (measured, not proposed)

Two applications, both Rust, both `MIT OR Apache-2.0`, both already shipping:

```
apps/cadcraft    2D drafting           12 crates, 185 000 Rust LOC, 2 212 tests
                 DXF (R12–2018 read, R2000 write), DWG via acadrust (MPL-2.0),
                 constraints, layouts/plotting, MCP server, CLI
                 geometry: cadcraft-geom (Vec2/Vec3/Line/Arc/Circle/Spline, EPS = 1e-9)
                 document: cadcraft-doc (Handle(u64), copy-on-write EntityStore)

apps/solidcraft  parametric 3D        13 crates, 1 148 000 Rust LOC, 5 884 tests (57 ignored)
                 sketch + constraint solver, feature timeline, assemblies, sheet metal,
                 STEP/IGES/3MF/STL/OBJ/glTF, desktop + web + CLI + MCP
                 geometry: solidcraft-kernel (94 803 LOC over truck 0.6, no truck types in its API)
                 document: solidcraft-doc (parametric timeline, incremental evaluation)
```

These are not projects to be replaced. The architectural problem is that **they share nothing**:
two vector types, two document models, two undo strategies, two unit conventions (`INSUNITS` vs
unit-aware expressions), and only one of them has a 3D kernel.

## 2. The decision that shapes everything

The brief proposes a `crates/cad-*` tree with a `cad-kernel-api` / `cad-kernel-truck` split. **That
split already exists inside SolidCraft** — `apps/solidcraft/crates/kernel` is exactly a
kernel-agnostic boundary over truck, with a documented promise that *"no truck type appears in the
public API, so the kernel can be replaced without touching the rest of the workspace"* and a panic
guard that converts third-party panics into `KernelError::Internal`.

Creating a second, competing abstraction over the same kernel would be the worst outcome: two
vocabularies, two adapters, and a 94 803 LOC implementation that nobody can reuse. So this milestone
does the opposite of a rewrite:

> **Extract the smallest stable *contract* both applications can speak, prove it against the real
> kernel, and let adoption happen incrementally.**

## 3. Implemented: `crates/cad-contract`

A dependency-light crate (thiserror + serde + serde_json) that owns **vocabulary, not geometry**:

| Module | Contents | Why it exists |
|---|---|---|
| `units` | `Unit` (mm, cm, m, in, ft), exact conversion, DXF `INSUNITS` mapping both ways, parsing | CADCraft stores units in a header variable; SolidCraft in expressions. Lengths crossing a boundary are the easiest bug to ship. |
| `ids` | `EntityId` (non-zero validated, hex + integer forms, `Ord`), `DocumentId` | A project file must store application IDs, never kernel pointers. CADCraft's `Handle(u64)` is the model; this adds validation. |
| `tol` | `Tolerance { linear_mm, angular_rad }`, validated | A `NaN` or zero tolerance reaching a kernel produces wrong geometry instead of an error. Rejects values that indicate a unit mistake. |
| `capability` | `Capability` (19 variants), `CapabilitySet`, `KernelInfo` | Callers must be able to ask what a kernel can do *before* offering a command. Capabilities are only listed when a test exercises them. |
| `error` | `KernelError` (Unsupported / Invalid / Failed / TooLarge / Internal / UnknownSolid / Cancelled / Timeout), `ErrorKind`, `is_retryable()` | One taxonomy for every adapter, matching SolidCraft's shipped four categories and adding the two that interruption and capability-gating need. |
| `mesh` | `Mesh` (plain `f64`/`u32` arrays), `Aabb3`, `validate()`, `open_edges()`, `is_closed()`, `signed_volume()`, `surface_area()` | The only bulk geometry type that crosses the boundary. Plain arrays so a renderer, CLI and kernel never depend on each other's vector types. |
| `kernel` | `Kernel` trait: `info`, `extrude`, `boolean`, `tessellate`, `release` + provided `volume`, `bounding_box`, `require`, `supports`; `SolidId` (session handle); `validate_profile` | Deliberately small. Fillet/sweep/sheet-metal stay behind capabilities and adapter APIs rather than being flattened into a trait that cannot express them. |
| `document` | `DocumentHeader` (schema, id, units, tolerance), `CURRENT_SCHEMA = 2`, migrations v0→v1→v2 with a log, `Revision` | Versioned format with tested migrations. A **newer** schema is an error, not a silent downgrade. |

A first real consumer exists: `crates/cad-cli` turns a unit-aware box/union request into a
`DocumentHeader`, kernel geometry, a validated mesh and a binary STL. It found the tessellation
defect recorded in ADR-0005.

Deliberate non-goals for the contract crate: no `Vec3` of its own (that would be a third vector
type), no document payload, no rendering, no file I/O. The payload lives in `cad-document`
(§5b), which uses the contract's `DocumentHeader`, `EntityId`, `Unit`, tolerance and `Kernel`
trait.

**Safety properties enforced by the crate itself:** `unsafe_code = "forbid"` at workspace level,
`deny(clippy::unwrap_used, expect_used, panic)`, and tests that *return `Result`* so a failed kernel
call fails the test.

## 4. Implemented: `crates/cad-kernel-truck`

A truck-backed implementation of the contract — the proof that the contract describes a real kernel.

* **Advertises only what is tested:** `Extrude`, `BooleanUnion`, `BooleanIntersection`, `Tessellate`.
  Not fillets, not sweeps, not STEP, not WASM, and **not boolean difference** (§6).
* **Deep copies operands** with `builder::clone`, because truck's boolean rewires the topology it is
  given; a shallow `clone()` corrupts the caller's solid.
* **Panic guard:** every call into truck runs under `catch_unwind`, converted to
  `KernelError::Internal`. Verified: a genuine panic inside truck's intersection-curve code was
  caught and reported, not propagated.
* **Coplanar-face retry:** when a boolean returns no result, the tool is moved by
  `[7e-5, 9e-5, 1.1e-4]` mm and the result is moved back. The retry count is observable
  (`coplanar_retries()`), and the benchmark reports that **100 of 100** unions needed it.
* **Volume-conservation guard:** a union may not shrink and an intersection may not grow beyond a
  relative slack; a result that violates this is refused with an error instead of returned.
* **Bounded input:** profiles are validated before they reach truck (≥3 points, finite, no repeated
  consecutive point, non-zero area); tessellation tolerance is validated against the kernel minimum.
* **Cancellation and deadlines:** every operation has a `_with_budget` twin that takes a
  [`Budget`](crates/cad-kernel-truck/src/lib.rs) — a deadline plus a cancellation token — and checks
  it before and after the work and at each internal checkpoint. The trait methods keep their plain
  signatures and report `Timeout`/`Cancelled` rather than hanging. Cancellation is cooperative, not
  preemption: a single pathological boolean inside truck cannot be interrupted, which is stated in
  the module rather than implied away.
* **Correctness corpus:** `crates/cad-kernel-truck/examples/corpus.rs` runs ten analytic fixtures —
  box control, non-axis-aligned box, partial-overlap union, thin-wall intersection, near-degenerate
  sliver, tangent contact, large coordinates, and the three difference cases — and prints a
  pass/fail table. The gate is the exit code: a previously passing fixture that regresses makes the
  binary exit non-zero. Two fixtures are recorded **known failures** (tangent contact, coordinates
  near 1e6 mm) and the three difference fixtures are recorded **unsupported by the kernel**, which
  the corpus counts as correct rather than as a silent skip.

Measured behaviour of this adapter is in `docs/performance-baseline.md`; what the underlying kernel
can and cannot do is in `docs/kernel-comparison.md`.

## 5. How the two applications adopt this without being rewritten

The contract is additive. Nothing in CADCraft or SolidCraft needs to change for this milestone to be
correct, and neither repository was modified.

| Step | CADCraft | SolidCraft | Risk |
|---|---|---|---|
| A1 | Use `Unit`/`INSUNITS` mapping in `crates/doc` and `crates/dxf` | Use `Unit` in `display_units`/`expr` | Low: conversions only, existing tests cover round trips |
| A2 | Replace `Handle` internals with `EntityId` (or add `From`) | Add `EntityId` to `naming`/`names` | Low: `Handle` is already `u64` |
| A3 | Use `Tolerance` in `cadcraft-geom` instead of a global `EPS` | Use `Tolerance` in the kernel-facing calls | Medium: touches many call sites; do per crate, behind a type alias |
| B1 | Advertise 2D capabilities through `KernelInfo` (a 2D "kernel" implementation) | Implement `cad_contract::kernel::Kernel` for the existing kernel crate behind a feature flag | Medium |
| B2 | — | Wire `DocumentHeader` into `.solidcraft` save/load; keep the existing payload | Medium: must not break existing files |
| C1 | Share `Mesh` for viewport upload | Share `Mesh` for viewport upload | Low |

**The smallest verified next step** is B1 for SolidCraft: implement the `Kernel` trait for
`solidcraft-kernel` behind a `cad-contract` feature, mapping its `Body`/`KernelError` to the contract
types. It changes no behaviour, is additive, and a test can assert that the adapter agrees with the
existing kernel on volume for the same fixtures. That is the milestone that turns a contract into an
integration.

## 5b. Defined and tested: the project payload and its consumers

* **`cad-document`** is the versioned `.zcad` project payload: stable document and entity IDs, named
  millimetre parameters, an **ordered feature graph** of rectangle/circle/regular-polygon sketches
  and extrude features (`new_body` or `join`), bounded undo/redo (64 snapshots), validating load,
  atomic save, and schema 1 → 2 → 3 migration. It depends on `cad-contract` for `DocumentHeader`,
  `EntityId`, `Unit`, tolerance and the kernel trait, and on no adapter. `Project::build` walks the
  history in order, feeds the running solid into `Kernel::boolean(Union)` for a join, and returns a
  per-feature report plus the last good mesh rather than an error when a feature fails. Its tests
  cover analytic circle, polygon and two-feature union volumes, dependency refusal, failure
  propagation, migration, the 64 MiB read limit, corrupt input and save/reopen equivalence. The
  graph is **linear and single-body**; see §6 and ADR-0009.
* **`cad-io`** is the shared binary-STL codec used by both the CLI and the application: a 50-byte
  facet layout, exact declared-count handling before allocation, a validating reader and a closed,
  outward mesh check before writing. STL carries no unit field, so a document header must accompany
  it when units matter.
* **`cad-cli`** is the headless consumer: extrude/union → tessellate → binary STL, plus create/edit/
  save/reopen/undo/redo for a `.zcad` project. Its tests assert analytic volumes, inch conversion, a
  watertight mesh, a valid STL round trip and a multi-process project workflow.
* **`cad-engineering`** is the control plane described in `docs/engineering-control-plane.md`: typed
  manifests, cross-validation, workspace scanning, change detection, idempotent task generation and
  report rendering. It writes only `engineering/state/`, `engineering/reports/` and — when a task is
  genuinely new — the backlog. It is not allowed to rewrite a curated claim.

The tessellation path now welds coincident vertices and makes the winding consistently outward
(ADR-0005). Before that repair, a box tessellation had 24 unwelded vertices, 8 open edges after
welding, and one inward-wound cap whose error the signed volume could not reveal.

## 5c. Implemented: the agent platform

Three crates, layered so that the dependency only ever points one way:

| Crate | Responsibility |
|---|---|
| `cad-agent-api` | The versioned contract: agent identity and metadata, declared capabilities and permissions, JSON Schemas for input and output, budgets, and one structured error taxonomy. Depends on serde alone. |
| `cad-agent-runtime` | The host: contract validation, the `discovered → registered → ready → running` lifecycle, capability grants, budget enforcement, and a strict JSON Schema subset checker. |
| `cad-agent-geometry` | The first real agent: extrude, boolean, tessellate and measure solids through the contract kernel. |

**The contract is the agreement; the runtime is the enforcement; the agent is the work.** An agent
depends on the contract and on whatever it computes with, never on the runtime — so any host can run
it, and the host can be replaced without touching the agent. `cad-agent-geometry` depends on
`cad-agent-api`, `cad-contract` and `cad-kernel-truck`; `cad-agent-runtime` is a *dev*-dependency
only, used by the tests that drive the agent through a real registry.

What the runtime actually guarantees, and what the integration tests pin:

* **A new agent is not trusted.** It is discovered, registered, and only becomes `ready` with a
  passing contract-test report. An agent whose contract tests failed stays `registered` and refuses
  to run.
* **Capabilities are intersected, never widened.** The effective grant is the contract's declaration
  ∩ the owner's decision. `shell` and `secrets` are owner-gated: an agent that declares them cannot
  be activated until an owner has decided, and the registry refuses to activate it otherwise.
* **Budgets are enforced from outside.** The call count, the output size and the wall-clock limit
  live in the contract, and the runtime checks them around the call — the agent cannot widen its own
  budget, because `AgentContext` has no setters.
* **Schemas are enforced before the agent runs.** A hostile input is refused by the runtime, so an
  agent's own validation is a second line of defence rather than the only one. The checker implements
  a documented keyword subset (`type`, `enum`, `required`, `properties`, `items`, `minItems`,
  `maxItems`, `minLength`, `maxLength`, `minimum`, `maximum`) and **refuses any keyword it does not
  implement**, so a contract cannot promise more than the runtime checks.
* **A misbehaving agent is contained.** A panic inside an agent is caught and the agent is
  quarantined rather than taking the host down.

The geometry agent is deliberately modest. It asks for exactly one capability,
`read_workspace`, and uses it for exactly one operation: reading a JSON profile file from a
confined workspace. The name must be a single path component — separators, `..` and absolute paths
are refused before any filesystem call — and the resolved file must still be inside the workspace
after symlinks are followed. Every other operation is pure computation over the agent's own kernel.
Its replies carry **measurements, never geometry**, so the output budget does not depend on the size
of the solid.

The cancellation token belongs to the caller: it is handed to the agent, cloned into every call's
budget, and shared with the caller, so cancelling it stops an in-flight kernel operation at its next
checkpoint and the call returns `Cancelled`. Because the token is shared, the caller resets it before
the next call — the same rule the kernel adapter documents for a UI that cancels one command and
then runs the next.

What this does **not** do, and what it therefore does not claim: no document, no history, no
parametric features, no file write, no network, no shell, no persistence of solid handles (they are
session handles), and no third agent. The three planned agents that are still unimplemented —
import/export, design-rule checks and natural-language-to-CAD — are recorded as such in
`engineering/manifest/agents.json`, with their contracts and permissions described but
`implemented: false`.

## 5d. Shipped: Atlas Studio (`cad-studio`)

Atlas Studio is the root workspace's own application, not a wire-up of either imported desktop app.
ADR-0008 records why it is a local HTTP shell over the existing Rust document and kernel rather than
a new egui tree.

```text
browser (assets/index.html, app.css, app.js)
  │  loopback HTTP/1.1, JSON, same-origin checks
  ▼
cad-studio  ──  cad-document (parameters, feature graph, undo, .zcad)
  │                 │
  │                 └── cad-contract (header, ids, units, kernel trait)
  ├── cad-kernel-truck  (extrude + union + tessellate)
  └── cad-io            (validated binary STL)
```

* **Transport and trust boundary.** A `std::net::TcpListener` on `127.0.0.1` by default. Every
  request's `Host` must be the studio's own loopback host and port, and every `POST` must carry a
  same-origin `Origin` and `application/json`; foreign or empty hosts are refused before routing
  (tested). Header limit 16 KiB, body limit 1 MiB, read timeout 5 s, write timeout 10 s, at most 64
  concurrent connections. Responses send `no-store`, `nosniff`, `no-referrer`, `X-Frame-Options: DENY`
  and a `default-src 'self'` CSP.
  **Sharing is opt-in and token-gated:** `--share`, or any non-loopback `--host`, binds the LAN
  interface and always requires a 128-bit session token on every `/api/*` request (header or query);
  the static page is served without one so the browser can present it. The `Host` check then accepts
  any host naming the studio's port, because the address a colleague dialled is not known in advance,
  and the token is what authorises the call. There is no account layer, and the link grants full edit
  access until the process stops. The port must not be exposed to the internet.
* **Document mutation is serialized.** The server owns one `Project`, one kernel instance and one
  `BuildCache` behind a mutex; every mutating route goes through the same path that snapshots undo
  history, re-evaluates through the kernel and returns the complete render state. A rejected
  parameter or profile leaves the project unchanged. Because the kernel outlives a request, cached
  solids stay valid across requests; the cache is keyed by the resolved inputs of each feature prefix,
  so an edit invalidates exactly the features that read it.
* **UI.** One HTML document, CSS custom properties for light/dark, and a dependency-free module that
  parses the mesh into WebGL 1 buffers, ray-casts faces for selection, and exposes the ordered model
  tree (one row per sketch and feature, with operation, depth and rebuild status), a sketch-placement
  panel, a parameter panel, the status bar, the command palette and keyboard help. A feature that
  cannot rebuild is marked in the tree with the kernel's message, shown in a banner, and disables
  export rather than letting the file drift from the history. The narrow layout turns both side
  panels into drawers instead of hiding them. The browser is a renderer and input surface; all
  document and geometry logic stays in Rust.
* **Persistence.** Save is explicit (`Ctrl/Cmd+S` or the palette) and goes through
  `Project::save`, which writes a sibling temporary file, syncs it and renames atomically. Autosave,
  crash recovery and a native file dialog are **not** implemented.

## 6. Known structural limits (not hidden)

1. **The adapter is not the production kernel.** SolidCraft's kernel has 94 803 LOC of fillets,
   chamfers, healing, sheet metal, STEP/IGES, boolean retries and history. The contract adapter has
   extrude, union, intersection and tessellation. Difference and chains do not work (§
   `docs/kernel-comparison.md`), which is precisely why the platform should reuse the mature kernel
   rather than build a new one.
2. **Three application surfaces now exist.** CADCraft and SolidCraft still have separate `ui-egui`
   trees (egui 0.36 + wgpu 30); Atlas Studio is a browser surface over a Rust server. There is no
   shared component library between them and no measured reason to merge them yet.
3. **No cloud service, collaboration, or WASM target.** Atlas Studio listens on loopback by default;
   `--share` exposes one session on the local network behind a session token. There is no account,
   sync, hosted storage, or concurrent multi-user editing (a share is one document, one history, and
   whichever client last wrote wins). `wasm32-unknown-unknown` is not
   installed on this machine and no WASM build was attempted; neither is claimed.
4. **The feature graph is linear and single-body.** `.zcad` schema 7 persists a header, named
   parameters, an ordered list of sketches and features with stable IDs, where the first feature
   starts the body and every later one is a join (union into the body) or a cut (difference out of
   the body) through the kernel's booleans. A feature is an extrude along +Z by a parameter, or a
   full-turn revolve of the sketch about one of its own axes; a partial revolve is refused because it
   would be an open shell, and a profile that crosses the axis is refused because it has no solid of
   revolution. A sketch profile is a rectangle, circle, regular polygon, drawn polyline or arc sector;
   there is only one closed profile per sketch, not an entity graph. There is no sweep, loft, fillet,
   pattern, arbitrary-axis revolve, multi-body,
   constraint solver, assembly or drawing. A per-feature
   `BuildCache` keeps the body after each function prefix, so an unchanged request does no kernel work
   and an edit re-runs only the features that read it; it is session state, never persisted, and
   `Project::build` remains the uncached path. Up to 32 **named revisions** are persisted with the
   project as full copies of the editable history; they are restore points, not a diff or a branch.
   Alongside the file, `<name>.zcad.recovery` holds the last unsaved edit and `<name>.zcad.backup`
   holds the previous good save, and neither is read unless the project itself cannot be opened. The
   header and payload schema are versioned
   independently, and schema 1, 2, 3 and 4 files migrate on load without being rewritten on disk. An
   unsaved edit is snapshotted to a separate `<project>.recovery` file after each change, so a crash
   loses no work and a corrupt project file can still be recovered; the snapshot never writes the
   project, and saving or discarding removes it.
5. **The agent runtime is in-process and single-owner.** The registry is a plain struct with `&mut
   self` methods, so one host owns it at a time, calls are synchronous, and a long operation blocks
   the calling thread. There is no scheduler, no queue, no process isolation for third-party agents,
   and no persistence of agent registrations across restarts.
6. **Only one agent exists.** The runtime's guarantees are proven against one agent and the
   registry's own test doubles. The other three planned agents are declared but not implemented.
7. **The application is not a full CAD product.** It has no constraints, dimensions, arcs/splines,
   sweep, loft, partial revolve, fillet, chamfer, shell, patterns, multi-body, assemblies, drawings,
   STEP/IGES/DXF import, autosave, or crash recovery. The verified workflow is exactly what §5d and
   `docs/feature-matrix.md` §3c list, and nothing more.

## 7. Crate dependency rules (to keep the graph acyclic)

```
cad-contract        (no cad dependencies; thiserror + serde)
   ▲        ▲                ▲
   │        │                └── cad-document (payload; no adapter)
   │        │                        ▲
   │        └── cad-kernel-truck      │
   │            (truck-modeling/topology/shapeops/meshalgo)
   │                                 │
   └── cad-io (STL only)              │
            ▲                        │
            └──────── cad-cli ───────┤
                                     │
                              cad-studio (HTTP + WebGL shell)

cad-agent-api       (the contract; serde + thiserror only)
   ▲              ▲
   │              └── cad-agent-geometry (also depends on cad-contract + cad-kernel-truck)
   │
   └── cad-agent-runtime   (the host; depends on the contract, never on an agent)
```

Rules: an **adapter** may depend on `cad-contract`; nothing in `cad-contract` may depend on an
adapter or on a kernel crate; `cad-document` may depend on the contract but never on an adapter;
applications (`cad-cli`, `cad-studio`) may depend on both. That is what keeps a kernel swap (or a
second kernel) from becoming a rewrite. `cad-studio` embeds its HTML/CSS/JS with `include_str!`, so
the shipped binary has no runtime asset path to configure or lose.

The same rule applies one level up: an **agent** may depend on the agent contract and on the code it
computes with, but never on the **runtime** that hosts it. If an agent depended on the runtime, the
runtime could no longer be replaced, and an agent could not be tested outside a host. The
`cad-agent-geometry` → `cad-agent-runtime` edge exists only in `dev-dependencies`, where it is the
test harness rather than the agent.

## 8. Workspace note

The root `Cargo.toml` lists `exclude = ["apps", "kernels", "references"]`. Without it, the nested
repositories whose manifests have no `[workspace]` of their own (`kernels/hagane`, `kernels/cascade-rs`,
`kernels/cadrum`) resolve upward to this root and fail with *"current package believes it's in a
workspace when it's not"*. This was observed during the audit and fixed; the repositories are
independent and must stay that way.
