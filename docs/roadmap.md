# Roadmap

**Principle:** each milestone must be *verifiable on this hardware*, must not break either shipping
application, and must end with a test that fails if the feature regresses.

Sizes are rough effort for one engineer: **S** ≤ 1 day, **M** ≤ 1 week, **L** > 1 week.

---

## Done in the 2026-10-31 cycle

| Item | Result | Evidence |
|---|---|---|
| **Phase A — unified engineering model** | **done for identity, materials and provenance; 14 unit tests + 1 doctest** — the new `cad-engineering-model` crate gives every derived artifact a stable id, the document/revision/fingerprint it came from, its source entities and its upstream artifacts. Materials are validated values in engineering units with an explicit "not a certified specification" source. The registry refuses duplicate ids, unknown upstreams, cycles, newer schemas, unknown fields, zero revisions and files over 8 MiB, and saves atomically. | `crates/cad-engineering-model`, `docs/architecture/UNIFIED_ENGINEERING_MODEL.md` |
| Staleness propagation and review | **tested** — a source change marks an artifact `requires_review`; a content change at the same revision (`requires_recompute`) is caught by the optional fingerprint; a chain propagates; a current artifact built on a stale upstream is `blocked_by_upstream`; review records an explicit acknowledgment without changing the source revision or claiming a recompute. | unit tests in `cad-engineering-model` |
| **`atlas artifacts` release gate** | **done, 2 end-to-end CLI tests** — `add`, `review` and `check` manage a versioned sidecar registry; `check` exits **3** when any recorded artifact needs action. The test records a drawing and a dependent toolpath, edits the model, observes the failure, reviews both artifacts and observes the gate open with the original source revisions intact. | `crates/cad-cli/tests/artifacts_workflow.rs` |
| Studio provenance | **done, 1 new HTTP test (32 in the suite)** — a generated drawing follows the model and reports `current`; a saved drawing is pinned, and a later model edit makes it report `derived_from_revision: <old>` / `derivation_status: requires_review`; `GET /api/engineering` assesses it through the same registry and returns the seven-material library. A drawing loaded from a `.zdraft` file reports `unknown` rather than guessing. | `crates/cad-studio/src/lib.rs`, `crates/cad-studio/tests/http_api.rs` |
| Required capability/status documents | **written, with absence stated** — CAD capabilities, CAM/CAE/ECAD (explicitly not implemented), engineering AI policy (no AI exists), machine safety policy (no machine output or control), validation policy, and the phase tracker. | `docs/{cad,cam,cae,ecad,ai,manufacturing,quality}/`, `docs/roadmap/IMPLEMENTATION_TRACKER.md` |

**Still absent and not implied above:** every CAM, CAE, ECAD, BIM, GIS, generative-design and AI
capability; machine output and control; assemblies and motion; constraints; fillet/chamfer/shell/
sweep/loft/pattern; STEP/IGES. The registry makes a future producer's provenance trustworthy; it does
not create a producer.

## Done in the 2026-10-30 cycle

| Item | Result | Evidence |
|---|---|---|
| **M11g** — sketch plane height and extrude direction (`.zcad` schema 8) | **done, 8 document tests + 3 analytic HTTP tests + 5 browser assertions + 1 measurement** — a sketch sits on a horizontal plane at any height and an extrude sweeps up or down from it, so a pocket can start on a top face and a boss can sit on one. A blind pocket, a downward through-hole and a top-face boss each match the analytic inscribed-polygon volume to 1e-2 mm³ over HTTP against the real Truck kernel, and both raised-plane exports are watertight. A join tool that sits exactly on a body face is moved 0.01 mm into the body, which is volume-neutral only inside the body's footprint, so an overhanging join is left exactly as it was. | `crates/cad-document`, `crates/cad-studio`, `engineering/quality/tests.json#t-platform-workspace-20261030`, `benchmarks.json#bm-rebuild-boss-plane`, `docs/performance-baseline.md` §11 |
| Measured cost of a raised plane | **done** — a two-feature plate plus top-face boss rebuilds cold in **4.80 ms** (cached edit 4.41 ms) against **2.65 ms** for a same-size join at z = 0; the volume is the analytic 597.057 mm³ | `docs/performance-baseline.md` §11 |
| Root workspace gate | **green** — 309 tests (up from 298), fmt and clippy `-D warnings` clean; the browser check grew to 20 assertions and the HTTP suite to 25 | `engineering/quality/tests.json#t-platform-workspace-20261030` |
| Research and platform documents requested by the 2026-10-30 brief | **written, with status separated from proposal** — capability matrix, industry-toolset roadmap, API/plugin compatibility, system requirements, feature implementation status | `docs/research/CAD_CAPABILITY_MATRIX.md`, `docs/research/INDUSTRY_TOOLSETS.md`, `docs/research/API_AND_PLUGIN_COMPATIBILITY.md`, `docs/platform/SYSTEM_REQUIREMENTS.md`, `docs/roadmap/FEATURE_IMPLEMENTATION_STATUS.md` |

| **M12** — 2D drafting: a drawing document, DXF read/write and the plan panel (new 2026-10-30) | **done, 25 document tests + 1 CLI workflow + 10 HTTP assertions + 5 browser assertions** — a `.zdraft` drawing holds layers (ACI colour, visibility, lock, line weight) and point/line/circle/arc/polyline entities, validates every entity before it enters the document, keeps a bounded undo history and refuses unknown fields, newer schemas and oversized files. DXF goes out as R12 and comes back in: the reader honours the layer table (including an off layer and the locked flag), `$INSUNITS` and polyline vertices through `SEQEND`, counts entity types it cannot represent instead of dropping them, and fails a malformed file by naming the offending group. The studio shows the plan, its counts and extents, and downloads the DXF through the token gate; the drawing is built from the sketch, not the mesh, so a failed feature does not blank it. A writer→reader→writer round trip is byte-identical. | `crates/cad-drafting`, `crates/cad-cli/tests/draft_workflow.rs`, `crates/cad-studio/tests/http_api.rs`, `t-platform-drafting-20261030` |
| Root workspace gate after M12 | **green** — 339 tests (up from 309), fmt and clippy `-D warnings` clean; the live simulation stayed at 56 assertions and the browser check grew to 24 loopback / 27 shared | `engineering/quality/tests.json#t-platform-drafting-20261030` |
| **M12b** — drafting editor, text and dimensions, plotted PDF sheet (new 2026-10-30) | **done, 18 layout/PDF tests + 2 HTTP workflows + 1 CLI workflow + 4 browser assertions** — the Drawing panel is an editor: Line, Circle, Poly, Point, Dim and Text tools post validated entities to the drawing document, with undo/redo, delete, layer creation and layer on/lock, and **Save drawing** writes a `.zdraft` beside the project that the model then leaves alone. `Text` and `Dimension` are first-class entities, rendered in the page and on the sheet and written to DXF (a dimension exploded into lines and ticks and a text value). A `Sheet` lays the drawing out on A4–A0 or Letter, portrait or landscape, at an explicit scale or a standard `fit` that never enlarges, with a frame and a title block (title, number, revision, author, date, scale, unit, paper, count) and overall dimensions; it refuses a scale that does not fit by naming the one that would. The page is written as uncompressed PDF 1.4 with base-14 Helvetica, so the text is selectable: a rendered A3 sheet was inspected and every field extracted. | `crates/cad-drafting/src/{sheet,pdf}.rs`, `crates/cad-studio/assets/app.js`, `crates/cad-cli/src/main.rs`, `t-platform-editor-20261030` |
| Root workspace gate after M12b | **green** — **360 tests** (up from 339), fmt and clippy clean, corpus unchanged at 8 ok / 3 known / 0 regressions; simulation 56 assertions, browser 27 loopback / 31 shared, zero console errors | `engineering/quality/tests.json#t-platform-editor-20261030` |
| **M13** — deployment: container, systemd unit, proxy, and the two sharing bugs it exposed (new 2026-10-30) | **done, 2 new HTTP tests + 1 container smoke test run end to end** — `deploy/` holds a Dockerfile, an entrypoint, a hardened systemd unit, an env template, a Caddyfile and `build-image.sh --smoke`. The smoke run builds the image, waits for the container's own healthcheck, and asserts: liveness without a token (200), the document without a token (401), the document with it (200), and a plotted `%PDF-1.4`. Two real defects were found by doing it rather than describing it: a **shared studio rejected every proxied request** because it required the presented `Host` to carry its own listen port (so `-p 7599:7519` and Caddy both returned 403), and a **loopback-bound studio could not be token-guarded at all**, which is exactly the shape a TLS proxy on the same host needs. Both are fixed and covered. Liveness is now the one API route answered without the token. | `deploy/`, `crates/cad-studio/tests/http_api.rs`, `docs/operations/deployment.md` |

**Not done in this cycle, and not implied by the table above:** patterns, fillet, chamfer, shell,
sweep, loft, multi-body, assemblies, projected/section/detail views, GD&T, a BOM, sheet sets or batch
plot, snapping/grips/trim in the drawing editor, STEP import or export, industry workspaces, the AI
layer, a plugin SDK, cloud billing, or any enterprise capability. DXF (R12) and PDF are the only
formats read or written. Those remain absent as listed in `docs/feature-matrix.md` and
`docs/roadmap/FEATURE_IMPLEMENTATION_STATUS.md`, and no row of this table should be read as covering
them.

---

## Done in the 2026-10-29 cycle

| Item | Result | Evidence |
|---|---|---|
| **M11c** — per-feature rebuild cache (`cad_document::BuildCache`) | **done, 7 new document tests** — the body after each feature prefix is cached by resolved inputs; an unchanged history issues no kernel call, an edit re-runs only the features that read it, a failure is reproduced rather than cached, and a cache handed to another kernel instance is discarded | `crates/cad-document`, `t-cad-document-m11c`, `docs/performance-baseline.md` §9 |
| Measured effect of the cache | **done** — `GET /api/state` 17.4 → 0.7 ms, unchanged and six-feature histories 30.3 → 0.01 ms and 30.3 → 7.9 ms; a parameter two features share still costs a full rebuild (18.5 ms, no regression) and that is recorded rather than hidden | `docs/performance-baseline.md` §9, `bm-studio-m11c-state`, `bm-rebuild-cache` |
| Control plane made valid again | **done** — `atlas-eng validate` now reports 0 errors and 0 warnings: the task schema gained the documented `user_value` and `measurement` priorities plus the optional closure fields (`acceptance_results`, `contract_changes`, `limitations`, `verified_on`, `verified_host`) the 2026-10-28 entries already used, and the two incomplete task records were completed rather than trimmed | `crates/cad-engineering/src/model.rs`, `engineering/tasks/completed.json` |
| **M11d** — full-turn revolve (`Kernel::revolve`, `.zcad` schema 5) | **done, 12 new tests + a scripted browser run** — a rectangle on the axis measures the analytic cylinder and an offset rectangle the analytic annulus, both watertight; a partial turn and a profile across the axis are refused with a reason; the studio offers the turn, hides the depth field, shows the axis, and keeps the row through save/reload | `crates/cad-kernel-truck`, `t-cad-kernel-truck-m11d`, `t-cad-document-m11d`, `t-cad-studio-browser-m11d` |
| **M11e** — arc-sector sketch profile (`.zcad` schema 6) | **done, 5 new tests + 3 browser assertions** — a profile can be one arc of a parameter-driven circle plus its two radii; the arc is tessellated at the circle's chord tolerance scaled to the swept fraction; a quarter sector resolves to its analytic shoelace area and cuts a plate to the analytic volume through save/reopen; a 0° or 360° sweep is refused with a reason; the studio has Start°/Sweep° fields that round-trip from the tree | `crates/cad-document`, `crates/cad-studio`, `t-cad-document-m11e`, `t-cad-studio-m11e` |
| **M11f** — named revisions, the last-good-file backup, and a cache memory measurement | **done, 7 new tests + 4 browser assertions + 1 measurement** — a project keeps up to 32 named, timestamped copies of its history; restoring one is a single undo away and survives save/reopen; a truncated file with no snapshot opens the previous good `<file>.backup` and says so; the cache's 32-prefix memory cost is measured at 68.8 MB against 27.6 MB uncached on a 32-feature, 10,460-triangle history | `crates/cad-document`, `crates/cad-studio`, `t-cad-document-m11fr`, `t-cad-studio-m11fr`, `bm-cache-memory-32`, `docs/performance-baseline.md` §10 |
| Root workspace gate | **green** — 273 tests (up from 261), fmt and clippy `-D warnings` clean; the corpus is unchanged at 8 ok / 3 known failure / 0 regressions | `engineering/quality/tests.json#t-platform-workspace-20261029-m11d` |
| Latest root workspace gate (after M11f) | **green** — **298 tests** (up from 286 at the start of the day), fmt and clippy `-D warnings` clean, corpus 8 ok / 3 known failure / 0 regressions, 56 live HTTP/CLI assertions and 15 + 17 browser assertions; the gate always rebuilds the release binaries it simulates, after a stale one was caught | `engineering/quality/tests.json#t-platform-workspace-20261029-revisions` |

## Done in the 2026-10-27 cycle

| Item | Result | Evidence |
|---|---|---|
| **M11** — `.zcad` schema 3 feature graph: ordered sketches and features, stable IDs, explicit `new_body`/`join`, schema-1/2 migration | **done, 36 document tests** — 9 new tests; a forward reference or cycle is unrepresentable because a feature may only reference an earlier ID | `crates/cad-document`, ADR-0009, `t-cad-feature-graph` |
| Extrude-join wired to the real kernel | **done** — join goes through `Kernel::boolean(Union)` and is refused with a stated reason when the adapter does not advertise `BooleanUnion`; two 10×10×5 boxes overlapping 2×2 mm measure **980 mm³ ± 1**, bounds 18×18×5 | `crates/cad-document`, `t-cad-feature-graph`, `t-cad-studio-m11` |
| Failure propagation instead of rollback | **done** — a feature that cannot rebuild is reported by ID with the kernel message, the last good mesh stays renderable, export is refused, and undo removes it | `crates/cad-studio`, `docs/quality/m11-feature-graph.md` |
| Atlas Studio multi-feature UI | **done, 13 studio tests + browser run** — model tree with operation/status per feature, sketch placement panel, join form, failure banner; the browser workflow creates two sketches, joins them, saves, reopens, exports and recovers via undo | `docs/quality/m11-feature-graph.md` |
| Multi-feature cost measured | **done** — 17.3 ms per full rebuild (median of 20), 12.6 ms save, 31 ms startup on the two-feature project; the single-feature path is unchanged at 3.5 ms | `docs/performance-baseline.md` §8 |
| Root workspace gate | **green** — 239 tests, fmt and clippy `-D warnings` clean (superseded by 261 on 2026-10-29) | `engineering/quality/builds.json#b-platform-workspace-20261027` |

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
| Control plane refreshed for the feature graph | **done** — `cap.document.feature-graph` added (33 capabilities, 17 verified); module boundaries, public APIs, compatibility and performance baselines updated; the two M11 tasks and the two scanner change-tasks are closed as `verified` with evidence, and a repeated scan is a no-op | `engineering/reports/latest-integration-report.md`, `engineering/tasks/completed.json` |

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

## Just completed — M11 (2026-10-27)

### M11 — Stage A: a real feature graph with multi-feature join (M) — **done 2026-10-27**

Delivered as schema 3 with `new_body`/`join` and a rebuild report. The evidence, the exact numbers
and the remaining limits are in `docs/quality/m11-feature-graph.md`; the decision is ADR-0009. What
follows is the milestone as it was written, kept for the record.

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

**Met on 2026-10-27.** The failure path was exercised with a sketch placed 1e9 mm out, where the
kernel fails to attach a plane to the wire; the document test additionally injects a failing join
directly. In both cases export is disabled rather than writing a mesh that no longer matches the
history.

---

## Next, in order

### M11b — Cut, behind a verified difference (M) — **done 2026-10-27**

**Goal:** a second extrude can remove material, and the application refuses cut on a kernel whose
difference is not proven.

The corpus already had three analytic cut fixtures (coplanar, piercing, contained void). The order
was: get the adapter's difference to pass them, then add `ExtrudeOperation::Cut` with the same
capability check and failure reporting the join already has. The adapter's `boolean(Difference)` was
already implemented (it computes `a AND NOT b` and applies the same coplanar retry and volume-
conservation guard the union uses); it was only absent from the advertised capability set.

**Met on 2026-10-27.** The adapter now advertises `BooleanDifference`; the document adds
`ExtrudeOperation::Cut` and a cut is refused with a stated reason when the kernel does not advertise
it; the corpus's three difference fixtures flip from `unsupported` to `must_pass` and the gate stays
green with no newly skipped fixture; the studio exposes cut in the operation select, the command
palette, the model tree badge and the failure banner; save/reopen and undo cover the cut.

**Acceptance (from the plan, now met):** a through-cut and a blind cut match analytic volumes ±1 mm³
in the corpus *and* in an HTTP integration test; a kernel that does not advertise
`BooleanDifference` refuses the feature with a stated reason; save/reopen and undo cover the cut; the
corpus gate still prints the known failures it had before, with no newly skipped fixture.

**Out of scope:** curved cuts, draft, thin-wall, patterns.


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

1. M11b acceptance met: cut gated on a passing difference corpus, with analytic through-cut and
   blind-cut volumes in the corpus and in an HTTP test, capability refusal, undo and save/reopen.
2. The feature rebuild cost is either bounded by a per-feature cache (with a measured improvement
   against the 17.3 ms record) or explicitly accepted with the measurement attached.
3. M5 CI gates green, or the missing tools reported as blockers rather than claimed.
4. M1 merged with the flag off by default and a passing agreement test on a ≥16 GB host.
5. Every remaining "Blocked" row in `docs/feature-matrix.md` either has an owner decision or a date.
6. The S-3 import budgets are closed in SolidCraft IO, so `M4` can be marked done rather than half
   done.

### Closed by the 2026-10-27 cycle

- ~~M11 acceptance met~~ — multi-feature join, dependency refusal, failure propagation, schema-2
  migration and a browser workflow that survives save/reopen: `docs/quality/m11-feature-graph.md`.
- ~~The feature graph's recompute cost measured, correctness checked in the same run~~ — 17.3 ms per
  rebuild with the 980 mm³ union asserted by the tests in the same cycle:
  `docs/performance-baseline.md` §8.
