# 0009 — An ordered feature graph, built one feature at a time

- **Status:** accepted
- **Date:** 2026-10-27
- **Supersedes:** none
- **Superseded by:** none

## Context

`.zcad` schema 2 stored exactly one sketch and one extrusion, and `Project::evaluate` rebuilt only
the last feature. That was honest for the single-feature application built in the 2026-10-25 cycle,
but it is not a CAD document: a CAD product's defining property is an *editable history*, where a
parameter or placement change rebuilds everything that depends on it. The document shape is the most
expensive part of the system to change after users have files, so it was changed before more
features were built on top of it.

Two constraints shaped the design:

- **The kernel decides which operations exist.** The M2 corpus records difference as unsupported and
  union as passing except for `union_tangent_contact`. A feature graph that promises cut would be
  shipping a defect generator, so only extrude + join is wired to real geometry.
- **A failed rebuild must not destroy the user's model.** The studio previously rolled the whole
  mutation back when the kernel failed, which is correct for an invalid parameter but wrong for a
  history: the user needs to see *which* feature failed and keep editing the rest.

## Decision

### 1. The project is an ordered history with stable IDs

`Project` schema 3 stores `sketches: Vec<Sketch>` and `features: Vec<Feature>` in build order. Every
sketch and feature carries an `EntityId` allocated from one monotonic `next_entity` counter, so the
order of allocation is persisted in the IDs themselves.

A feature references one sketch (`sketch_id`) and one depth parameter. Because IDs are allocated in
creation order, "the sketch existed before the feature" is exactly `sketch.id < feature.id`. That is
checked in `validate`, which means a forward reference or a dependency cycle is **unrepresentable in
a loaded file**, not merely rejected at render time. The feature list must also be strictly
increasing in ID; an out-of-order list is refused as having no defined history.

### 2. The operation is explicit, and only one body exists

`ExtrudeOperation` is `new_body` or `join`:

- the first feature must be `new_body`;
- every later feature must be `join`;
- `add_extrude_feature` refuses a second `new_body` ("this project already has a body") and a `join`
  with no body ("the first extrude must create one").

Multi-body is therefore **not** a hidden feature: it is a refused request with a message, and it is
what Stage B has to change when multi-body becomes a real requirement.

### 3. Rebuild is sequential, and join goes through the kernel's union

`Project::build` walks the features in order, feeding the running `SolidId` forward:

| Operation | Kernel calls |
|---|---|
| `new_body` | `extrude` → `tessellate` |
| `join` | `extrude` → `boolean(Union, running, extruded)` → `tessellate` |

A join first checks `KernelInfo::supports(BooleanUnion)` and refuses with a stated reason if the
adapter does not advertise it, so a kernel without booleans cannot silently produce two solids.
Solids superseded by a join are released **after** the replacement has tessellated, so a failure at
any step leaves the previous body and its mesh intact. Every tessellation is validated
(`Mesh::validate`) before it becomes the current body.

### 4. A failed rebuild is a result, not an error

`build` returns `Build { reports, mesh, solid, failed }`:

- `reports` has one `FeatureReport` per *attempted* feature, with `Built { volume_mm3, triangles }`
  or `Failed { message }`;
- `mesh` is the mesh of the last feature that succeeded;
- `failed` names the first feature that stopped the rebuild.

The walk stops at the first failure, so later features are reported by the studio as `pending`
rather than being evaluated against a body that no longer exists. `Project::evaluate` remains the
strict form for headless callers: it returns `FeatureFailed { feature, message }` and releases the
partial solid.

The studio's HTTP mutation path still rolls back when a *request* is invalid (a bad parameter value,
an unknown sketch), because an invalid request should leave the document untouched. A kernel failure
inside an otherwise valid history is different: it is stored state, reported in `/api/state` with the
feature ID and message, and undoable like any other change.

### 5. Migration is explicit and one-way

`from_json` dispatches on the declared schema:

- **schema 1 → 2:** flat rectangle fields become an explicit `SketchProfile::Rectangle`, and a
  `diameter` parameter is added if absent (`migrate_v1_to_v2`).
- **schema 2 → 3:** every feature gains an explicit `operation` — the first is `new_body`, the rest
  are `join` (`migrate_v2_to_v3`). A schema-2 file with one feature therefore means exactly what it
  meant before. A schema-2 file that somehow held several features is interpreted as the ordered
  history the old writer never evaluated.
- **schema 3 → 4:** every feature gains an explicit `kind` (`migrate_v3_to_v4`), whose only variant at
  that point is `extrude { depth_param }`, and `depth_param` moves inside it.
- **schema 4 → 5:** the same shape is preserved and the schema number is bumped for the revolve
  feature (`migrate_v4_to_v5`). A schema-4 feature becomes `kind: extrude`; a schema-5 file may hold
  `kind: revolve { axis }`, which only a reader that knows schema 5 can evaluate.
- **schema 5:** parsed directly.
- Anything newer is refused with `UnsupportedSchema { found, supported }`.

Migration happens in memory on load; **no file is rewritten unless the user saves**, and the
migration rules are covered by tests rather than prose.

## Consequences

- The document is now a real history: a parameter edit or a sketch-origin move rebuilds every later
  feature, and undo/redo restores history and geometry together because the snapshots contain both.
- `cad-cli` and `cad-studio` both consume the same `build` report; neither carries its own history
  logic.
- Cut and full-turn revolve are available because the kernel corpus and analytic tests back them;
  fillet, sweep, loft, partial revolve and multi-body remain unavailable. They are stated as
  unavailable in the UI rather than faked, and the kernel corpus is the authority on when that can
  change.
- **Known limit carried forward:** a join of two bodies that only touch tangentially hits the
  corpus's recorded `union_tangent_contact` failure. The application reports the join failure and
  keeps the previous body; it does not emit a wrong solid.
- **Performance cost:** an unchanged request is served from a per-prefix `BuildCache`, so it does no
  kernel work; an edit re-runs only the features whose resolved inputs changed. A two-feature join
  with a shared parameter still rebuilds both and measures ~18.5 ms on the recorded host; see
  `docs/performance-baseline.md` §9.
- **The graph is linear.** There is no branching, no suppressed feature, no pattern and no
  assembly reference. A history that needs any of those requires a new schema step, not a new field
  bolted onto this one.
