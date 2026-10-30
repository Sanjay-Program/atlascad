# Feature Implementation Status

**As of:** 2026-10-30. **This is the authoritative status page.** It is derived from recorded test,
benchmark and task records, not from the roadmap's intentions. Where it disagrees with another
document, this page plus `engineering/quality/*.json` win.

## 1. The ladder

A feature's status is the highest rung whose acceptance has actually been exercised. A rung is never
inferred from a lower one: a build is not a test, a unit test is not an end-to-end run, and an
end-to-end run is not a performance measurement.

| # | Rung | What must exist |
|---:|---|---|
| 0 | Not researched | nobody has looked |
| 1 | Researched | a source and a workflow description, no code |
| 2 | Planned | acceptance criteria that can fail, dependencies named |
| 3 | Implemented | code exists and is reachable |
| 4 | Builds successfully | the package compiles in the workspace |
| 5 | Unit-tested | a test asserts the behaviour, not that a button exists |
| 6 | Integration-tested | exercised through the API/kernel boundary on a real fixture |
| 7 | End-to-end verified | the user's workflow, from input to saved output, driven as a user would |
| 8 | Performance-measured | a published number on a named host and workload |
| 9 | Security-reviewed | the trust boundary, inputs and dependencies reviewed with findings recorded |
| 10 | Released | passed the release gate on the shipped commit and was distributed |

## 2. Implemented features and their rung

| Feature | Rung | Evidence | Gap to the next rung |
|---|---:|---|---|
| `.zcad` project: atomic save, migration, refusal of unknown/newer schemas | **8** | 78 `cad-document` tests; corrupt/oversized/newer fixtures; `t-platform-workspace-20261030` | security review of the untrusted-input path |
| Ordered feature history with per-prefix rebuild cache | **8** | kernel-call-log tests; 0.01 ms unchanged / 7.9 ms last-of-six; `docs/performance-baseline.md` §9 | security review; the 64-bit hash collision caveat is recorded |
| Sketch profiles: rectangle, circle, regular polygon, polyline, arc sector | **8** | analytic shoelace/volume tests; HTTP cut tests; browser sector run | security review |
| Sketch plane height + extrude direction (new 2026-10-30) | **8** | 8 document tests + 3 analytic HTTP tests (pocket, boss, through-hole, 1e-2 mm³) + 5 browser assertions; `docs/performance-baseline.md` §11 | security review (input is a finite f64, validated on create/load) |
| Extrude as new body / join / cut | **8** | corpus + HTTP analytic volumes; watertight exports; 1.50–4.80 ms rebuilds | coplanar join/kernel robustness is a recorded limitation, not a rung |
| Full-turn revolve | **7** | analytic cylinder/annulus over HTTP; save/reopen; watertight export | no benchmark (rung 8); security review |
| Named revisions + last-good-file backup + crash recovery | **7** | 7 document tests, HTTP round-trip, `kill -9` scenario, browser restore | no benchmark; no audit log (recorded) |
| Studio HTTP API (state, parameter, sketch, feature, revision, undo/redo/save, STL, discard) | **7** | 25 real-socket tests, incl. token boundary and payload limits | no benchmark of request latency since the cache (in-process figures only) |
| WebGL viewport: orbit/pan/zoom/fit/views, wireframe, picking | **7** | headless-browser checks, 20 assertions, zero console errors | no frame-time measurement (integrated GPU only; a figure would mislead) |
| Token-gated LAN sharing | **7** | 401/403/415/413 tests; shared-page browser run | security review is partial (finding S-4 covers the MCP/agent port, not this); no concurrency model |
| Binary STL export | **7** | volume asserted from the exported file, 0 open edges after welding | no importer wired |
| Analytic kernel corpus + regression gate | **8** | 11 fixtures, 8 ok, 3 recorded known failures, exit-code gate; `t-cad-kernel-truck-corpus` | the real SolidCraft kernel has not been run against it (needs ≥16 GB host) |
| `cad-contract` (capabilities, errors, mesh, header) | **8** | 55 tests | security review of `serde` boundaries |
| Headless CLI (`atlas`) | **7** | 10 tests incl. a multi-process project workflow | no batch mode; no benchmark |
| Agent platform + geometry agent | **7** | 53 tests across api/runtime/geometry; grant, budget, quarantine, schema | in-process and unscheduled (S-8); no second agent or host (M9) |
| Engineering control plane (`atlas-eng`) | **7** | 30 tests; manifests validate with 0 errors; idempotent scans | no CI scheduling |
| Entitlements policy | **5** | 10 tests | no billing provider exists, by design |
| 2D drafting document: layers, point/line/circle/arc/polyline, undo, versioned save/load, DXF R12 output (new 2026-10-30) | **7** | 25 `cad-drafting` tests; CLI workflow (plan → DXF → byte-identical re-export); HTTP `/api/draft` and `/api/export/dxf`; browser plan panel, 24/27 assertions | no benchmark (rung 8); no security review of the DXF parser (rung 9); no interactive editor |
| DXF **input**: layer table, colours/off/locked, `$INSUNITS`, LINE/CIRCLE/ARC/POINT/POLYLINE/LWPOLYLINE, skipped-type report | **7** | 12 reader tests + a multi-process CLI import of a hand-written inch DXF; writer→reader→writer byte-identical | no fuzz corpus of real vendor files; TEXT/DIMENSION/HATCH/blocks/splines counted as skipped, not read |
| Plan drawing in the studio (SVG preview, counts, extents, DXF download) | **7** | `/api/draft` + `/api/export/dxf` under the token gate (401 without it), headless-browser checks in loopback and shared sessions | no drawing editor; one plan outline per project |
| 2D drafting (CADCraft), SolidCraft, imported kernels | **4** | code present and read; not built or tested on this host | everything from rung 5 upward; requires a ≥16 GB host |

## 3. Researched or planned, not implemented

| Item | Rung | Record |
|---|---|---|
| Pattern / mirror features | **2** | `docs/research/CAD_CAPABILITY_MATRIX.md` §2; now unblocked by the sketch plane |
| Multi-body document, assemblies, mates | **1–2** | `docs/research/INDUSTRY_TOOLSETS.md` §3B |
| Mass properties / interference | **2** | matrix §2; `Capability::MassProperties` declared, unadvertised |
| Fillet / chamfer / shell | **1** | present in SolidCraft's kernel, not in the contract; no accepted plan |
| Sweep / loft, partial revolve | **1–2** | contract declares `Sweep`/`Loft`, no adapter |
| STEP / 3MF / OBJ / glTF / IFC / GIS formats | **0–1** | matrix §3; nothing wired in the platform path. **DXF is the exception and is at rung 7** — see §2 |
| Drawing sheets: dimensions, text, title block, scale, PDF plot, batch plot | **1** | matrix §1; the drawing document and DXF exchange that this work needs now exist, the sheet does not |
| Industry workspaces (architecture, mechanical, electrical, MEP, plant, GIS, raster) | **1–2** | `docs/research/INDUSTRY_TOOLSETS.md`; **none implemented** |
| AI assistance (typed plan → preview → approval) | **1–2** | matrix §4; needs the command layer first |
| Plugin SDK / scripting / language compatibility | **1** | `docs/research/API_AND_PLUGIN_COMPATIBILITY.md` §5 |
| Cloud, collaboration, enterprise admin | **1** | `docs/tasks.md` §5 (deferred with reasons) |
| Resource profiles / diagnostics utility | **1** | `docs/platform/SYSTEM_REQUIREMENTS.md` §5–§6; nothing implemented |
| WASM engine build | **0** | target not installed; no measurement |

## 4. Release status

**Nothing is at rung 10.** The release gate (`scripts/gate.sh`) exists and is run on this commit, and a
deployable bundle and live simulation exist (`docs/operations/release-process.md`), but code signing,
notarisation and a published update channel are not implemented, and no build has been distributed.
For that reason every row above stops at rung 7 or 8 even where the workflow is verified end to end.

## 5. How to move a row up (and what would be dishonest)

* **Rung 7 → 8** needs a published number from a named harness on a named host, with the workload
  described well enough to repeat. An in-process rebuild time is not an HTTP request time; a
  debug-build figure is not a release figure.
* **Rung 8 → 9** needs the input and trust boundary reviewed: file parsing limits, path confinement,
  token comparison, error messages that do not leak content, and a dependency/licence check. Findings
  must be recorded even when unfixed.
* **Rung 9 → 10** needs the gate run on the shipped commit and a signed artefact.
* A feature is **not** promoted because a button exists, because a build passed, or because a
  comparison to a competitor looks favourable. The three "advertised only" capabilities in
  `engineering/manifest/capabilities.json` are deliberately left at that status for this reason.
