# Implementation Tracker

**As of:** 2026-11-03. This is the per-phase tracker for the unified engineering brief. It uses the
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
| **A** | Shared engineering foundation | **rung 7 for identity, materials, provenance and per-kind release policy** | `cad-engineering-model`: document + revision + fingerprint, stable artifact ids, validated 7-material library, revision-pinned artifacts, transitive staleness, explicit review, per-kind release policy, versioned atomic JSON; embedded in `.zcad` schema 9; CLI gate (`atlas artifacts`, exit 3); studio `/api/engineering` and `/api/engineering/review`; drawing and simulation staleness | entity-level change diff; only two producers (drawing, linear static study); no signed audit trail |
| **B** | Core CAD and assemblies | **partial — core CAD at rung 7–8, assemblies absent (rung 1)** | parametric feature graph, sketch profiles, extrude/join/cut/revolve, rebuild cache, drawing editor, DXF/PDF, STL | fillet/chamfer/shell/sweep/loft/pattern, multi-body, assemblies, joints, motion, BOM, constraints, STEP/IGES |
| **C** | Basic CAM (2.5D/3-axis) | **absent (rung 1–2)** | artifact metadata kind and a recompute-only release policy | every CAM producer, tool library, simulation, postprocessor |
| **D** | Basic CAE | **partial — linear static at rung 7, batch-deployed; modal, thermal, buckling absent** | `cad-cae`: voxel hex8 mesh from the closed tessellation, isotropic linear elasticity, Jacobi-PCG solve, displacement/von Mises/strain energy/reactions, volume-error and open-mesh guards, analytic patch test and cantilever convergence series, `atlas cae static`, revision-pinned `simulation` record, a versioned `--study` job file, and a non-root batch container with exit 0/1/2/3 | modal, thermal, buckling, nonlinear, dynamic, injection moulding, electronics cooling, result fields and a viewer, boundary-fitted meshing, a job queue or submission service |
| **E** | Electronics | **absent (rung 0–1)** | one FR-4 material entry and a PCB-layout artifact kind | schematic, PCB, ERC, sync, SPICE, 3D integration, cooling |
| **F** | Advanced engineering | **absent (rung 1)** | nothing | 4/5-axis, nesting, turning, additive, nonlinear, buckling, moulding, generative design |
| **G** | AI and enterprise | **absent (rung 0–1)** | permissioned deterministic agent runtime only | AI pipeline, intent parsing, approvals, collaboration, document control, audit |

## Phase A detail

| Item | Rung | Evidence | Next rung needs |
|---|---:|---|---|
| Document/revision/fingerprint identity | 7 | revision + fingerprint mismatch tests; used by CLI and studio | security review of the JSON boundary |
| Material library and validation | 5 | 7 validated entries; impossible-property tests; consumed by the CAE producer | temperature-dependent data and a qualified source before any structural claim |
| Stable artifact identity and kinds | 6 | duplicate/zero id refusal; kind parse tests; drawing and simulation producers | more producers, or a kind registry owned by the control plane |
| Derived artifact provenance and upstream links | 7 | CLI workflow and studio HTTP tests; CAE record at the project revision | entity-level "what changed" |
| Transitive staleness and blocked-upstream propagation | 7 | chain test, blocked test, end-to-end gate | entity-level "what changed" |
| Review acknowledgment | 7 | review semantics tests; CLI review | identity/audit trail if multi-user |
| Per-kind release policy (new 2026-11-03) | 5 | simulations, toolpaths, manufacturing setups, inspections and PCB layouts require a recompute; drawings, documentation and other kinds may be released after review; a policy-blocked result blocks its dependents | a user-facing explanation in the studio and a per-artifact policy override with an audit record |
| Versioned atomic persistence | 7 | registry round trip; project schema-9 save/reopen and schema-8 migration; unknown/newer schema, 8 MiB, zero revision, no temp files | measurement on a 4096-artifact registry |
| Release gate | 7 | `atlas artifacts check` exits 3 on stale records in project-embedded and sidecar modes; a reviewed toolpath still exits 3 | CI wiring; a per-kind requirement in the report is not yet shown in the studio |

## Phase D detail

| Item | Rung | Evidence | Next rung needs |
|---|---:|---|---|
| Linear static solver | 7 | exact uniaxial patch test; cantilever vs Timoshenko at 5 / 2.5 / 1.25 mm (−12.90%, −3.95%, −1.25%); 10 unit + 6 integration tests | a second independent reference (published benchmark or physical test) before stronger accuracy claims |
| Voxel mesher | 7 | closed-mesh requirement; volume-error guard; self-intersection is not checked | a boundary-fitted or tet mesher and element-quality metrics |
| Study definition and results | 7 | versioned `--study` file; result embeds the definition, engine version and source revision, plus material, mesh, solver and assumptions | persisted result fields and a viewer |
| Support and load selection | 7 | plane selection with explicit refusals; symmetry planes validated by the patch test | face/edge selection bound to stable geometry ids |
| Provenance | 7 | `atlas cae static --record`; edit → exit 3 → review still exits 3 → recompute → exit 0; recording works inside the container | a studio surface for studies |
| Deployment job | 8 | `deploy/Dockerfile.cae` + `build-cae-image.sh --smoke`: real 384-element study (3.95% from beam theory), exit 3/2 refusals, non-root, no network, read-only root, provenance record; 160 000 elements in 24.1 s under 512 MB | image signing and scanning, a fuzz corpus, a queue/orchestrator, a cluster-run Job |
| Modal analysis | 0 | — | mass matrix, eigenvalue solver, frequency reference and convergence series |
| Thermal analysis | 0 | — | conduction/convection/radiation model and a reference case |
| Buckling | 0 | — | linear eigenvalue buckling after modal validation |
| Nonlinear/dynamic | 0 | — | an established external solver and a licence review; not to be hand-written |
| Injection moulding / electronics cooling | 0 | — | validated specialist solver, material data, 3D PCB assembly |

## Required documents

| Document | State |
|---|---|
| `docs/architecture/UNIFIED_ENGINEERING_MODEL.md` | written, describes the implemented Phase A and the release policy |
| `docs/cad/CAD_CAPABILITIES.md` | written, implemented set + limits |
| `docs/cam/CAM_CAPABILITIES.md` | written, explicitly **not implemented** |
| `docs/cae/CAE_CAPABILITIES.md` | written, **linear static implemented and validated**; other analyses explicitly absent |
| `docs/ecad/ECAD_CAPABILITIES.md` | written, explicitly **not implemented** |
| `docs/ai/ENGINEERING_AI.md` | written, no AI implemented; pipeline and safety rules stated |
| `docs/manufacturing/MACHINE_SAFETY.md` | written, no machine output/control; policy stated |
| `docs/quality/ENGINEERING_VALIDATION.md` | written, Phase A, linear static CAE and container evidence; future domain gates |
| `docs/operations/cae-deployment.md` | written, the CAE batch job contract, exit codes, measured envelope and security posture |
| `docs/roadmap/IMPLEMENTATION_TRACKER.md` | this file |

## Next highest-priority tasks

1. **Modal analysis** on the existing mesh, validated against an Euler–Bernoulli cantilever frequency
   series, then reuse the mass matrix for buckling.
2. **Persist and display result fields** so a study's displacement and stress can be inspected, not
   only summarized, and measure the payload size.
3. **A boundary-fitted/tetrahedral mesher** to remove the stair-step approximation, with element
   quality reporting.
4. **Entity-level change reporting** from `cad-document`, so a stale report can name the sketches and
   features that changed rather than only the revision numbers.
5. **CAM 2.5D foundation**: machining setup, tool library, contour/pocket/facing/drilling, a
   toolpath preview and postprocessor fixtures, gated by the recompute-only toolpath policy.

## Explicit non-claims

* No CAM, ECAD, BIM, GIS, generative-design or AI capability is implemented.
* No machine code is generated and no machine is controlled.
* Linear static results are preliminary studies; no result is certified, and no modal, thermal,
  buckling, nonlinear or dynamic result exists.
* Injection moulding and electronics cooling do not exist.
* No performance benchmark of `assess` on a large registry exists.
* The studio has no study editor or result viewer.
