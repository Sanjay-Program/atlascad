# Implementation Tracker

**As of:** 2026-10-31. This is the per-phase tracker for the unified engineering brief. It uses the
same ladder as [`docs/roadmap/FEATURE_IMPLEMENTATION_STATUS.md`](FEATURE_IMPLEMENTATION_STATUS.md):
a rung is claimed only when its acceptance has been exercised.

| # | Rung | Meaning |
|---:|---|---|
| 0 | absent | nothing exists |
| 1 | researched | a source and workflow description, no code |
| 2 | planned | acceptance criteria that can fail |
| 3 | implemented | code exists and is reachable |
| 4 | builds | compiles in the workspace |
| 5 | unit-tested | behaviour is asserted |
| 6 | integration-tested | exercised through a real boundary on a real fixture |
| 7 | end-to-end verified | the user workflow runs from input to saved output |
| 8 | performance-measured | a published number on a named host/workload |
| 9 | security-reviewed | trust boundary and inputs reviewed, findings recorded |
| 10 | released | gate run on the shipped commit, artefact distributed |

## Phase status

| Phase | Scope | Status | What is real | What is missing |
|---|---|---|---|---|
| **A** | Shared engineering foundation | **partial — rung 7 for identity/materials/provenance** | `cad-engineering-model`: document + revision + fingerprint, stable artifact ids, validated 7-material library, revision-pinned artifacts, transitive staleness, explicit review, versioned atomic JSON; CLI gate (`atlas artifacts`, exit 3); studio `/api/engineering` and drawing staleness | artifacts are a sidecar, not in `.zcad`; entity-level change diff; per-kind release policy; only one producer (drawing) |
| **B** | Core CAD and assemblies | **partial — core CAD at rung 7–8, assemblies absent (rung 1)** | parametric feature graph, sketch profiles, extrude/join/cut/revolve, rebuild cache, drawing editor, DXF/PDF, STL; see `docs/cad/CAD_CAPABILITIES.md` | fillet/chamfer/shell/sweep/loft/pattern, multi-body, assemblies, joints, motion, BOM, constraints, STEP/IGES |
| **C** | Basic CAM (2.5D/3-axis) | **absent (rung 1–2)** | artifact metadata kind and revision gate only | every CAM producer, tool library, simulation, postprocessor |
| **D** | Basic CAE | **absent (rung 1–2)** | material library and result provenance only | study model, mesh, solver, results, reference suite |
| **E** | Electronics | **absent (rung 0–1)** | one FR-4 material entry and a PCB-layout artifact kind | schematic, PCB, ERC, sync, SPICE, 3D integration, cooling |
| **F** | Advanced engineering | **absent (rung 1)** | nothing | 4/5-axis, nesting, turning, additive, nonlinear, buckling, moulding, generative design |
| **G** | AI and enterprise | **absent (rung 0–1)** | permissioned deterministic agent runtime only | AI pipeline, intent parsing, approvals, collaboration, document control, audit |

## Phase A detail

| Item | Rung | Evidence | Next rung needs |
|---|---:|---|---|
| Document/revision/fingerprint identity | 7 | revision + fingerprint mismatch tests; used by CLI and studio | security review of the JSON boundary |
| Material library and validation | 5 | 7 validated entries; impossible-property tests | temperature-dependent data and a qualified source before any structural claim |
| Stable artifact identity and kinds | 5 | duplicate/zero id refusal; kind parse tests | a second producer |
| Derived artifact provenance and upstream links | 7 | CLI workflow and studio HTTP test | persistence with the project file |
| Transitive staleness and blocked-upstream propagation | 7 | chain test, blocked test, end-to-end gate | entity-level "what changed" |
| Review acknowledgment | 7 | review semantics tests; CLI review | identity/audit trail if multi-user |
| Versioned atomic persistence | 5 | round trip, unknown/newer schema, 8 MiB, zero revision, no temp files | measurement on a 4096-artifact registry |
| Release gate | 7 | `atlas artifacts check` exits 3 on stale records | CI wiring and per-kind policy |

## Required documents

| Document | State |
|---|---|
| `docs/architecture/UNIFIED_ENGINEERING_MODEL.md` | written, describes the implemented Phase A |
| `docs/cad/CAD_CAPABILITIES.md` | written, implemented set + limits |
| `docs/cam/CAM_CAPABILITIES.md` | written, explicitly **not implemented** |
| `docs/cae/CAE_CAPABILITIES.md` | written, explicitly **not implemented** |
| `docs/ecad/ECAD_CAPABILITIES.md` | written, explicitly **not implemented** |
| `docs/ai/ENGINEERING_AI.md` | written, no AI implemented; pipeline and safety rules stated |
| `docs/manufacturing/MACHINE_SAFETY.md` | written, no machine output/control; policy stated |
| `docs/quality/ENGINEERING_VALIDATION.md` | written, Phase A evidence and future domain gates |
| `docs/roadmap/IMPLEMENTATION_TRACKER.md` | this file |

## Next highest-priority tasks

1. **Persist the artifact registry with the project** (or as a managed sidecar written atomically by
   the studio) and migrate `.zcad` accordingly. Today a registry can drift from its project file.
2. **Entity-level change reporting** from `cad-document`, so a stale report can name the sketches and
   features that changed rather than only the revision numbers.
3. **One real CAE producer**: linear static analysis on the existing watertight mesh, validated
   against a cantilever and a mesh-convergence series. This is the cheapest honest Phase D slice and
   immediately exercises the artifact registry with a second producer.
4. **A second CAM/CAE consumer** of `ArtifactKind::Simulation`/`Toolpath` that is generated rather
   than supplied by hand, so the gate protects real output.
5. Then — and only then — begin CAM 2.5D foundations and the electronics data model.

## Explicit non-claims

* No CAM, CAE, ECAD, BIM, GIS, generative-design or AI capability is implemented.
* No machine code is generated and no machine is controlled.
* No simulation result is produced or certified.
* No performance measurement of the new registry exists yet.
* The database/UI polish implied by the original brief is not present; this cycle implemented the
  shared foundation and made the rest of the absence explicit.
