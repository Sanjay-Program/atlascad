# Feature Matrix

**Status vocabulary — used strictly:**

| Status | Meaning |
|---|---|
| **Shipped** | Implemented in an application in this workspace, with tests in that repository (verified by reading; the full suites were not executed here — see `docs/build-and-test.md`). |
| **Shipped (verified here)** | Implemented **and** exercised on this machine by the new adapter's tests/benchmarks. |
| **Partial** | Implemented for a subset, or implemented with a measured defect. |
| **Planned** | Designed, not implemented. |
| **Absent** | Not implemented anywhere in this workspace. |
| **Blocked** | Cannot proceed without a decision, a missing repository, or unavailable tooling. |

No row claims AutoCAD/SolidWorks parity. Where a competitor comparison would be needed, the row says
so instead.

---

## 1. 2D drafting — CADCraft

Source of truth: `apps/cadcraft/README.md` "What works today" table plus the crate list. Statuses are
**Shipped** unless noted.

| Capability | Status | Note |
|---|---|---|
| Lines, polylines (arcs, widths), circles, arcs, rectangles, polygons, ellipses, splines, points, xlines/rays, donut | Shipped | `LINE, PLINE, CIRCLE, ARC, RECTANG, POLYGON, ELLIPSE, SPLINE, POINT, XLINE, RAY, DONUT` |
| Text / MTEXT with TrueType, single-stroke font, RTL | Shipped | `cadcraft-fonts`, `harfrust`, `unicode-bidi` |
| Layers, linetypes, lineweights, ACI + true colour | Shipped | Layer Properties Manager, isolate/freeze/lock/match |
| Object snaps (13 kinds), polar tracking, ortho, grid snap | Shipped | includes deferred tangent/perpendicular |
| Modify: erase, move, copy, rotate, scale, mirror, stretch, offset, trim, extend, fillet, chamfer, break, join, explode, array, draw order, overkill | Shipped | |
| Dimensions (all DIM*), dimension styles, associative dimensions, multileaders, tables | Shipped | |
| Hatch with islands, gradients, pattern library | Shipped | pick-point boundaries |
| Blocks, attributes, nested blocks | Shipped | |
| Paper space, viewports, page setups, plot, export PDF | Shipped | |
| Constraints (geometric + dimensional), expressions, AUTOCONSTRAIN, conflict detection | Shipped | `cadcraft-constraints`, 4 283 LOC |
| Undo/redo | Shipped | copy-on-write `EntityStore` snapshots (design read; not benchmarked) |
| Grips (stretch/move/rotate/scale/mirror) | Shipped | |
| DXF import (R12–2018 ASCII+binary) / export (R2000) | Shipped | round-trip coverage in tests; not re-run here |
| DWG import/export | Shipped | via `acadrust` (MPL-2.0), *"used unmodified"* |
| SVG/PNG export | Shipped | |
| MCP server + JSON control channel + CLI | Shipped | loopback control port |
| **Native DWG compatibility claim** | **Not verified** | No representative DWG corpus was tested here. Do not claim native DWG support publicly without one. |
| 3D modelling | **Absent** | CADCraft is a 2D drafting application; `cadcraft-geom` has `Vec3`/`Mat4` for transforms only, no B-rep. |

## 2. 3D modelling — SolidCraft

| Capability | Status | Note |
|---|---|---|
| Sketch + constraint solver | Shipped | `crates/sketch`, 23 681 LOC |
| Parametric timeline, feature history, incremental rebuild | Shipped | only changed features recompute |
| Extrude, revolve, sweep, loft, fillet, chamfer, shell, draft, holes, threads, patterns, mirror, split, combine | Shipped | in SolidCraft's kernel/engine; **not** in the new adapter beyond extrude |
| Assemblies, joints, motion, interference, mass properties | Shipped | |
| Sheet metal (flanges, hems, fold/unfold, flat patterns) | Shipped | |
| STEP, IGES, 3MF, STL, OBJ, glTF, URDF | Shipped | via `crates/io` + patched `truck-stepio` |
| STEP import of legacy encodings (GBK, Shift_JIS) | Shipped | `chardetng` + `encoding_rs` |
| Desktop app, web app, headless CLI, MCP server | Shipped | all run the same command registry |
| Correctness oracle over reference parts | Shipped (claim) | *"75 of 76 public reference parts match their analytic or FreeCAD-computed volume, area, centre of mass and bounding box"* — **read from README, not re-run** |
| WASM build | **Unverified** | `wasm32-unknown-unknown` target and `wasm-pack` not installed here; no build attempted |

## 3. The shared contract — what this milestone added

Verified on this machine (2026-10-22).

| Capability | Status | Evidence |
|---|---|---|
| Unit system with exact conversions + DXF `INSUNITS` both ways | **Shipped (verified here)** | `cad-contract` tests |
| Stable `EntityId`/`DocumentId` with zero-rejection and hex/JSON round trip | **Shipped (verified here)** | tests |
| Validated `Tolerance` (rejects NaN, ≤0, units mistakes) | **Shipped (verified here)** | tests |
| Capability discovery (`CapabilitySet`, `KernelInfo`) | **Shipped (verified here)** | tests; adapter advertises 4 of 19 |
| Kernel error taxonomy + `is_retryable` + `ErrorKind` | **Shipped (verified here)** | tests |
| `Mesh` interchange with `validate`, `open_edges`, `is_closed`, `signed_volume`, `surface_area`, `Aabb3` | **Shipped (verified here)** | tests incl. cube/open-patch/NaN/out-of-range |
| Versioned `DocumentHeader` with v0→v1→v2 migrations and refusal of newer schemas | **Shipped (verified here)** | tests: migration log, newer-schema refusal, corrupt file |
| Monotonic `Revision` with explicit overflow | **Shipped (verified here)** | test |
| `Kernel` trait: extrude, boolean, tessellate, release, volume, bbox | **Shipped (verified here)** | mock-kernel tests + truck adapter |
| Truck adapter: extrude | **Shipped (verified here)** | 148 753 boxes/s, exact volume |
| Truck adapter: boolean union | **Shipped (verified here)** | 1120 mm³ ± 0.004 |
| Truck adapter: boolean intersection | **Shipped (verified here)** | 40 mm³ |
| Truck adapter: boolean **difference** | **Blocked (kernel defect)** | returns 1053 mm³ for an 840 mm³ cut; refused by capability gate |
| Truck adapter: boolean **chains** (3+ operands) | **Blocked (kernel panic)** | third union panics inside truck; caught as `Internal` |
| Coplanar-face booleans without a workaround | **Partial** | fails; a ~1e-4 mm tool nudge makes them work (100/100 unions needed it) |
| Panic containment | **Shipped (verified here)** | real panic converted to `Internal` |
| Cancellation / timeouts | **Shipped (verified here)** | `Budget` with a deadline and a cancellation token on every operation; timeout, cancellation and "the instance survives" are tested (`t-cad-kernel-truck`) |
| Analytic boolean correctness corpus | **Shipped (verified here)** | 10 fixtures with a pass/fail table and an exit-code regression gate (`t-cad-kernel-truck-corpus`): 8 ok, 2 recorded known failures, 3 difference cases recorded unsupported |
| WASM target for the contract/adapter | **Absent / unverified** | no WASM build performed |
| STL read/write shared by CLI and app | **Shipped (verified here)** | `cad-io`: binary STL byte writer and validating reader, round-trip and malformed-input tests; not part of the `Kernel` trait |
| STEP through the contract | **Planned** | truck-stepio is available; not wired |
| Fillet/chamfer through the contract | **Planned** | present in SolidCraft's kernel, not in the contract |
| Document payload model (entities/features) | **Partial (verified here)** | `cad-document` stores one sketch (rectangle/circle/polygon), one extrude feature, named parameters, revision, and bounded undo history; no general entity graph, constraints, or multi-feature timeline |
| Undo/redo through the contract | **Planned** | each app has its own; no shared transaction type yet |

## 3b. The agent platform — what this milestone added

| Capability | Status | Evidence |
|---|---|---|
| Versioned agent contract: identity, metadata, capabilities, I/O schemas, budgets, error taxonomy | **Shipped (verified here)** | `cad-agent-api`, 13 tests (`t-cad-agent-api`) |
| Agent lifecycle: `discovered → registered → ready`, with refusal to activate an agent whose contract tests failed | **Shipped (verified here)** | `cad-agent-runtime`, 16 tests (`t-cad-agent-runtime`) |
| Capability-based permissions: effective grant = declaration ∩ owner decision; `shell`/`secrets` owner-gated | **Shipped (verified here)** | runtime tests + the geometry agent's own grant test |
| Budget enforcement from outside the agent: call count, output size, wall clock | **Shipped (verified here)** | runtime tests; the agent cannot widen its own budget |
| Strict JSON Schema subset with refusal of unenforced keywords | **Shipped (verified here)** | `schema.rs`; `type`, `enum`, `required`, `properties`, `items`, `minItems`, `maxItems`, `minLength`, `maxLength`, `minimum`, `maximum` |
| Agent quarantine on panic | **Shipped (verified here)** | runtime test |
| First real agent: extrude, boolean, tessellate, volume, bounding box, release | **Shipped (verified here)** | `cad-agent-geometry`, 24 tests (`t-cad-agent-geometry`) |
| Bounded, confined file read inside an agent workspace | **Shipped (verified here)** | single path component, no separators, no `..`, symlink-resolved path must stay inside the workspace, 1 MiB cap |
| Import/export agent | **Planned** | declared, not implemented (`agents.json`, `implemented: false`) |
| Design-rule-check agent | **Planned** | declared, not implemented |
| Natural-language-to-CAD agent | **Planned** | declared, not implemented; needs a model provider and a credential policy |
| Scheduled or isolated agent execution | **Absent** | the registry is synchronous and in-process; process isolation for third-party agents is recorded as the strategy, not implemented |
| Agents in either desktop application | **Absent** | neither app depends on the runtime yet |

## 3c. Atlas Studio — the local application added 2026-10-25

Verified on this machine by `cargo test -p cad-studio` (HTTP integration tests) and by exercising the
running app in a browser at 392 px and full width.

| Capability | Status | Evidence / limit |
|---|---|---|
| Local project create/open/save as versioned `.zcad` JSON | **Shipped (verified here)** | `Project::save`/`load`, CLI `--new`/`--file`, API `/api/save`; save is atomic via a temporary file |
| Rectangle, circle, regular-polygon sketch profiles | **Shipped (verified here)** | profile switch API and UI; polygon sides 3–64; profile validation rejects degenerate loops |
| Named parameters (width, height, depth) with re-evaluation | **Shipped (verified here)** | dependency-aware parameter map; 0.1–10,000 mm bounds; invalid values rejected with a typed error |
| Single extrusion feature | **Shipped (verified here)** | through `cad-kernel-truck`; no booleans, revolve, loft, fillet, or multi-body |
| Feature tree and selection | **Shipped (verified here)** | sketch + feature nodes, click to select, properties follow selection |
| 3D viewport: orbit, zoom, pan, fit, front/top/right views | **Shipped (verified here)** | WebGL 1, per-face highlight; frame/grid/wireframe toggles |
| Face picking and inspection | **Shipped (verified here)** | ray-cast against the tessellation; hit point, normal, and Z readout in the status bar |
| Undo / redo | **Shipped (verified here)** | bounded snapshot history in the document; UI buttons, shortcuts, and palette commands |
| Command palette | **Shipped (verified here)** | project, sketch, view, parameter, theme, and help commands; disabled commands are visibly disabled, not fake |
| STL export | **Shipped (verified here)** | `/api/export/stl` through `cad-io`, validated mesh before writing |
| Light/dark theme, responsive drawers, keyboard help | **Shipped (verified here)** | persisted theme choice; inspectors accessible below 980 px |
| Diagnostics log and typed API errors | **Shipped (verified here)** | request failures carry codes (`invalid_request`, `invalid_parameter`, `not_found`, `export_failure`, …) |
| Sketch constraints, dimensions, arcs/splines | **Absent** | Stage A remainder |
| Multiple features, booleans, revolve | **Absent** | Stage A/B remainder |
| STL/STEP/IGES/DXF import | **Absent** | `cad-io` can read binary STL, but no application importer is wired |
| Assemblies, drawings, mass properties UI, materials | **Absent** | later milestones |
| Autosave, crash recovery, file dialogs | **Absent** | save is explicit; the project path is a command-line argument |
| Accounts, cloud sync, collaboration, telemetry | **Absent** | no network calls beyond the loopback browser connection |
| Native desktop shell | **Absent** | see ADR-0008 |

## 4. Platform capabilities (brief §16–§19)

| Capability | Status | Note |
|---|---|---|
| AI assistant over structured CAD commands | **Shipped** (both apps) | SolidCraft has an in-app agent chat + ACP support; CADCraft has an MCP server. Both are command-typed, not code execution. |
| MCP server | **Shipped** (both) | SolidCraft: `solidcraft-cli mcp`; CADCraft: `cadcraft-cli mcp` |
| Collaboration (Yrs/Yjs, multi-user) | **Absent** | correctly deferred; single-user persistence is not stable yet |
| SaaS backend (auth, Postgres, object storage, billing) | **Absent** | no server crate exists in either app; no cloud dependency added |
| Multi-tenant isolation / RBAC / audit logs | **Absent** | |
| Rate limiting on expensive operations | **Absent** | |
| WASM/browser build | **Unverified** | web apps exist in source (`solidcraft-web`), not built here |
| Native packaging (MSI, DMG, AppImage, Snap, winget, Flatpak) | **Shipped** (source + CI configs) | `packaging/` + workflows present; not executed here |
| CI | **Shipped** (source) | CADCraft: 5 workflows; SolidCraft: 13 workflows |
| Benchmarks in CI | **Absent** | the harness in this repo is manual |
| Dependency/advisory gates in CI | **Absent / partial** | SolidCraft has `deny.toml`; CADCraft has none; no advisory scan was run here |

## 5. What must not be claimed publicly today

1. **"Native DWG compatibility."** Untested against representative files.
2. **"AutoCAD/SolidWorks parity."** No comparative benchmark exists; the brief forbids the claim.
3. **"Runs in the browser."** The source and CI suggest it; no WASM build was performed here.
4. **"AI can model anything."** The command surface is bounded; failure modes and limits were not
   characterised.
5. **"Booleans work."** Union/intersection do; **difference and boolean chains do not** through the
   shared adapter. The corpus records two further failures — tangent contact and coordinates near
   1e6 mm — that the feature matrix previously did not know about.
6. **"Zero dependencies on native code."** True for truck, false for `acadrust`-based DWG or anything
   OCCT-based.
7. **"The agent platform is production-ready."** One agent exists and runs; there is no scheduler,
   no persistence of registrations, no process isolation for third-party code, and no second agent to
   show that the contract is not accidentally geometry-specific.
8. **"Atlas Studio is a CAD product."** It is a verified single-feature local workflow: one sketch,
   one extrusion, parameters, undo, save, and STL export. It has no constraints, booleans, drawings,
   assemblies, collaboration, or cloud services, and its browser shell is not a native desktop app.
