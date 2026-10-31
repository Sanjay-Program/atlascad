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
| Drawing **editor** in the studio: line/circle/polyline/point/dimension/text, layers, undo, delete, save (new 2026-10-30) | **7** | `/api/draft/entity`, `/remove`, `/layer`, `/undo`, `/redo`, `/save` under the token gate; HTTP test drives add → refuse → layer → remove → undo → redo → save → model edit; headless browser draws a line by clicking and saves it | no snapping, grips, trim/fillet, copy/rotate/scale or multi-select; no measurement of edit latency |
| Text and linear dimensions as drawing entities | **7** | 4 unit tests (validation, bounds, DXF exploded output, import of `TEXT`); rendered in the page and on the sheet | no rotation, radius/diameter/angular dimensions or dimension styles; imported `DIMENSION` records are skipped and counted |
| Sheet plot to PDF: A4–A0/Letter, scale or fit, frame, title block, overall dimensions | **7** | 10 layout tests + 5 PDF tests; `atlas sheet` writes a named A4 PDF in a CLI test; HTTP route tested including refusals; a rendered sheet inspected and its text extracted | no PDF/A (fonts are base-14, not embedded), one page per plot, no GD&T, BOM or sheet set; no plot-time measurement |
| Unified engineering model — Phase A: document/revision/fingerprint identity, stable artifact ids, validated 7-material library, revision-pinned derived artifacts, transitive staleness, explicit review, versioned atomic JSON (new 2026-10-31) | **7** | 14 unit tests + 1 doctest; 2 CLI workflow tests; 1 studio HTTP test; `docs/architecture/UNIFIED_ENGINEERING_MODEL.md` | rung 8 (no assessment benchmark); rung 9 (JSON trust boundary not reviewed); registry is a sidecar, not in `.zcad`; only the drawing is a real producer |
| `atlas artifacts check` release gate: exits 3 when a recorded drawing/simulation/toolpath belongs to an older revision (new 2026-10-31) | **7** | end-to-end CLI test: edit model → exit 3 → review both artifacts → exit 0, with source revisions unchanged | no CI wiring; no per-kind rule that forces recomputation instead of review |
| Studio drawing provenance: saved drawing reports `derived_from_revision` and becomes `requires_review` after a model edit; `/api/engineering` exposes document, revision, materials and assessment (new 2026-10-31) | **7** | HTTP test against the real server; 32-test studio API suite | no browser assertion for the new fields; no UI surface that presents the warning yet |
| Deployed studio: container image, hardened systemd unit, Caddy front, `build-image.sh --smoke` (new 2026-10-30) | **8** | image built and smoke-tested here (health, anonymous read 401, tokenised read 200, plotted PDF); the proxy shape run against the release binary with a real `Host` and an `https://` origin; `systemd-analyze verify` clean apart from the not-installed binary | rung 9–10 undone: no security review of the container surface, no signed artefact, no published channel. One token, all-or-nothing, no revocation without restart, no accounts, no multi-document |
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
| Drawing sheets beyond one titled page: multiple layouts, projected/section/detail views, GD&T, BOM, sheet sets, batch plot | **1** | matrix §1; a single titled page with the plan and overall dimensions now exists (rung 7) |
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
