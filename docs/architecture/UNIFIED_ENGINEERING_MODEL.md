# The Unified Engineering Model

**As of:** 2026-10-31 · **Status:** Phase A implemented and tested; later phases are not.

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
| Versioned JSON persistence, atomic save, fail-closed load | implemented, unit-tested | unknown-field, newer-schema, oversize and zero-revision refusals |
| Headless workflow and release gate | integrated, end-to-end tested | `atlas artifacts add/review/check`, exit code 3 when action is required |
| Studio visibility of the one derived artifact it produces | integrated, HTTP-tested | `GET /api/engineering`, `derived_from_revision` and `derivation_status` in `/api/draft` |
| Simulation, toolpath, PCB and inspection producers | **not implemented** | no solver, no CAM, no ECAD; see the capability pages |

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
| `accepted_after_review` | older result, acknowledged at the current revision | yes, and visibly not current |
| `blocked_by_upstream` | itself current, but an upstream artifact requires action | no |
| `future_revision` | records a revision newer than the document supplied | no |

`ArtifactStatus::is_current` is `false` for `accepted_after_review` on purpose. Review is not
recomputation, and a report that showed a reviewed artifact as current would be the exact silent
promotion the brief forbids.

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
atlas artifacts add --registry part.artifacts.json --document 1 --revision 3 \
    --id 1 --kind drawing --label "shop drawing" --entity 1,2
atlas artifacts add --registry part.artifacts.json --document 1 --revision 3 \
    --id 2 --kind toolpath --label "roughing" --upstream 1
atlas artifacts review --registry part.artifacts.json --id 1 --revision 4
atlas artifacts check --project part.zcad --registry part.artifacts.json
```

`check` prints the report as JSON and **exits 3** when any artifact needs recomputation or review, so
a script or CI step can gate on it without parsing output. A design change after the artifacts were
recorded fails the gate; reviewing each artifact clears it and leaves the original source revision in
the report.

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

1. **One producer.** Only the drawing exists; there is no simulation, toolpath, PCB or inspection
   producer to register. The registry is exercised end to end with drawing and toolpath records, but
   the toolpath record is metadata a caller supplies, not the output of a CAM engine.
2. **No result payloads.** Summary and warnings only. Nothing here can reproduce or visualise a
   result.
3. **No automatic invalidation hook.** Producers must record the revision they used. The document
   does not yet emit a change event that updates registries in place; `assess` detects staleness at
   query time instead.
4. **No entity-level diff.** `source_entities` is recorded but not yet used to narrow "what changed";
   the report names the revisions, not the modified entities.
5. **Not persisted in the project file.** The registry is a sidecar JSON file with its own schema. A
   `.zcad` file does not carry its artifacts.
6. **Fingerprint is optional** and only compared when both sides provide one.
7. **No multi-user or audit log.** A review is a field, not a signed record; there is no user
   identity in this local-first application.
8. **No timing measurement** of assessment on a large registry.

## 8. Next steps, in order

1. **Change events with entity granularity.** Make `cad-document` report which entities a mutation
   touched, so `assess` can say *what* changed, not only that the revision moved.
2. **Persist the registry beside the project** (or inside schema 9) with migration, so artifacts move
   with the file.
3. **A second real producer.** Linear static analysis against the existing mesh is the smallest
   honest CAE producer; until it exists, `ArtifactKind::Simulation` should stay untrusted metadata.
4. **Blocked-until-validated release rules per kind.** A drawing may be released after review; a
   toolpath or structural result should require recomputation rather than review. That distinction is
   policy the registry should express, and it is not implemented.
5. **Measurement** of `assess` on a full 4096-artifact registry.
