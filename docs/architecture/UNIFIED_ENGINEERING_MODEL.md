# The Unified Engineering Model

**As of:** 2026-11-03 · **Status:** Phase A implemented and tested, including a per-kind release policy; linear static analysis is the first real CAE producer. Later phases are not.

This page describes the shared engineering data layer that a CAD, CAM, CAE or ECAD workspace uses to
answer one question honestly: **which derived result belongs to which design revision?** It is the
first phase of the unified CAD + CAM + CAE + ECAD brief, and it is deliberately smaller than the
brief: it is the identity, material and dependency foundation the later workspaces need, not a
solver, a toolpath engine or a schematic editor.

Implementation: [`crates/cad-engineering-model`](../../crates/cad-engineering-model/src/lib.rs).
CLI: `atlas artifacts`. Studio: `GET /api/engineering` and the drawing fields in `GET /api/draft`.

## 1. What exists today

| Capability | Status | Evidence |
|---|---|---|
| Stable document + revision + content fingerprint (`DocumentRef`, `Fingerprint`) | implemented, unit-tested | `Fingerprint::of_bytes`, revision/fingerprint mismatch tests |
| Stable artifact identity (`ArtifactId`, non-zero) | implemented, unit-tested | duplicate-id and zero-id refusal tests |
| Typed artifact kinds (drawing, simulation, manufacturing setup, toolpath, inspection, PCB layout, documentation, other) | implemented, unit-tested | `ArtifactKind::parse`/`label` round trip |
| Material definitions with validated, attributable property values | implemented, unit-tested | 7-entry built-in library; impossible-property refusal tests |
| Derived artifacts pinned to a source revision, source entities and upstream artifacts | implemented, unit-tested | registry round-trip; duplicate and unknown-upstream refusal |
| Transitive staleness detection over an acyclic dependency graph | implemented, unit-tested | `ArtifactRegistry::assess`; chain and blocking tests |
| Explicit review that acknowledges staleness without rewriting the source revision | implemented, unit-tested | `ArtifactRegistry::review`; `AcceptedAfterReview` keeps `is_current() == false` |
| Per-kind release policy: simulation, toolpath, manufacturing setup, inspection and PCB layout are recompute-only; drawing, documentation and other may be released after review | implemented, unit-tested | `ArtifactKind::release_requirement`, `ArtifactAssessment::policy` |
| Versioned JSON persistence, atomic save, fail-closed load | implemented, unit-tested | unknown-field, newer-schema, oversize and zero-revision refusals |
| Headless workflow and release gate | integrated, end-to-end tested | `atlas artifacts add/review/check`, exit code 3 when action is required |
| Studio visibility of the one derived artifact it produces | integrated, HTTP-tested | `GET /api/engineering`, `derived_from_revision` and `derivation_status` in `/api/draft` |
| Linear static simulation producer | implemented, unit- and end-to-end tested | `cad-cae` and `atlas cae static --record`; see `docs/cae/CAE_CAPABILITIES.md` |
| Toolpath, PCB and inspection producers | **not implemented** | no CAM, ECAD or inspection engine |

The important limit in that table: the model records and reasons about artifacts, but today only one
producer exists — the drawing. A registry can hold a `simulation` or `toolpath` record, and the CLI
will correctly gate on it, but nothing in this workspace generates those results yet.

## 2. Why a shared identity layer comes first

Every later workspace in the brief has the same failure mode. A study is run, the part changes, and
the old result is presented as if it still describes the part. A toolpath is posted, the model is
revised, and the machine runs the stale program. A schematic and a PCB drift apart.

The fix is not a bigger application; it is that every derived result must record the revision and the
inputs it came from, and the system must be able to say which results are no longer valid. That is
what this crate does, and it is why Phase A of the brief is implemented before any CAM or CAE
workspace:

* a source revision is a first-class, persisted value, not a display string;
* the dependency graph is validated for duplicates, unknown references and cycles, so assessment is
  total and cannot loop;
* staleness propagates: a current artifact built on a stale upstream is reported as blocked, not
  current;
* a review is recorded as an explicit act at a revision. It never edits the artifact's source
  revision and never claims a recomputation happened.

## 3. Data model

### 3.1 Document and revision identity

```text
DocumentRef { document: DocumentId, revision: Revision, fingerprint: Option<Fingerprint> }
```

* `DocumentId` and `Revision` come from `cad-contract` and are the same identities the `.zcad`
  document already persists. Nothing invents a second document id space.
* `Revision` must be positive; zero is refused on construction and on load.
* `Fingerprint` is a deterministic 64-bit FNV-1a hash. It is a change detector, not a cryptographic
  integrity guarantee. It matters for the case a revision number cannot catch: a file restored from
  a backup or hand-edited with its `revision` left alone. When both sides state a fingerprint and
  they differ at the same revision, the artifact is reported as requiring recomputation.

### 3.2 Materials

`Material` carries a stable slug id, a human name, a category and validated property values in
engineering units: density (kg/m³), Young's modulus and yield strength (MPa), Poisson ratio,
thermal conductivity (W/(m·K)) and maximum service temperature (°C). A property may be absent when
the library has no defensible value; absent is not zero.

`MaterialLibrary::standard()` ships seven entries (1018 steel, 6061-T6 aluminium, Ti-6Al-4V, C11000
copper, ABS, PLA, FR-4). **Every entry states that it is a typical handbook value and not a
certified material specification.** That sentence is part of the data, is asserted by a test, and is
the reason a study may cite the library as a starting estimate but not as a qualified material
source.

### 3.3 Derived artifacts

```text
DerivedArtifact {
  id, kind, label,
  source: DocumentRef,            // the revision it was produced from
  source_entities: [EntityId],     // the design entities it read
  upstream: [ArtifactId],          // results it was derived from
  result: { summary, warnings, produced_at_unix },
  reviewed_revision: Option<Revision>,
}
```

The record stores the result's **summary and warnings, not the result**. Solver fields, toolpath
moves, a generated Gerber set and a plotted sheet stay in their producing workspace; duplicating them
here would create a second source of truth. The registry is the index that says whether such a
payload may be trusted for the current revision.

### 3.4 Status

| Status | Meaning | Releasable? |
|---|---|---|
| `current` | derived from the current revision | yes |
| `requires_review` | derived from an older revision, not acknowledged | no |
| `requires_recompute` | same revision, different fingerprint: content changed underneath it | no |
| `accepted_after_review` | older result, acknowledged at the current revision | only for kinds whose policy allows review; a simulation or toolpath still needs recomputation |
| `blocked_by_upstream` | itself current, but an upstream artifact requires action | no |
| `future_revision` | records a revision newer than the document supplied | no |

`ArtifactStatus::is_current` is `false` for `accepted_after_review` on purpose. Review is not
recomputation, and a report that showed a reviewed artifact as current would be the exact silent
promotion the brief forbids.

### 3.5 Release policy per kind

A review is a person saying "I have seen that this came from an older revision". For a drawing or a
document, that can be enough: the artifact is a presentation of the design and the reader can judge
it. For a result that makes a claim about physical behaviour or machine output, it is not: the stale
result does not become a description of the new part because someone acknowledged it.

`ArtifactKind::release_requirement` therefore splits the kinds:

| Policy | Kinds | A reviewed stale result |
|---|---|---|
| `review_or_recompute` | drawing, documentation, other | releasable, and still reported as `accepted_after_review` (not current) |
| `recompute` | simulation, toolpath, manufacturing setup, inspection, PCB layout | **not** releasable; the gate still requires action until the producer records a result at the current revision |

`ArtifactAssessment` reports both the `status` and the `policy`, so a consumer does not have to
derive the rule. A current result is releasable under either policy. The upstream propagation uses
the policy-aware answer, so a drawing built on a reviewed-but-recompute-only study is
`blocked_by_upstream` rather than releasable.

## 4. Assessment algorithm

1. `validate` checks the schema, every artifact's fields, duplicate ids, positive revisions, the
   registry/document match and the absence of cycles.
2. `topological_order` runs Kahn's algorithm over the upstream edges; a cycle or a dangling
   reference is an error, so assessment is total and cannot recurse without bound.
3. Artifacts are visited upstream-first. Each one's own source status is computed, and if it is
   releasable but any upstream requires action, it becomes `blocked_by_upstream` naming the
   offending artifact.
4. The report keeps insertion order for display and sets `requires_action` when any artifact is not
   releasable.

Assessment changes nothing. `GET /api/engineering` and `atlas artifacts check` are reads; only
`review` and `add` write.

## 5. Integration points

### 5.1 CLI — a release gate

```sh
atlas artifacts add --project part.zcad --revision 3 \
    --id 1 --kind drawing --label "shop drawing" --entity 1,2
atlas artifacts add --project part.zcad --revision 3 \
    --id 2 --kind toolpath --label "roughing" --upstream 1
atlas artifacts review --project part.zcad --id 1 --revision 4
atlas artifacts check --project part.zcad
```

`check` prints the report as JSON and **exits 3** when any artifact needs recomputation or review, so
a script or CI step can gate on it without parsing output. A design change after the artifacts were
recorded fails the gate; reviewing each artifact clears it and leaves the original source revision in
the report. The older `--registry FILE --document ID` form is still accepted for a registry that is
not embedded in a project, but the project form is the one that cannot drift from the design.

### 5.2 Studio — the drawing as the first derived artifact

`GET /api/engineering` returns the document id, the live revision, the unit, the material library and
the drawing assessment built through the same registry the CLI uses. `GET /api/draft` gained
`derived_from_revision` and `derivation_status`.

* A drawing generated from the sketch follows the model: whenever the model revision changes it is
  regenerated, so it reports `current`.
* Once saved as a `.zdraft` it is pinned. A later model edit leaves it reporting
  `derived_from_revision: <old>` and `derivation_status: requires_review` instead of silently
  presenting a stale sheet.
* A drawing loaded from a file does **not** record its source revision (`.zdraft` has no such field),
  so its status is `unknown`. The studio refuses to guess.
* **Saving** a drawing with `POST /api/draft/save` records (or updates) the drawing artifact in the
  project's embedded registry at the current revision, then saves it with the next `POST /api/save`.
  `POST /api/engineering/review {"id":1,"revision":N}` records an explicit review through the same
  `ArtifactRegistry::review` path as the CLI. Recording a result does not bump the design revision;
  the studio marks the project dirty itself so the record is not lost.

## 6. Validation rules (all enforced on write and on load)

* schema must equal `1`; a newer schema is refused, not downgraded;
* the file is refused above 8 MiB before parsing, and `deny_unknown_fields` rejects extra keys;
* artifact ids are non-zero and unique; labels are 1–200 characters;
* result summaries are ≤ 2000 characters, ≤ 64 warnings, each ≤ 500 characters;
* source revisions are positive and belong to the registry's document;
* an artifact may name at most 64 direct upstream artifacts, no duplicates, and never itself;
* the dependency graph must be acyclic; a registry is limited to 4096 artifacts;
* a review revision may not predate the artifact's source revision.

## 7. Recorded limitations

1. **Two producers.** The drawing and the linear static study are generated by real producers; a
   toolpath, PCB or inspection record is still metadata a caller supplies, not the output of an
   engine.
2. **No result payloads.** Summary and warnings only. Nothing here can reproduce or visualise a
   result.
3. **No automatic invalidation hook.** Producers must record the revision they used. The document
   does not yet emit a change event that updates registries in place; `assess` detects staleness at
   query time instead.
4. **No entity-level diff.** `source_entities` is recorded but not yet used to narrow "what changed";
   the report names the revisions, not the modified entities.
5. **Persistence is single-writer.** The registry is now embedded in `.zcad` (schema 9) and written
   atomically with the project. There is no file locking, so two processes editing one project still
   race in the ordinary way; the older sidecar registry remains readable for compatibility. A
   `.zdraft` drawing file itself still carries no source revision.
6. **Fingerprint is optional** and only compared when both sides provide one.
7. **No multi-user or audit log.** A review is a field, not a signed record; there is no user
   identity in this local-first application.
8. **No timing measurement** of assessment on a large registry.

## 8. Next steps, in order

1. **Change events with entity granularity.** Make `cad-document` report which entities a mutation
   touched, so `assess` can say *what* changed, not only that the revision moved.
2. **Modal analysis** on the existing CAE mesh, then thermal analysis and buckling, each with its own
   analytic reference and convergence series.
3. **Persisted result payloads** (displacement and stress fields) with a viewer, so the registry's
   summary points at an inspectable result instead of only describing it.
4. **A per-artifact policy override with an audit record**, so a team can justify an exception to the
   recompute rule without the default being weakened.
5. **Measurement** of `assess` on a full 4096-artifact registry.
