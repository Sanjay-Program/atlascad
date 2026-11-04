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
| **C** | Basic CAM (2.5D/3-axis) | **2.5D, 3D roughing, rest machining, wall and surface finishing, feeds and speeds, and holder checking at rung 7 headless; turning absent** | `cad-cam`: setup/stock/WCS, tool library with holders, facing/contour/pocket/drilling/slotting, 3D Z-level roughing from a sampled part heightfield, Z-level wall finishing by contouring the sampled cross-section, ball-nose 3D finishing, toolpath validation (continuity, axis limits, gouging, rapids through the stock), material-removal stock preview, tool-holder clearance against the carved stock, setup sheet, tool list and paginated PDF, Grbl/LinuxCNC/Mach3 postprocessors with byte-exact fixtures and a tool change in one program, `atlas cam gcode` and `atlas cam job`, a versioned job file, a toolpath artifact recorded at the design revision, and a hardened batch image, 69 domain tests and 19 CLI tests | turning, machine-head and fixture collision, machine simulation, a tool-vendor cutting-data database, GUI |
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

## Phase C detail

| Item | Rung | Evidence | Next rung needs |
|---|---:|---|---|
| Setup, stock and work coordinate system | 7 | stock outside the machine envelope and a clearance plane below the stock top are refused; machine->work mapping is checked against the envelope | tilted WCS and 4/5-axis kinematics |
| Tool library | 7 | flat/ball end mill, drill and face mill; duplicate ids, non-positive diameters, spindle speeds above the machine limit and a plunge feed above the cutting feed are refused | a tool-vendor cutting-data database |
| Feeds and speeds | 7 | six cases on material cutting data: aluminium at 6 mm and 3 flutes gives 13 262 rpm / 795 mm/min / 318 mm/min plunge; an 8 000 rpm cap gives 480 mm/min and reports `spindle_limited`; a 300 mm/min feed cap reports the achieved chip load; 12 mm and 3 mm tools follow the chip-load table; an unknown material and a zero flute count are refused | per-operation adaptive control, a vendor data feed |
| Facing, contour, pocket, drilling, slotting | 7 | reference fixture with exact move counts, offset contour corners, closed loops and peck depths; deep cuts split by the tool's maximum depth of cut | adaptive clearing |
| Toolpath validation | 7 | 12 acceptance cases: short, non-finite, unset feed, zero-length, 250 mm discontinuity, envelope, gouge, rapid-through-stock collision, clearance and spindle | fixture collision, machine simulation |
| Postprocessors | 7 | Grbl, LinuxCNC and Mach3 byte-exact fixtures, modal moves, optional line numbers, a tool change in one program (Grbl `M0` pause, LinuxCNC/Mach3 `T.. M6`), unsafe toolpaths refused before posting | a controller-accepted program on real hardware, and a versioned post definition |
| `atlas cam gcode` | 7 | CLI test writes a program and a JSON summary; an impossible depth exits 3 without writing a file | a studio surface and a job-file form |
| Material-removal preview | 7 | heightmap stock model carved by the validated program; facing a 40 x 30 block 2 mm deep reports 2 400 mm³ within 5%; rapids remove nothing; text render and deepest cut | a triangle or voxel model that can show an undercut |
| Setup sheet and tool list | 7 | generated from the same setup, tools and toolpaths as the program; machine, stock, WCS, clearance, per-tool feeds/speeds and per-operation moves, lengths and time | a per-fixture layout page |
| PDF setup sheet | 7 | a self-contained A4 writer (Helvetica, uncompressed streams) with a valid trailer whose `startxref` points at the xref table, deterministic bytes, escaped parenthesised text and pagination past 48 lines | a text-shaping layer for non-ASCII titles |
| 3D roughing | 7 | two-block fixture (16 mm beside 8 mm): the tall block is untouched below its top, the low block stops at the deepest level leaving the 0.5 mm allowance, stock beside the part is cleared to the bottom skin, levels respect the 2 mm maximum depth of cut, and a part filling the stock returns `NothingToMachine` | a watertight part requirement |
| Rest machining | 7 | a channel fixture with a 3 mm slot: the 6 mm prior pass leaves the slot at full height, the 2 mm rest pass clears it to 7 mm over a 5 mm base, the legs stay untouched, the cleared ring is not re-cut, every rest cut stays in the slot, and a prior tool that is not larger is refused | reading a real prior program instead of re-planning it, and a rest pass that is not Z-level |
| 3D finishing | 6 | six cases on a sampled step part: every surface cut sits one ball radius above the sampled height (15.0 mm over a 12 mm top), the allowance lifts it, a flat end mill and a surface above the clearance plane are refused, and reversing the raster leaves the same cut length | pencil/steep-shallow finishing, a normal-offset (non-conservative) contact point |
| Wall finishing | 7 | a 20 x 20 x 12 mm block inside the stock: every cut stays at least one tool radius from the wall and within reach of the roughing shell, levels respect the 2 mm maximum depth of cut and stop at the floor, the pass removes stock roughing left, a part that fills the stock returns `NothingToMachine`, and a negative allowance is refused | slicing the mesh boundary and offsetting the contour, instead of contouring a grid-sampled cross-section (which leaves about one cell on the wall) |
| Tool-holder clearance | 6 | the holder is checked as a wide cylinder against the progressively carved heightmap: a short tool with a wide holder collides at the named move, a 19 mm narrow slot does not clear a 20 mm holder, long flutes and no holder pass | machine head, spindle nose, fixtures and clamps; side entry into material that was never above the holder |
| `atlas cam job` | 7 | one command writes `program.nc`, `setup.md`, `setup.pdf`, `stock-preview.txt` and `summary.json` from an STL part or, without one, faces the block; `--material`/`--flutes` use calculated cutting data, `--rest MM` adds rest machining, and `--finish MM` adds wall finishing and a ball-nose surface pass, all in one program with tool changes; CLI tests assert all five, the operations, the changes, the printed summary, a material program (`S13262 M3`, `F795`), a rest run from flags and from a job file, and a holder collision that exits 3 without writing a program | a studio surface |
| Versioned CAM job file, provenance and batch image | 7 | `--job` reads schema 1, refuses unknown fields, caps the file at 64 KiB, resolves the part beside the job and refuses every conflicting input flag, all with exit 2; `--project` + `--record` writes a recompute-only `toolpath` artifact at the current revision and `atlas artifacts check` passes until the design moves on; `deploy/build-cam-image.sh --smoke` built `atlas-cam:0.1.0` and ran it hardened: 614 moves and five files, holder collision exit 3 with nothing published, malformed job exit 2, uid 10001, no output volume exit 2 | cluster orchestration was not executed; recording needs a project, so an STL-only job has no revision to pin |
| Turning, multi-axis | 0 | — | a real producer, a reference fixture and a collision test each |

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
| `docs/cam/CAM_CAPABILITIES.md` | written, **2.5D CAM implemented and tested**; 3D, turning, simulation and GUI explicitly absent |
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
5. **CAM depth**: 3D finishing/surfacing over the existing height field, and a stock preview that
   can show an undercut, before any turning or multi-axis claim.

## Explicit non-claims

* No ECAD, BIM, GIS, generative-design or AI capability is implemented.
* CAM posts G-code for Grbl, LinuxCNC and Mach3, but no program has been run on a controller and no
  machine is controlled. 3D milling, turning and multi-axis CAM do not exist.
* Linear static results are preliminary studies; no result is certified, and no modal, thermal,
  buckling, nonlinear or dynamic result exists.
* Injection moulding and electronics cooling do not exist.
* No performance benchmark of `assess` on a large registry exists.
* The studio has no study editor or result viewer.
