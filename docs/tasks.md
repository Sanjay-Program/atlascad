# Next Steps — AtlasCAD Tasks Register

**Canonical source:** this file plus `engineering/tasks/{backlog,active,completed}.json` and
`engineering/manifest/{capabilities,modules}.json`. The JSON files are the machine-readable truth;
this file is the human guide through them. When they disagree, the JSON wins and this file is updated
to match.

**Status vocabulary (used strictly):**

| Status | Meaning |
|---|---|
| **Ready** | Acceptance criteria are written, inputs exist, no blocker is unresolved. |
| **In progress** | Someone is working on it now. |
| **Blocked** | Cannot start without a decision, a missing tool, or a larger host. |
| **Proposed** | Designed, not yet scheduled; needs an owner to move it to ready. |
| **Verified** | Acceptance met, evidence recorded, task closed. |
| **Cancelled** | Superseded, out of scope, or abandoned with a reason. |
| **Failed** | Attempted, did not meet acceptance, and the failure is recorded. |

**Priority vocabulary:**

| Priority | Meaning |
|---|---|
| `integration_blocker` | Without this, "what is integrated" is not honest. |
| `security` | Affects the trust boundary, supply chain, or input safety. |
| `interface_compatibility` | Affects the contract or the ability to adopt it incrementally. |
| `user_value` | Affects what a user can do in the local application. |
| `measurement` | Establishes or protects a baseline; not itself a user feature. |
| `maintenance` | Keeps the engineering control plane and docs honest. |

**Sizes (one engineer, rough):** **S** ≤ 1 day · **M** ≤ 1 week · **L** > 1 week.

---

## 0. How to read this file

Each task has an `id`, a `title`, a one-line `rationale`, a `priority`, a `risk`, a `status`, a
`depends_on` list, a `blocks` list, acceptance criteria written as testable statements, the inputs and
outputs, the required tests, the rollback plan, and the evidence once it is done.

The registry is the source of truth for what is planned; `docs/roadmap.md` is the narrative version of
the same plan for the broader audience. When a milestone is verified, both are updated in the same
cycle and the task moves from `backlog.json` to `completed.json`.

**Do not create a task without acceptance criteria that can fail.** "Improve performance" is not a
task. "Edit the last feature's parameter must not re-run the earlier extrudes, measured by kernel-call
count against the 17.3 ms baseline, with the analytic volume unchanged" is.

---

## 1. Next features for the local application (ordered)

These are the feature milestones that extend Atlas Studio's `.zcad` project workflow. They are listed
in the order the roadmap records, with the acceptance criteria already written for each, and notes on
what evidence must exist before the task can be claimed done.

### M11b — Cut, behind a verified difference (M)

**Goal:** a second extrude can remove material, and the application refuses cut on a kernel whose
difference is not proven.

**Why this order, not first:** the corpus recorded difference as unsupported, and wiring cut before
that changes is shipping a defect generator. The order was therefore: extend the corpus with analytic cut
fixtures, get the adapter's difference to pass them, then enable `ExtrudeOperation::Cut`.

**Why it turned out deliverable:** the recorded "difference returns a solid that is not the difference"
measurement was of truck's **raw** output. That output is wrong only because the walls of a cavity are
wound inward, so the enclosed volume came out as `outer + cavity`. The adapter's existing weld and
orientation repair already flips a face whose winding disagrees with its neighbour; extending it to give
each connected shell the sign its **nesting depth** requires makes a cavity subtract instead of add. With
that change the analytic fixtures pass exactly and no new geometry code was needed. The `Cut` operation,
schema 4 and the UI gating already existed in the document and studio — only the kernel capability was
withheld.

**Status:** verified for the application path, with three recorded limitations (below) ·
**Priority:** user_value · **Risk:** medium · **Verified:** 2026-10-28

**Delivered:**

1. `Capability::Transform` and `Kernel::translate` added to the contract, because an exact through-cut
   needs a tool that *overshoots* the body and `extrude` always starts at z = 0. The truck adapter
   implements both; a kernel without `Transform` keeps the old behaviour.
2. The adapter's orientation repair now orients each connected shell by its nesting depth (ray parity), so
   a cut that encloses a void subtracts the cavity. That is what turned the difference measurement from
   `1053.3 mm³` into the analytic `840 mm³`.
3. `BooleanDifference` is advertised. Both new cut fixtures pass **exactly** (delta 0.000e0):
   `difference_piercing_cut` 84 000 mm³ / 22 400 mm² and `difference_contained_void` 184 000 mm³ /
   32 800 mm². The two pre-existing known failures (tangent contact, large-coordinate intersection) still
   print and no fixture was skipped: 11 fixtures, 8 ok, 3 known-failure, 0 regressions, exit code 0.
4. `cad-document` builds every cut tool with `CUT_OVERSHOOT_MM` (0.01 mm) of overshoot on each face that
   would coincide with the body's, so a through-cut is exact rather than dependent on the kernel's coplanar
   retry. A pocket whose end lies inside the body is not extended, so it stays exact too.
5. A difference that removes the whole body is refused with an actionable message instead of returning a
   sliver. Measured before the guard: cutting a 10 x 10 x 5 plate with its own profile reported `built`
   with a 0.008 mm³ body.
6. End-to-end HTTP test `a_cut_removes_the_analytic_volume_and_survives_save_reopen_and_undo`: create ->
   cut -> **255.000 mm³** (analytic, not "close") -> undo -> redo -> save -> reopen -> STL export, with the
   exported mesh welded back and asserted watertight (0 open edges).

**Recorded limitations, not claimed away:**

1. `difference_coplanar_cut` is a **known failure**: operands that share a face need the coplanar retry,
   whose nudge leaves a 1.1e-4 mm membrane on the shared face (volume delta 0.176 mm³ on 84 000) and a
   tessellation that is not watertight (8 hairline sliver edges). The application never builds a cut this
   way, but a direct `Kernel::boolean` caller can.
2. Every cut starts on the z = 0 sketch plane, so a cut is a bottom-anchored pocket or a through-hole.
   Side-anchored or offset cuts need a sketch-plane parameter, which the document does not have.
3. Deviation from criterion 2 as written: only the two coincident-free cut fixtures are `must_pass`. The
   coplanar one is recorded as a known failure with its measurement rather than promoted, because promoting
   it would normalise a non-watertight mesh.

**Performance** (release, this host, 3 warm-up rebuilds discarded, median of 20; rebuild only, not HTTP):

| History | Median rebuild |
|---|---|
| box + join (2 features) | 3.14 ms |
| box + cut (2 features) | **1.81 ms** |
| box + cut + join (3 features) | 4.05 ms |

Measured with `cargo run --release -p cad-studio --example rebuild_bench`. The cut is cheaper than the join
because it needs no coplanar retry, and its volume is exact (255.000 mm³) where the join still carries the
union nudge's 0.011 mm³. These are rebuild times and are **not** comparable to the 17.3 ms *request* figure
in `docs/performance-baseline.md` section 8, which includes serialisation and the response write.

**Acceptance criteria:**

1. The corpus gains analytic cut fixtures — a through-cut of a plate and a blind cut of a block — each
   with a stated analytic volume and area, within a stated tolerance.
2. The truck adapter's `boolean(Difference, ...)` passes those fixtures and the corpus records them as
   `must_pass`, not as unsupported.
3. `ExtrudeOperation` gains a `Cut` variant, stored in schema 4, with a documented migration from schema
   3 on load.
4. The document refuses a cut when the kernel does not advertise `BooleanDifference`, with a stated
   reason in the feature report — the same failure mode the join already has.
5. A through-cut and a blind cut match their analytic volumes **±1 mm³** in the corpus *and* in an HTTP
   integration test against the running studio.
6. Save/reopen and undo cover the cut: a saved cut project reopens with the same volume, and undoing the
   cut removes the feature and its geometry together.
7. The corpus gate still prints the known failures it had before (tangent contact, large-coordinate
   intersection), with **no newly skipped fixture**.

**Out of scope:** curved cuts, draft, thin-wall, patterns, multi-body.

**Evidence that existed before it started:** `crates/cad-kernel-truck/examples/corpus.rs` and
`crates/cad-kernel-truck/src/corpus.rs` gave the corpus shape; `docs/kernel-comparison.md` §4.2 recorded a
1053 mm³ result for an 840 mm³ cut with an inner shell of the wrong orientation and magnitude. Re-reading
that as an *orientation* defect rather than a geometry defect is what unblocked the task.

**Evidence recorded on closure:** the analytic volumes asserted by the corpus (delta 0 on both
coplanar-free cut fixtures), the HTTP test `a_cut_removes_the_analytic_volume_and_survives_save_reopen_and_undo`,
the `Cut` operation in schema 4, and the corpus table printed by
`cargo run -p cad-kernel-truck --example corpus` (11 fixtures, 8 ok, 3 known-failure, exit code 0).

---

### M11c — Per-feature mesh cache (S)

**Goal:** editing the last feature's parameter must not re-run earlier extrudes or re-union the earlier
solid.

**Why now:** the 2026-10-27 measurement shows a full rebuild costs **17.3 ms** median on the two-feature
project, dominated by re-running every extrude and re-tessellating the whole history on every request,
including `GET /api/state`. That is inside an interactive budget today, so no optimisation was attempted;
this task exists so the next change can show a regression or a win rather than claim one.

**Status:** verified 2026-10-29 · **Priority:** measurement · **Risk:** low · **Verified:** 2026-10-29

**Delivered:**

1. `cad_document::BuildCache` stores, for each **prefix** of the ordered history, the body that prefix
   produced (`SolidId`), its mesh, its volume and its triangle count. The key is a hash of the
   *resolved* inputs of the prefix: every feature ID, operation, millimetre profile point, millimetre
   depth, the document tolerance, and the kernel identity (name, version, capability set). Resolving
   first means an edit is detected by the geometry it produces, not by a dirty flag.
2. `Project::build_with_cache` is the cached entry point; `Project::build` is unchanged and remains the
   cold path used by `cad-cli`, the agents and the corpus. Reports, mesh and failure are identical
   between the two (asserted).
3. The cache owns its solids, so the cached path does **not** release the running body. Entries from
   earlier revisions are retained up to a bound of 32, which is what makes undo back to an already
   built state free; beyond the bound the oldest are released.
4. `Kernel::contains` was added with a `None` default: when a cache is handed to a *different kernel
   instance* of the same type, its handles are discovered to be unknown and the cache is discarded
   instead of adopted. `TruckKernel` answers it from its own solid map.
5. `TruckKernel` now counts extrudes, booleans and tessellations (`extrude_calls()` and friends),
   because "the cache skipped work" has to be observable rather than asserted. The counters use
   atomics so `tessellate(&self)` can increment them.
6. `cad-studio` keeps one kernel and one cache for the life of the session instead of building a new
   kernel per request. Both `payload()` and the STL export path use the cached build, so an export
   after a state request reuses the same body.

**Measured** (details and methodology in `docs/performance-baseline.md` §9):

| Workload | Before | After |
|---|---|---|
| `GET /api/state`, unchanged two-feature history (debug, HTTP) | 17.4 ms | **0.7–0.9 ms** |
| Edit a parameter both features read (debug, HTTP) | 17.3 ms | 18.5 ms (full rebuild is unavoidable; +2% in-process over 20 samples) |
| Edit a parameter only the last of six features reads (release, in-process) | 30.3 ms | **7.9 ms** |
| Second build of an unchanged six-feature history (release) | 30.3 ms | **0.01 ms** |

**Recorded limitations, not claimed away:**

1. The cache is per kernel **instance**; it is not shared between processes or between two sessions.
2. A shared parameter still invalidates every prefix that reads it, so the two-feature benchmark with a
   shared `depth` is unchanged. That is correct, not an omission.
3. On a cache *miss* the produced mesh is cloned into the entry as well as returned, so a large mesh
   pays one extra copy per rebuilt feature. Measured at +2–5% on the full-rebuild path; the copy was
   kept rather than changing `Build::mesh` to an `Arc` for every caller.
4. Prefix keys are 64-bit FNV-1a hashes. A collision across two different histories would return the
   wrong body; the probability is negligible for interactive use but the cache is not collision-proof
   by construction, and a caller that cannot accept that should use `Project::build`.
5. The cache holds solids alive (up to 32 bodies plus their meshes), so peak memory is higher than a
   build-and-release request path. `BuildCache::clear` releases them.

**Acceptance criteria:**

1. After editing the last feature's depth parameter, the kernel-call count indicates the earlier extrudes
   and the earlier union were **not** re-run; only the last feature's extrude and its join into the
   cached running solid were performed.
2. The analytic volume of the rebuilt body is unchanged from the uncached rebuild for the same inputs.
3. The request latency for `POST /api/parameter` on the two-feature project is measured against the
   **17.3 ms** baseline recorded in `docs/performance-baseline.md` §8 and `docs/quality/m11-feature-graph.md`
   §5, with the same host, build mode, client convention and sample count.
4. The cache is invalidated correctly when an *earlier* feature or its sketch changes — not only when the
   last feature changes — so a parameter shared by two features still rebuilds both.
5. The cache does not hide a failing feature: a feature that cannot rebuild is still reported by ID with
   the kernel message, the last good mesh stays renderable, and export is refused, exactly as today.

**Out of scope:** GPU-side cache, viewport caching, cut/revolve support, any change to the schema.

**Evidence recorded on closure:** the kernel-call assertions in `crates/cad-document/src/lib.rs`
(`editing_the_last_features_parameter_reuses_the_earlier_prefix` asserts the second build issues exactly
`extrude 4` and `union 1 4`; `a_cached_rebuild_of_an_unchanged_history_does_no_kernel_work` asserts an
empty call log; `editing_an_earlier_feature_invalidates_every_later_prefix` asserts both features
rebuild), the unchanged-volume assertions in the same module and in `crates/cad-studio/tests/http_api.rs`,
the latency records in `engineering/quality/benchmarks.json` (`bm-studio-m11c-state`,
`bm-rebuild-cache`), and the measurements in `docs/performance-baseline.md` §9.

---

### M11d — Revolve (M)

**Goal:** a sketch can be revolved around an axis to form a solid of revolution, through the contract
kernel, with a documented schema migration and analytic volume tests.

**Status:** verified 2026-10-29 · **Priority:** user_value · **Risk:** medium · **Depends on:** nothing
· **Blocks:** nothing. Evidence: `engineering/tasks/completed.json#task.m11d.revolve`.

**Acceptance criteria, and what was actually met:**

1. Met. `Capability::Revolve` is in the contract register and `TruckKernel` advertises it. A kernel that
   does not advertise it is refused by the document with `does not advertise revolve`, which is asserted.
2. Met under a different schema number than planned: **schema 5**, because schema 4 was already taken by
   the cut change. `migrate_v4_to_v5` runs on load and is covered by
   `schema_four_features_migrate_to_an_explicit_extrude_kind`. The angle is a full turn by construction
   rather than an angle parameter — a partial turn is an open shell and the document carries bodies only.
3. Met, with the second case changed for good reason: the planned "half-circle revolved into a full
   cylinder" is not a solid of revolution of a half-disc, so the two analytic cases are a rectangle on
   the axis (cylinder, 150.796 mm³ against π·4²·3 within 0.05) and a rectangle offset from it (annulus,
   113.097 mm³ against π·(4²−2²)·3 within 0.05), both watertight. The annulus is the stronger test: it
   proves the sweep produced a real hole, which a cylinder cannot.
4. Met in the feature form and the feature tree, not a command palette (the studio has a form, not a
   palette). The Revolve option appears with the axis field, is disabled when the kernel does not
   advertise the capability, and a failed revolve gets the same badge and failure banner as an extrude.
5. Met. The axis round-trips through save/reopen, and undo/redo of a revolve uses the same history path
   as extrude, asserted in `cad-document` and over HTTP in `cad-studio`.

**Out of scope, and still out:** sweep, loft, helical sweep, patterned revolve, arbitrary axis lines
(the axis is vertical or horizontal through the sketch origin), partial sweeps, trimming a profile that
crosses the axis.

**Verification actually run:** `cargo test --offline --workspace -j 2` (273 passed, 0 failed), `cargo
clippy --offline --workspace --all-targets -D warnings` (0 warnings), `cargo fmt --all --check`,
`cargo run --offline -p cad-kernel-truck --example corpus` (8 must-pass ok, 3 known-failure, 0
regressions), and a headless-Chrome run of the built studio
(`node target/studio-qa/revolve.mjs <url>`, 11 DOM assertions, zero console errors: the turn built
as a `Revolve 2` row, `Turn: full revolution`, 3,133.72 mm³ against the analytic 3,141.59, export
enabled, row present after save and reload). The revolve itself is **not benchmarked** — no
performance record exists for it.

---### M11e — Additional sketch entities (S)

**Goal:** sketches can contain arcs and straight segments beyond rectangle/circle/regular-polygon, so a
profile is not limited to those three primitives.

**Status:** verified 2026-10-29 · **Priority:** user_value · **Risk:** medium · **Verified:** 2026-10-29
· **Depends on:** nothing · **Blocks:** nothing. Evidence:
`engineering/quality/tests.json#t-platform-workspace-20261029-m11e`.

**Delivered:**

1. `SketchProfile::Sector { diameter_param, start_degrees, sweep_degrees }` — an arc plus the two radii
   that close it at the sketch origin. The arc is tessellated with the circle's chord tolerance scaled
   to the swept fraction of the turn (`arc_segments`), so a quarter sector gets a quarter of a
   circle's facets rather than inheriting the whole circle's count, with a floor of two facets.
2. The segmented requirement was already met by `SketchProfile::Polyline` (schema 4, M11d cycle), so no
   second segment variant was added: it would have been a duplicate of an existing profile. The arc is
   the genuinely missing entity.
3. `.zcad` **schema 6**. The new profile kind is an additive enum variant, but a schema-5 reader would
   report it as an invalid profile rather than a newer schema, so the version is bumped and
   `migrate_v5_to_v6` (version only, no field changes) runs on load.
4. Validation happens in two places, both before the kernel: `validate_sector` refuses a non-finite
   angle, a `sweep_degrees` that is not strictly between 0 and 360, and a missing diameter parameter —
   on creation *and* on load. The resolved loop then goes through the same `validate_profile` gate as
   every other profile (≥3 points, finite, no repeated consecutive point, non-zero area).
5. The studio exposes it end to end: a **Sector (arc)** option in both the create and the profile
   selectors, Start°/Sweep° fields that appear only for a sector, the same range check in the page as
   in the server, and the angles preserved when the kind is switched away and back.

**Measured / asserted:**

* `cad-document`: a quarter sector of a 20 mm circle resolves its arc from 0° to 90° at radius 10,
  starting at the centre; its shoelace area equals the analytic inscribed-polygon formula to 1e-9 and
  is under the true quarter disc by less than 1%; its prism volume equals that area × depth exactly.
* `cad-studio` over HTTP on the real Truck kernel: a quarter sector of a 5 mm circle at (5,5) cuts a
  10×10×5 plate. The remaining volume is the analytic 500 − (*n*-gon area × 5) to 1e-2 mm³, the plate's
  bounds are unchanged, undo returns 500 mm³, redo restores the cut, save/reopen preserves the profile
  and the angles, and the exported STL is watertight (0 open edges after welding).
* Headless Chrome: the Create form opens Start°/Sweep°, adding the sketch puts a `sector` row in the
  tree, and selecting it reopens the panel with the angles it was created with (3 new browser
  assertions).

**Recorded limitations, not claimed away:**

1. The arc is a **tolerance-bounded polyline**, not an exact B-rep arc. At small radii the 24-facet
   circle floor makes a quarter sector only six chords, ~1.1% under the true quarter disc; the
   document is exact about the polygon it produces, not about the ideal arc.
2. `sweep_degrees` must be strictly between 0 and 360. A full turn is refused with a message that
   names the circle profile, and a zero sweep has no area.
3. Angles are literal numbers on the profile rather than parameters: the parameter map holds positive
   lengths, and a start angle of 0 is meaningful. Changing an angle is therefore a profile edit, not a
   parameter edit, and does not appear in the parameter panel.
4. One arc per sketch. A profile that needs two arcs or a tangent blend still needs a polyline, and a
   polyline is still a straight-segment loop — there is no mixing of arc and segment entities in one
   sketch, because a sketch is still one closed profile rather than an entity graph.
5. A sector whose arc crosses the revolve axis is refused by the same straddle check as any other
   profile; there is no partial revolve.

**Acceptance criteria, and where each landed:**

1. Met for the arc; the segmented half was already shipped as `Polyline`, and a duplicate was not
   added. A schema-5 reader refuses a sector file as "newer schema" rather than misreading it.
2. Met — `validate_sector` plus the shared `validate_profile` gate, exercised on creation and on load.
3. Met — the L-shape shoelace assertion already existed (`a_concave_polyline_resolves_its_analytic_area`,
   500 mm²); the sector assertion is
   `a_sector_resolves_an_arc_from_its_centre_with_its_analytic_area`.
4. Met — `an_arc_sector_cuts_its_analytic_volume_and_survives_save_reopen_and_undo` cuts a plate with
   the new profile and asserts the analytic volume, bounds, undo/redo, save/reopen and a watertight
   export.

**Out of scope, and still out:** spline profiles, constraint-driven sketches, dimensions, multiple
entities per sketch, exact B-rep arcs.

---

### M11f — Named revisions and recovery snapshots (M)

**Goal:** a project can record named snapshots of its history, and recover from a crash or a bad edit by
reverting to a named revision.

**Status:** verified 2026-10-29 · **Priority:** user_value · **Risk:** medium · **Depends on:** nothing
· **Blocks:** collaboration and any multi-user workflow. Evidence:
`engineering/tasks/completed.json#task.m11f.crash-recovery` and `#task.m11f.named-revisions`.

**Delivered (crash recovery, 2026-10-29):** `crates/cad-document` keeps an unsaved-edit snapshot next
to the project (`<file>.recovery`), offers it when it belongs to the current file version, falls back
to it when the project file cannot be read, deletes a superseded snapshot and ignores a corrupt one.
The studio shows a recovery banner with the time of the recovered edit and one discard action. A
`kill -9` with an unsaved edit used to lose it (24,000 mm³ → 12,000 mm³ on reopen); the same scenario
now reopens at 24,000 mm³ with `recovered` and `dirty` in `/api/state`.

**Delivered (named revisions, 2026-10-29):**

1. `Project` carries `revisions: Vec<NamedRevision>` and `next_revision`, both defaulted, in **schema
   7**. A revision is a named, timestamped copy of the editable history — parameters, sketches,
   features and the next entity id — deliberately **not** of the undo stacks, the recovery state or
   the other revisions, so restoring one cannot resurrect a deleted copy or a stale history.
2. `add_revision(name)`, `restore_revision(id)` and `remove_revision(id)` with `RevisionSummary` for
   listing. Names are trimmed, non-empty, limited to 80 characters and unique case-insensitively;
   ids live in their own space so recording a revision never shifts sketch or feature numbering.
3. **Retention is a stated rule, not silent loss:** at most `MAX_REVISIONS` (32) copies are kept and
   the oldest is dropped when a thirty-third is added. `revisions()` always reports what is kept.
4. A restore pushes the current history onto the undo stack first, so restoring the wrong revision is
   one undo away. Recording a revision is not an undo step, because it changes no geometry, but it
   does bump the document revision so the copy is not lost by closing the window unsaved.
5. **The last good on-disk revision is retained.** A successful save copies the previous file to
   `<path>.backup` (through a temporary file and a rename) before replacing it; opening a project that
   cannot be read and has no snapshot now falls back to that backup and reports
   `recovery.source: "backup"` with `file_unreadable: true`, so the banner says "previous saved copy"
   rather than pretending nothing happened. Saving the recovered document repairs the file.
6. Studio: a **Revisions** panel (name field, Save revision, one row per copy with Restore and delete),
   palette commands (`Save a named revision`, `Restore revision “<name>”`), and POST `/api/revision`,
   `/api/revision/restore`, `/api/revision/remove`. An unknown revision is `404 unknown_revision`.

**Defect found and fixed while driving the UI:** revision rows were rendered inside `applyState`,
which runs while a mutation is still in flight, so their buttons were created disabled and never
re-enabled by `setBusy(false)`. The headless-browser restore step timed out until `setBusy` re-rendered
the list. This is exactly what the browser assertion exists to catch.

**Measured (2026-10-29):**

| Workload | Result |
|---|---|
| Record, edit depth 5 → 12 mm, restore | 500 → 1,200 → 500 mm³ over HTTP, revisions preserved |
| Save and reopen | both copies still listed and restorable after a restart |
| Truncated project file with no snapshot | reopens the 5 mm backup, `source: "backup"`, `file_unreadable: true` |
| Duplicate / empty / over-long name | `400` with a message; the document keeps its existing copies |
| Unknown revision id | `404 unknown_revision` |
| Browser | Revisions panel creates "browser baseline", a parameter edit changes the body, Restore returns the original measurement, palette offers the restore (4 new assertions) |

**Recorded limitations, not claimed away:**

1. A revision is a full copy of the history, so the file grows with each one; the 32-copy bound is the
   only limit and the per-copy size is not separately capped beyond `MAX_PROJECT_BYTES` for the whole
   file.
2. No diff, labels, tags, branches or merge. A revision is a restore point only.
3. Recording a revision does not ask for a name dialog when the palette is used with an empty field —
   it generates `Revision N`, which is convenient but not very descriptive.
4. The backup is best-effort: a save in a read-only or full directory still succeeds, and the previous
   copy is then not refreshed. The atomic save itself is unaffected.
5. Backup and snapshot are single files per project path; opening a project from a different path does
   not see either.
6. No remote or cloud copy, no retention policy beyond the 32-copy bound, and no audit log of who
   created a revision.

**Acceptance criteria:**

1. The document records named, timestamped revisions stored separately from undo, surviving
   save/reopen. **Met.**
2. A revision can be named and recovered by ID. **Met**, with duplicate names refused.
3. The last good on-disk revision is retained so a corrupt file with no snapshot is recoverable.
   **Met** for a corrupt *project file* with a valid backup; the backup is refreshed only by a
   successful save.
4. The studio has a revisions panel and a palette command to revert to a named revision. **Met**, and
   driven in headless Chrome.

**Out of scope:** branch/merge, collaborative editing, cloud sync.

---

### M11g — Sketch plane height and extrude direction (M)

**Goal:** a sketch can sit on a horizontal plane above z = 0 and an extrude can sweep down from it, so
a pocket can start on a top face and a boss can sit on one.

**Why this order:** every feature started on the z = 0 plane, so a part could only grow upward from a
single level. That is the recorded M11b limitation ("every cut starts on the z = 0 sketch plane, so a
cut is a bottom-anchored pocket or a through-hole"), and it sits in front of patterns, mirrors, holes
and every industry workspace. It is a dependency, not a polish item.

**Status:** verified 2026-10-30 · **Priority:** user_value · **Risk:** medium · **Depends on:** M11b,
M11c · **Evidence:** `engineering/tasks/completed.json#task.m11g.sketch-plane-and-direction`,
`engineering/quality/tests.json#t-platform-workspace-20261030`.

**Delivered:**

1. `.zcad` **schema 8**. `Sketch.plane_z` is the height of the sketch plane in document units, and
   `FeatureKind::Extrude` gains `direction` (`up`/`down`). Both fields are defaulted, so a schema-7
   file is already valid and migrates by version alone; the bump exists so a schema-7 build reports
   "newer schema" rather than failing on an unknown field.
2. The kernel sweep always starts at z = 0, so the tool is extruded there and then **translated** to
   its plane (`place_on_plane`). A kernel that does not advertise `Transform` refuses a raised plane
   with a stated reason instead of silently building at z = 0.
3. A **downward** extrude resolves to a negative depth. That is what makes a top-face pocket
   expressible without a negative parameter: the parameter map holds positive lengths, so the
   direction is an explicit input rather than a sign.
4. A join whose tool sits exactly on a body face would be the kernel's coplanar case. The tool is
   moved `JOIN_OVERLAP_MM` (0.01 mm) **into** the body instead, where the extra material is already
   present, so the union's volume is unchanged. The adjustment is skipped unless the profile lies
   inside the body's footprint, because a tool that overhangs the body has material outside it and
   moving the face there would add or drop a sliver. That guard is why the pre-existing partial-
   overlap join still measures 980 mm³; the first implementation shrank the tool instead and measured
   978.08 mm³, a real 1.9 mm³ error found by the existing test.
5. A cut tool's overshoot now accounts for the plane: the tool spans `[plane + min(depth,0),
   plane + max(depth,0)]`, so a through-hole swept down from a top face has both faces coincident and
   is still exact.
6. The plane is part of the rebuild-cache key, so moving a sketch to another height rebuilds exactly
   the prefixes that read it; `set_sketch_placement` applies coordinates and height as one undoable
   transaction.
7. Studio: a **Plane Z** field on both the create form and the placement panel, an **Direction**
   selector (Up +Z / Down −Z) for extrudes, `plane_z` and `direction` in `/api/state`, and `plane` /
   `direction` accepted by `/api/sketch` and `/api/feature/extrude`. An unknown direction is a `400`
   that names itself.

**Measured / asserted** (real Truck kernel, over the real HTTP surface):

* a blind pocket on plane 3 of a 20 × 20 × 5 plate removes the inscribed-polygon area × 2 mm, the
  plate keeps its bounds, undo returns 2000 mm³, redo restores it, save/reopen preserves the plane and
  the exported solid is watertight (0 open edges);
* a top-face boss on plane 5 adds area × 5 mm and raises the bound to z = 10, and its export is
  watertight;
* a downward through-hole from plane 5 removes area × 5 mm exactly, with both tool faces coincident
  with the body's;
* document-level: an inch plane reaches the kernel as 12.7 mm, a signed direction resolves to ±5, a
  kernel without `Transform` fails with the capability named, moving the plane invalidates the cache,
  placing a sketch is one undo step, a non-finite plane is refused, and a schema-7 file loads at
  schema 8 with plane 0 / direction up.

**Measured cost** (`docs/performance-baseline.md` §11): a two-feature plate plus raised boss rebuilds
cold in **4.80 ms** (cached edit 4.41 ms) against **2.65 ms** for a same-size join at z = 0.

**Recorded limitations, not claimed away:**

1. Planes are **horizontal only**. A tilted or face-aligned plane is not representable, so a feature
   can be raised or lowered but not reoriented.
2. One body per project. A boss is a union into the body, not a separate component, so no assembly.
3. The join overlap only applies inside the body's footprint; an overhanging boss is joined as drawn
   and may hit the kernel's coplanar behaviour, which the feature reports if it fails.
4. A cut whose tool would remove the whole body is still refused.
5. The plane height is a literal number, not a parameter, so it is not in the parameter panel.
6. Only the two-feature raised-plane workload is benchmarked.

**Out of scope, and still out:** patterns, mirrors, holes/threads, fillets, multiple bodies, tilted
planes, sketches on a selected face of the body.

---

## 2. The correctness and kernel work (higher engineering value)

### M2 — Correctness corpus for booleans (M) — highest engineering value — partially delivered

**Goal:** a fixture set the adapter and the real kernel both run, each with analytic volume/area, and a
pass/fail table per kernel whose exit code gates regressions.

**Status:** delivered for the truck adapter · **Priority:** integration_blocker · **Risk:** medium ·
**Depends on:** nothing for the adapter half · **Blocks:** M11b cut (difference must pass first).

**What is done:** `crates/cad-kernel-truck/examples/corpus.rs` runs ten fixtures with analytic volumes and
areas and a stated tolerance, and prints a pass/fail table whose exit code gates regressions. Result:
**8 ok, 2 known failures** (union_tangent_contact, intersection_large_coordinates), **3 difference fixtures
recorded unsupported**, 0 regressions, 0 unexpected passes, exit code 0.

**What is not done:** the real kernel (`solidcraft-kernel`) has **not** been run against the corpus. That
needs the ≥16 GB RAM host, so the "two kernels on one corpus" comparison the definition of done asks for is
still open. Until it is, `docs/kernel-comparison.md` cannot honestly claim which kernel is more reliable.

**Acceptance criteria (remaining):**

1. The corpus gains analytic cut fixtures (through-cut and blind cut) so M11b has something to verify
   against.
2. The adapter's difference passes those fixtures and the corpus records them as `must_pass`.
3. The real kernel is run against the corpus on a ≥16 GB host, and the result is recorded per kernel in the
   report — not as a hand-written claim.
4. The corpus gate still prints the known failures it had before, with no newly skipped fixture.

**Evidence:** `crates/cad-kernel-truck/src/corpus.rs`, `crates/cad-kernel-truck/examples/corpus.rs`,
`docs/kernel-comparison.md` §3.1, `engineering/quality/tests.json#t-cad-kernel-truck-corpus`.

---

### M1 — Prove the contract against the real kernel (M) — needs a larger host

**Goal:** `solidcraft-kernel` implements `cad_contract::Kernel` behind a feature flag, and a test asserts
that the contract path and the existing kernel agree on volume for the same fixtures.

**Why:** it is additive, changes no behaviour, and converts "we have a contract" into "the contract
describes the kernel we ship". It also immediately exposes which contract methods the mature kernel can
implement better than the experimental adapter (difference, chains, fillets). It is not next because its
acceptance run needs a ≥16 GB RAM machine and the root application's history model has higher user value per
hour.

**Status:** proposed · **Priority:** interface_compatibility · **Risk:** medium ·
**Depends on:** `task.quality.solidcraft-tests` (the kernel test suite must be runnable on a larger host
first) · **Blocks:** nothing, but unlocks M2 for the real kernel and makes M11b cut's "verified difference"
claim honest.

**Acceptance criteria:**

1. `cad-contract` is an optional dependency of `apps/solidcraft/crates/kernel`, feature `cad-contract`,
   **off by default**.
2. `Kernel` is implemented by delegating to the existing `Body` API; `KernelError` maps onto the contract
   taxonomy (the four categories already match).
3. A test builds a box both ways and compares `volume()` from the contract path with the kernel's own
   answer, within a stated tolerance.
4. No existing public signature changes; the existing test suite passes unchanged.
5. `cargo test -p solidcraft-kernel --features cad-contract` passes on a machine with ≥16 GB RAM.

**Out of scope:** any change to the existing kernel API, any wiring into the application.

**Evidence this task needs before it can start:** a host with ≥16 GB RAM, and the SolidCraft kernel test
suite passing there. The current host cannot build the full workspace; one crate build peaked at 2.56 GB RSS
with only ~1.3 GB available.

---

## 3. The integration and adoption work (contract value)

### M3 — Wire units and IDs into CADCraft (S)

**Goal:** replace the implicit `INSUNITS` handling and `Handle` with `cad-contract::Unit`/`EntityId`
(`EntityId` is already the same `u64`, so this is mostly a type alias plus conversions), with round-trip
tests for DXF files with inch and millimetre units.

**Why:** it is the cheapest real adoption, and unit bugs are the most expensive class in CAD.

**Status:** proposed · **Priority:** interface_compatibility · **Risk:** low · **Depends on:** nothing ·
**Blocks:** nothing.

**Acceptance criteria:**

1. `Unit` and the `INSUNITS` mapping round-trip through the DXF reader/writer for millimetre and inch
   drawings.
2. `EntityId` is introduced for new code and `Handle` converts to it losslessly; no persisted file changes
   meaning.
3. Existing CADCraft tests still pass.

**Out of scope:** rewriting the CADCraft document model, any 3D work.

**Evidence:** `docs/architecture.md` §5 step A1/A2, `docs/roadmap.md` M3.

---

### M4 — Import budgets and cancellation in SolidCraft IO (M) — security — half done

**Goal:** add file-size, entity-count and 3MF zip-entry/uncompressed-size limits to SolidCraft's `crates/io`,
and a cancellable import.

**Status:** adapter half done · **Priority:** security · **Risk:** high · **Depends on:** nothing ·
**Blocks:** nothing, but is the highest-severity item still open after the adapter half.

**What is done:** the adapter half. Every kernel operation now takes a `Budget` — a deadline plus a
cancellation token — and reports `Timeout`/`Cancelled` instead of hanging. Cancellation is cooperative, not
preemption: a single pathological boolean inside truck cannot be interrupted, which is stated in the module
rather than implied away.

**What is left:** the import half. SolidCraft's IO crates still have no file-size or entity-count limits,
which is security finding **S-3** and the highest-severity item still open. CADCraft's `crates/io` already has
explicit caps (`MAX_PLOT_STYLE_TABLES = 1000`, `MAX_PLOT_STYLE_NAMES = 100_000`, `MAX_XDATA_ITEMS = 4096`,
`MAX_PAYLOAD = 64 MiB`); SolidCraft has nothing equivalent visible for STEP/3MF sizes.

**Acceptance criteria:**

1. Maximum file size, entity count, triangle count, zip entry count and uncompressed size are enforced and
   configurable for every SolidCraft IO reader.
2. An import can be cancelled and returns a structured error rather than hanging.
3. A test feeds a zip-bomb-shaped 3MF and a truncated STEP file and asserts a clean error.
4. The plain entry points use defaults; the `_with` variants take custom budgets — the same shape the kernel
   adapter already uses.

**Out of scope:** fuzzing, advisory scanning, path confinement (already exists in SolidCraft's `vfs.rs` and
MCP `--allow-path`).

**Evidence:** `docs/security-review.md` S-3, `docs/roadmap.md` M4.

---

## 4. The governance, security and release work (blocking commercial readiness)

### M5 — Dependency governance in CI (S)

**Goal:** `cargo-deny` with a `deny.toml` in both apps; a CI guard that fails if `opencascade*`/`cadrum`
enter a shipping graph; record the `craft-libs` licence and vendor the pinned commits.

**Why:** closes S-1, S-2 and S-5, the three findings that can block a commercial release.

**Status:** proposed · **Priority:** security · **Risk:** high · **Depends on:** nothing · **Blocks:**
nothing directly, but is required before any commercial release claim.

**Acceptance criteria:**

1. `cargo-audit` and `cargo-deny` are available and their versions recorded.
2. `deny.toml` exists for CADCraft, matching SolidCraft's existing policy.
3. `cargo deny check advisories bans licenses sources` runs for both applications and its output is recorded.
4. A CI guard fails when `opencascade*` or `cadrum` enters a shipping dependency graph.
5. The `craft-libs` licence is recorded and the pinned commits are vendored.

**Evidence:** `docs/security-review.md` S-1, S-2, S-5; `docs/license-inventory.md`; `docs/roadmap.md` M5.

---

### M6 — Benchmark the things that actually dominate a CAD session (M)

**Goal:** the current baseline measures geometry primitives. Build the fixtures that measure what users feel —
document load, selection/snap on a large drawing, undo snapshot cost, constraint solve, parametric rebuild,
STEP/DXF round trip — at 10k–100k entity scale, and publish the table.

**Why:** without this, any later "optimisation" is unmoored.

**Status:** proposed · **Priority:** measurement · **Risk:** low · **Depends on:** nothing · **Blocks:**
nothing, but sets the baseline for any future performance claim.

**Acceptance criteria:**

1. Document load/save timing for a `.zcad` project at representative scale.
2. Selection/snap timing on a large 2D drawing (CADCraft uses `rstar 0.12` already; no baseline taken).
3. Undo snapshot cost for both CADCraft's copy-on-write `EntityStore` and the document's snapshot stack.
4. Constraint solve timing for CADCraft `constraints` (4 283 LOC) and SolidCraft `sketch` (23 681 LOC).
5. Parametric rebuild timing beyond the current single-feature and two-feature numbers.
6. STEP/DXF round-trip timing.

**Out of scope:** GPU frame time on this host (integrated Intel UHD G4 only; any figure would mislead).

**Evidence:** `docs/performance-baseline.md` §5 (the baselines that still need to be established), §6
(the rules for future performance work).

---

### M7 — Persistent project format expansion (L) — schema 2 shipped; schema 3 is M11

**Goal:** `.zcad` schema 2 stores stable IDs, parameters, a rectangle/circle/polygon sketch, one extrude,
and bounded undo history, with schema-1 migration and verified atomic save/load. It still lacks a feature
graph, constraints, named revisions and recovery snapshots. M11 generalizes the payload to an ordered feature
list; this milestone continues afterward with named revisions, recovery snapshots and additional sketch
entities. Continue to refuse unknown fields and newer schemas rather than silently discard data.

**Status:** in progress (schema 8 shipped: feature graph, named revisions, recovery, arc sectors,
sketch planes and extrude direction) · **Priority:** user_value · **Risk:** medium · **Depends on:**
M11e, M11f, M11g · **Blocks:** anything that needs collaboration, versioning or SaaS.

**Acceptance criteria:** the schema expansion steps above, each with its own acceptance criteria in the
feature tasks. The format must continue to refuse unknown fields (`serde(deny_unknown_fields)`) and newer
schemas (an error, not a silent downgrade).

**Evidence:** `docs/roadmap.md` M7; `docs/architecture.md` §6 item 4; `crates/cad-document/src/lib.rs`.

---

### M8 — WebAssembly feasibility spike (M, time-boxed)

**Goal:** install `wasm32-unknown-unknown` + `wasm-pack`, build `cad-contract` (no I/O, no threads) and
measure size/startup. Then try the truck adapter alone. Report numbers; do not promise a browser app.

**Why:** the brief asks, and the honest answer requires a measurement that has not been taken.

**Status:** proposed · **Priority:** measurement · **Risk:** low · **Depends on:** nothing · **Blocks:**
nothing.

**Acceptance criteria:**

1. The target is installed and `cad-contract` builds for `wasm32-unknown-unknown` with no I/O and no
   threads.
2. The built size and startup are measured and reported, not asserted.
3. The truck adapter alone is attempted and the result (build or blocker) is reported.
4. The spike is time-boxed and does not promise a browser application.

**Evidence:** `docs/roadmap.md` M8; `docs/feature-matrix.md` §3 "WASM target for the contract/adapter —
Absent / unverified".

---

### M9 — Give the agent platform a second agent and a host (M)

**Goal:** the runtime and one agent exist. Two things would prove the platform is a platform rather than a
worked example:

1. **A second agent with a different permission profile.** `agent.cad.import-export` is the obvious
   candidate: it would need `read_workspace`, and its contract would exercise a *different* schema shape and
   a different permission set. Two agents with different profiles is the minimum evidence that the contract is
   not accidentally geometry-specific.
2. **A host that is not the test harness.** The registry today is driven by tests. It needs a small CLI
   (`atlas agent run <id> <input.json>`) so an agent can be exercised from a shell and from CI without writing
   Rust.

**Why:** an agent platform with one agent and no host is a library, not a platform. The brief asks for agents
that can be registered, tested, replaced and removed without redesigning the application.

**Status:** proposed · **Priority:** interface_compatibility · **Risk:** medium · **Depends on:** nothing ·
**Blocks:** M10.

**Acceptance criteria:**

1. A second agent with a different permission profile exists and passes the runtime's contract validation,
   schema enforcement and budget checks.
2. A CLI host exists that can run an agent from a shell and from CI without writing Rust.
3. The two agents have different profiles, so the contract is shown to be not geometry-specific.

**Out of scope:** a scheduled or distributed backend (that is M10, deferred until a second host exists).

**Evidence:** `docs/roadmap.md` M9; `docs/feature-matrix.md` §3b; `docs/architecture.md` §5c.

---

### M10 — Replace the in-process registry with a scheduled one (L)

**Goal:** the registry is synchronous and single-owner: a long operation blocks the calling thread. A job
queue with bounded concurrency, cancellation across process boundaries and a persistent registry would let the
desktop and web clients share the same agents.

**Why:** defer until a second host exists (M9) — scheduling a single in-process agent is not worth the
complexity, and the brief is explicit that a distributed backend should not be built before the product needs
it.

**Status:** proposed · **Priority:** maintenance · **Risk:** high · **Depends on:** M9 · **Blocks:** nothing.

**Acceptance criteria:**

1. A job queue with bounded concurrency exists.
2. Cancellation can propagate across process boundaries.
3. The registry persists across restarts.
4. The desktop and web clients can share the same agents through it.

**Out of scope:** anything before M9 exists.

**Evidence:** `docs/roadmap.md` M10; `docs/architecture.md` §6 item 5; `docs/security-review.md` S-8.

---

## 5. The explicitly deferred work (and why)

| Item | Why not now | Revisit when |
|---|---|---|
| Collaboration (Yrs/Yjs) | Requires a broader persistent document model (M7) and semantic conflict recovery; the first local project slice is single-user | After M7 expansion |
| SaaS backend (Axum/Postgres/billing) | The engine must work without cloud; billing before persistence is backwards | After M7 + a real multi-user need |
| Second geometry kernel | No candidate has demonstrated an advantage; every extra kernel multiplies test surface | A corpus (M2) shows truck loses on a feature the product needs |
| DWG native work | Currently `acadrust` (MPL-2.0) used unmodified; no corpus to judge compatibility | A legally obtained DWG test set exists |
| Replacing egui with a custom renderer | No measured product benefit; both apps already use egui 0.36 + wgpu 30 | A rendering benchmark shows a user-visible problem |
| Merging CADCraft and SolidCraft into one crate tree | They have incompatible document models and different release cadences; the contract is the integration point | A shared shell is actually required by a product decision |
| WASM browser application | No measurement taken yet; M8 is the spike that would inform this | After M8 |

---

## 6. Decision gates (owner decisions, not engineering tasks)

These are not tasks an engineer can complete alone. They are recorded here because the next engineering
tasks depend on them, and a task should not be started behind an unresolved gate.

| Gate | Options | Consequence | Status |
|---|---|---|---|
| **G1 — LGPL/OCCT** | (a) exclude OpenCASCADE entirely; (b) dynamic-link OCCT with relink rights; (c) relicense the product | Blocks any use of `opencascade-rs`/`cadrum`. Default today: **exclude**. | **Blocked** — owner decision required |
| **G2 — Licence of the combined product** | Current root platform license is personal, non-commercial; imported components retain their own licenses | Commercial use of AtlasCAD root crates requires prior written permission. Imported application licenses do not change that requirement. | **Open** |
| **G3 — Distribution model** | Personal/non-commercial distribution under the current root license; commercial service or product only after written permission | Determines whether the SaaS work (M7+) is permitted and on the critical path. | **Open** |
| **G4 — Performance target** | e.g. "100k-entity drawing pans at 60 fps on integrated graphics" | Without a target, rendering work cannot be prioritised or judged. | **Open** |

---

## 7. The tasks as they actually live in the registry

The machine-readable task files are:

- `engineering/tasks/backlog.json` — proposed / ready / blocked / failed / needs_review
- `engineering/tasks/active.json` — in_progress (at most a handful at once)
- `engineering/tasks/completed.json` — verified / cancelled

**Current shape (read from the files, not asserted):**

- `backlog.json` carries the curated feature, adoption, security and governance tasks above, plus the
  scanner-generated change-detection tasks that appear when the workspace changes.
- `active.json` is empty when nothing is in progress.
- `completed.json` carries the verified tasks, including the M11 feature-graph tasks closed in the
  2026-10-27 cycle.

**What "done" looks like for a task:**

1. The task moves from `backlog.json` to `active.json` when someone starts it.
2. The acceptance criteria are exercised — not just claimed — and the evidence is recorded in
   `engineering/quality/` and/or `docs/`.
3. The task moves to `completed.json` as `verified`, with an `evidence` list that points at the concrete
   records.
4. If a task fails, it moves to `completed.json` as `failed` with the failure recorded, or back to
   `backlog.json` as `failed` / `needs_review` if the reason is not final.

**What the scanner does and does not do with tasks:**

- The scanner generates *change-detection* tasks when the workspace changes (repository discovered/missing,
  commit changed, working tree dirty, files modified, package added/removed).
- The scanner **never** edits a curated task, so a verified task is never reopened by a re-scan, and a
  repeated scan is a no-op (`engineering/tasks/backlog.json` byte-unchanged).
- Generated task ids are derived from the observed value, so re-scanning an unchanged workspace creates
  nothing.

---

## 8. The next cycle, concretely

If the goal of the next cycle is to keep the application moving while the higher-value kernel and governance
work waits on a larger host and an owner decision, the ordered slice is:

1. **A second named agent plus a CLI host (M9)** — the runtime is exercised only by tests today; a host
   and a second permission profile would make the platform claim honest.
2. **Import budgets and cancellation outside the adapter (M4 remaining)** — SolidCraft's IO crates
   still have no file-size or entity-count limits, which is security finding S-3.
3. **A larger host for M1/M2 and the SolidCraft suite** — the contract has never described the kernel
   that `apps/solidcraft` actually ships, because its tests need more RAM than this machine has.

Closed in earlier cycles, kept here so the ordering is not rewritten after the fact: M11b cut (2026-10-28),
M11c per-feature rebuild cache (2026-10-29), M11d revolve (2026-10-29), M11e arc sector profile
(2026-10-29), M11f named revisions and last-good-file backup (2026-10-29), the cache memory
measurement (2026-10-29, `docs/performance-baseline.md` §10).

If the goal of the next cycle is to make the contract honest about the real kernel, the slice is:

1. **M1** — implement `cad_contract::Kernel` for `solidcraft-kernel` behind a feature flag, off by default,
   on a ≥16 GB host.
2. **M2 (remaining)** — run the real kernel against the corpus, record the per-kernel table, and extend the
   corpus with analytic cut fixtures.

If the goal of the next cycle is to close the findings that block a commercial release, the slice is:

1. **M5** — `cargo-deny` in both apps, the CI guard that forbids `opencascade*`/`cadrum`, and the
   `craft-libs` licence recorded and vendored.
2. **M4 (remaining)** — import budgets and cancellation in SolidCraft IO, closing S-3.

No cycle should claim more than one of these slices as "done" without the evidence to back each task's
acceptance criteria. The registry and this file are updated together when a slice closes.

---

## 9. What must not be claimed before these tasks close

1. **"Booleans work."** Union, intersection and difference work for the recorded corpus fixtures —
   including both coplanar-free cut fixtures, exactly. They still fail on **coplanar operands** (a
   recorded known failure with a 0.176 mm³ membrane on 84 000 mm³ and 8 hairline non-manifold edges),
   on **tangent contact** and at **coordinates near 1e6 mm**, and boolean **chains of 3+ operands panic
   inside truck**. Do not generalise from the passing fixtures.
2. **"Cut is verified for what it covers."** M11b is closed: the analytic through-cut and blind-cut
   volumes are asserted in the corpus and over HTTP, the document refuses a cut on a kernel that does
   not advertise difference, and save/reopen/undo cover it. The recorded limits stand: a coplanar cut
   is a known failure, every cut starts on the z = 0 sketch plane, and a cut that would remove the
   whole body is refused.
3. **"The real kernel is verified against the corpus."** Not until M2's remaining acceptance is met on a
   ≥16 GB host.
4. **"The agent platform is production-ready."** One agent exists and runs; there is no scheduler, no
   persistence of registrations, no process isolation for third-party code, and no second agent to show that
   the contract is not accidentally geometry-specific.
5. **"Atlas Studio is a CAD product."** It is a verified multi-feature local workflow: ordered sketches and
   extrudes with stable IDs, `new_body`/`join`/`cut`, a full-turn revolve, rectangle/circle/polygon/
   polyline/sector profiles, parameter and sketch-placement edits that rebuild every dependent feature,
   per-feature rebuild status with failure reporting, undo/redo, named revisions with restore,
   crash recovery and a last-good-file backup, save, reopen, and binary STL export. It
   does **not** yet provide sketch constraints, dimensions, loft, sweep, fillet, chamfer, shell, patterns,
   multi-body, assemblies, drawings, or STEP/IGES/DXF import and native neutral-format interchange. Those
   remain planned work in this file and in `docs/roadmap.md`.
6. **"The application is fast enough."** The 17.3 ms rebuild figure is a measurement, not a target. M6
   establishes the baselines that would let a performance claim be honest.
7. **"Dependencies are safe."** Not until M5 closes S-1, S-2 and S-5. Advisory scanning is still not running
   on this host.
