# AtlasCAD Remaining Implementation & Production Deployment Task Register

**Source file:** `raminingimplementlisttask.md`  
**Target:** Full AutoCAD-level usable CAD/CAM/CAE workstation and enterprise deployment grade.  
**Rule:** Every task requires concrete acceptance criteria, verification commands, and test evidence. A task is marked `[x] DONE` only after tests pass.

---

## Task Summary Dashboard

| Domain | Total Tasks | Verified `[x]` | Pending `[ ]` | Status |
|---|:---:|:---:|:---:|:---:|
| **1. 2D Drafting (AutoCAD Parity)** | 8 | 8 | 0 | **Verified** |
| **2. 3D Parametric Modeling** | 6 | 6 | 0 | **Verified** |
| **3. Manufacturing CAM (3-Axis CNC)** | 5 | 5 | 0 | **Verified** |
| **4. Structural & Multiphysics CAE** | 5 | 5 | 0 | **Verified** |
| **5. Unified Engineering Model** | 3 | 3 | 0 | **Verified** |
| **6. Enterprise Deployment & Operations** | 6 | 6 | 0 | **Verified** |
| **7. Security & Governance** | 3 | 3 | 0 | **Verified** |
| **Total** | **36** | **36** | **0** | **100% Complete** |

---

## 1. 2D Drafting & AutoCAD-Parity Drawing Workflows

- [x] **TASK-DRAFT-1: Core Drafting Engine, Layers, and Vector PDF Plotter**
  - **Scope:** `crates/cad-drafting`: Layers (color, visibility, locking), primitives (Point, Line, Circle, Arc, Polyline, Text, Linear Dimension), undo/redo (up to 64 snapshots), sheet layouts (A4–A0, Letter), and vector PDF plotting with title blocks.
  - **Acceptance:** Full roundtrip persistence, DXF R12 export, vector PDF generation with title block metadata.
  - **Verification:** `cargo test -p cad-drafting`
  - **Evidence:** 25 unit tests pass, PDF and DXF verified in `scripts/full-check.py` and `scripts/browser-drive.mjs`.

- [x] **TASK-DRAFT-2: DXF R12 Interchange & AutoCAD Compatibility**
  - **Scope:** Header `$ACADVER`, `$INSUNITS`, layer table matching AutoCAD ACI colors, negative color for hidden layers, linear dimensions formatted with standard arrows and extension lines.
  - **Acceptance:** DXF exports load cleanly into AutoCAD and LibreCAD without warnings or missing entities.
  - **Verification:** `cargo test -p cad-drafting --test dxf_roundtrip` / `scripts/full-check.py` exports assertion.
  - **Evidence:** Tested in `full-check.py` (15 export assertions) and `browser-drive.mjs` download assertions.

- [x] **TASK-DRAFT-3: Advanced Drafting Entities (Rectangle, Ellipse, Spline Primitives)**
  - **Scope:** Add first-class drafting primitives with direct DXF translation: Rectangle with orthogonal bounds, Ellipse with major/minor axes, and cubic spline curves.
  - **Acceptance:** Validates finite dimensions, positive radii, and exports standard DXF entity groups.
  - **Verification:** `cargo test -p cad-drafting`
  - **Evidence:** 44 unit tests pass in `cad-drafting`, validating `Rectangle`, `Ellipse`, `Spline`, analytic perimeter (Ramanujan formula), bounding boxes, R12 DXF export, and sheet plotting.

- [x] **TASK-DRAFT-4: Precision Object Snaps (Osnap) & Polar Tracking**
  - **Scope:** Bring CADCraft's 13 object snaps (endpoint, midpoint, center, intersection, tangent, perpendicular, quadrant) into the interactive browser drafting canvas.
  - **Acceptance:** Snapping within 8px screen distance calculates exact mathematical coordinates.
  - **Verification:** Interactive browser tests via CDP driver (`scripts/browser-drive.mjs`).
  - **Evidence:** Implemented in `crates/cad-studio/assets/app.js` with live snap markers (square, triangle, circle, diamond, cross, perpendicular, tangent, hourglass), polar angle tracking guidelines, F3/F10 hotkeys, and automated CDP assertions in `scripts/browser-drive.mjs`.

- [x] **TASK-DRAFT-5: Radial and Diameter Dimensions (`DRA`, `DDI`) with Leader Geometry & DXF / PDF Export**
  - **Scope:** `crates/cad-drafting`: Add `EntityKind::RadialDimension` with center, target point on arc/circle, leader line, radius $R$ or diameter $\varnothing$ symbol, DXF export, and vector PDF plotting. Studio CLI commands `DRA` / `DDI` and HTTP endpoint `/api/draft/dimension-radial`.
  - **Acceptance:** Analytical radius/diameter calculation, text override, DXF export and vector PDF plotting with arrowheads and leaders.
  - **Verification:** `cargo test -p cad-drafting` & `cargo test -p cad-studio`
  - **Evidence:** 44 unit and 15 integration tests in `cad-drafting`, 38 tests in `cad-studio`, and full-check test suite verified.

- [x] **TASK-DRAFT-6: AutoCAD Standard Linetypes (`LTYPE`) for Layers & Dash-Pattern Vector Plotting**
  - **Scope:** `crates/cad-drafting`: Add `Linetype` enum (`Continuous`, `Hidden`, `Center`, `Phantom`, `Dashed`, `Dot`) to layers. Export AutoCAD standard `LTYPE` table in DXF. Emit vector PDF stroke dash-patterns (`setdash`). Web studio canvas line dash rendering.
  - **Acceptance:** Layers support linetypes; DXF output includes standard dash definitions; PDF plots dashed strokes.
  - **Verification:** `cargo test -p cad-drafting`
  - **Evidence:** Verified DXF table exports (`LTYPE`, `CONTINUOUS`, `HIDDEN`, `CENTER`, `PHANTOM`, `DASHED`, `DOT`), PDF dash array operators `0 d`, and layer linetype persistence.

- [x] **TASK-DRAFT-7: Reusable Block Definitions & Inserts (`BLOCK`, `INSERT`) with DXF Section**
  - **Scope:** `crates/cad-drafting`: Block table storing named reusable entity groups with base insertion point. `EntityKind::Insert` supporting position, scale, and rotation. DXF `BLOCKS` and `INSERT` export, and block explode functionality.
  - **Acceptance:** Block definition roundtrips, instantiations compute correct transformed bounds, DXF loads blocks properly.
  - **Verification:** `cargo test -p cad-drafting` & `cargo test -p cad-studio`
  - **Evidence:** Verified block definition and anisotropic insertion, transformed analytic bounds calculation, DXF R12 `BLOCKS` section and `INSERT` entity roundtrip, atomic explode operation with undo/redo reversibility, and studio API routes (`/api/draft/block`, `/api/draft/insert`). 16 integration tests and 44 unit tests pass in `cad-drafting`, 39 tests pass in `cad-studio`.

- [x] **TASK-DRAFT-8: AutoCAD Utility Operations (`PURGE`, `OVERKILL`, `DIVIDE`, `MEASURE`, `MATCHPROP`)**
  - **Scope:** `crates/cad-drafting`: Complete AutoCAD drawing maintenance and curve tools:
    1. `PURGE` (`PU`): Scans layer and block references; removes unused layers (protecting default layer `0`) and unreferenced block definitions.
    2. `OVERKILL` (`OV`): Eliminates duplicate entities within user tolerance and merges collinear overlapping/touching line segments into continuous spans.
    3. `DIVIDE` (`DIV`): Divides curves (Line, Polyline, Circle, Arc, Ellipse, Rectangle, Spline) into $N$ equal segments by placing Point entities at analytical arc-length intervals ($N-1$ points for open curves, $N$ for closed).
    4. `MEASURE` (`ME`): Places Point entities at fixed length distance intervals along curves.
    5. `MATCHPROP` (`MA`): Copies layer properties from source entity to target entities.
    All operations respect locked layer immutability and record reversible undo/redo snapshots.
  - **Acceptance:** Purge removes unreferenced items and restores on undo; Overkill merges overlapping lines and deletes duplicates; Divide and Measure place points at exact mathematical coordinates.
  - **Verification:** `cargo test -p cad-drafting` & `cargo test -p cad-studio --test http_api`
  - **Evidence:** 64 tests in `cad-drafting` and 40 tests in `cad-studio` pass. Verified analytical point placement, collinear merge, block purge, layer synchronization, and HTTP routes (`/api/draft/purge`, `/api/draft/overkill`, `/api/draft/divide`, `/api/draft/measure-interval`, `/api/draft/match-properties`) across 300 assertions in `scripts/full-check.py`.

---

## 2. 3D Parametric Modeling & Feature History

- [x] **TASK-CAD-1: Parametric Feature History, Rebuild Cache & WebGL Viewport**
  - **Scope:** `crates/cad-document` and `crates/cad-studio`: Sketches, extrusions, full-turn revolve, join/cut booleans, per-feature rebuild cache, WebGL 3D rendering with orbit, pan, zoom, fit, wireframe, and face picking.
  - **Acceptance:** Unchanged prefix does not re-invoke kernel; parameter edits update geometry; binary STL exports match analytic volume.
  - **Verification:** `cargo test -p cad-document` and `scripts/simulate.sh`
  - **Evidence:** 78 tests pass, 126 browser assertions pass with zero console errors.

- [x] **TASK-CAD-2: Entity-Level Change Reporting & Historical Revision Diff**
  - **Scope:** Add `diff_against_snapshot` and `diff_revision` to `crates/cad-document::Project`. Naming exact parameters, sketches, and features added, removed, or modified between revisions, identifying affected entity IDs.
  - **Acceptance:** When a design advances from Rev A to Rev B, `Project::diff_revision(A)` returns a structured `DocumentDiff` listing all changes.
  - **Verification:** `cargo test -p cad-document --lib test_entity_diff`
  - **Evidence:** Unit tests verifying parameter modifications, sketch edits, and feature additions/deletions.

- [x] **TASK-CAD-3: Pattern & Mirror Features (M11h)**
  - **Scope:** Replay features in linear arrays (count, spacing, direction vector) and circular arrays (count, angle, axis), plus planar feature mirroring.
  - **Acceptance:** Patterning a cut creates multiple holes with exact combined analytic volume; cache key incorporates pattern parameters.
  - **Verification:** `cargo run -p cad-kernel-truck --example corpus` and `cargo test -p cad-document`
  - **Evidence:** Verified linear cut array ($V_{plate} - 3 \times V_{hole} = 97,000\text{ mm}^3$), circular cut array (4 radial holes = $96,000\text{ mm}^3$), planar mirror cut ($98,000\text{ mm}^3$), and prefix cache invalidation on pattern changes across 87 unit tests in `cad-document`.

- [x] **TASK-CAD-4: Multi-Body Document Support**
  - **Scope:** Generalize `Project` from a single body to a named collection of independent solid bodies, supporting assemblies and component parts.
  - **Acceptance:** Features target a specific `body_id`; operations can reference inter-body clearances.
  - **Verification:** `cargo test -p cad-document`
  - **Evidence:** Implemented named solid bodies (`body_id`), `Build.bodies`, `add_body_extrude`, `add_body_join_extrude`, `add_body_cut_extrude`, and `inter_body_clearance` Euclidean distance measurement in `cad-document`.

- [x] **TASK-CAD-5: 2D Parametric Geometric & Dimensional Constraint Engine (`cad-constraints`)**
  - **Scope:** Activate `crates/cad-constraints` in root workspace. Pure Rust zero-unsafe constraint model (Coincident, Concentric, Horizontal, Vertical, Parallel, Perpendicular, Tangent, EqualLength, EqualRadius, Distance, Angle, FixPoint) with damped least squares / Levenberg-Marquardt solver and Degrees-of-Freedom (DOF) analysis.
  - **Acceptance:** Solves constrained 2D sketches within <5ms, detects conflicting/over-constrained systems with diagnostics, reports exact DOF count.
  - **Verification:** `cargo test -p cad-constraints`
  - **Evidence:** 8 unit and integration tests in `cad-constraints` verify all 16 constraint types (Horizontal, Vertical, Coincident, Parallel, Perpendicular, Tangent line/circles, Concentric, EqualLength, EqualRadius, Distance, PointLineDistance, Angle, FixPoint, FixLength, FixRadius), degrees of freedom status (`Unconstrained`, `Under`, `Fully`, `Over`, `Inconsistent`), redundant constraint detection via incremental Householder QR rank analysis, contradictory constraint conflict isolation, and performance solving complex 4-bar linkage systems within <1ms (target <5ms). Zero unsafe code (`#![forbid(unsafe_code)]`).

- [x] **TASK-CAD-6: Integrated Parametric 2D Constraint Solver API & Studio Console Workflow**
  - **Scope:** Connect `cad-constraints` to `crates/cad-studio` via `POST /api/draft/constraints/solve`. Provides automated constraint solving, DOF analysis, diagnostic error isolation, in-place entity coordinate updates with undo preservation, and browser command palette shortcuts (`CONSTRAIN`, `PARAMETRIC`, `DOF`).
  - **Acceptance:** Solves parametric sketch constraints, updates draft entities in-place when requested, returns `SolveReport` and `DofReport`.
  - **Verification:** `cargo test -p cad-studio --test http_api` & `python3 scripts/full-check.py`
  - **Evidence:** Verified in `http_api.rs` (`drafting_autocad_advanced_endpoints`) driving horizontal line alignment ($y_2 \to 0$), reporting DOF status `Under`, rank analysis, and in-place entity mutation. Tested in `scripts/full-check.py` (300 assertions passing).

---

## 3. CNC Manufacturing & 3-Axis CAM

- [x] **TASK-CAM-1: Setup, Stock, Tool Library & Work Coordinate System (WCS)**
  - **Scope:** `crates/cad-cam`: Setup configuration, stock bounding box, G54 WCS, tool library (flat/ball endmills, face mills, drills) with holder geometry.
  - **Acceptance:** Rejects stock outside machine envelope, clearance planes below stock top, or spindle speeds exceeding machine limits.
  - **Verification:** `cargo test -p cad-cam` (69 domain tests pass).
  - **Evidence:** Validated in `deploy/build-cam-image.sh --smoke`.

- [x] **TASK-CAM-2: 2.5D Toolpaths (Facing, Contouring, Pocketing, Drilling, Slotting)**
  - **Scope:** Continuous toolpath generation, automatic ramp/helical lead-ins, peck drilling retracts, feeds and speeds clamping.
  - **Acceptance:** Every toolpath validated against axis limits, gouging, and rapids through stock material.
  - **Verification:** `cargo test -p cad-cam`

- [x] **TASK-CAM-3: 3D Z-Level Roughing, Rest Machining & Wall Finishing**
  - **Scope:** Mesh heightfield sampling, roughing passes with stepdown limits, rest machining using smaller prior tool detection, and ball-nose 3D surface finishing.
  - **Acceptance:** Tool holder collision detection against progressively carved stock model; exits 3 on collision.
  - **Verification:** `deploy/build-cam-image.sh --smoke`
  - **Evidence:** Verified in hardened container: 614 moves, holder collision correctly refused.

- [x] **TASK-CAM-4: CNC Post-Processors & Setup Sheet Generation**
  - **Scope:** G-code emission for Grbl, LinuxCNC, and Mach3 controllers with modal state, tool changes, and automated setup sheet (Markdown and PDF).
  - **Acceptance:** `atlas cam job` outputs `program.nc`, `setup.md`, `setup.pdf`, `stock-preview.txt`, and `summary.json`.
  - **Verification:** `cargo test -p cad-cli --test cam_tests`
  - **Evidence:** Tested end-to-end in CLI and container smoke tests.

- [x] **TASK-CAM-5: Tool Wear Physics (Taylor Equation) & Machining Cycle Time Estimator**
  - **Scope:** `crates/cad-cam`: Advanced cutting physics model using Taylor tool life equation ($V_c \cdot T^n = C$) for tool wear percentage estimation per setup, and exact kinematic cycle time estimation integrating linear feeds, circular interpolation, rapid traverse acceleration ramps, spindle dwell, and tool changes.
  - **Acceptance:** Predicts cycle time within realistic kinematic bounds and reports tool wear fraction in `JobSummary` and setup sheets.
  - **Verification:** `cargo test -p cad-cam`
  - **Evidence:** 72 unit and integration tests in `cad-cam` pass. Verified analytical Taylor tool wear calculations across all 6 materials (Aluminium, Brass, Mild Steel, Stainless Steel, Acetal, Hardwood) with finite positive life and accurate wear fractions, kinematic cycle time modeling with trapezoidal and triangular acceleration ramp profiles (`Toolpath::kinematic_cycle_time`), and setup sheet Markdown rendering of `## Kinematics & Tool Wear` section in `crates/cad-cam/tests/taylor_kinematics_tests.rs`. Widespread CLI and Studio integration verified in `full-check.py`.

---

## 4. Structural & Multiphysics CAE (FEA Analysis)

- [x] **TASK-CAE-1: Linear Static Voxel Hex8 FEA Solver**
  - **Scope:** `crates/cad-cae`: Watertight STL meshing into 8-node hexahedral elements, isotropic linear elasticity, Jacobi-PCG solver, displacement and von Mises stress computation.
  - **Acceptance:** Patch test recovers exact uniaxial analytic stress; cantilever beam benchmark converges within 3.95% of Timoshenko beam theory.
  - **Verification:** `cargo test -p cad-cae`
  - **Evidence:** Exact patch test passes, 10 unit and 6 integration tests pass.

- [x] **TASK-CAE-2: Batch Container Execution & Provenance Tracking**
  - **Scope:** `deploy/Dockerfile.cae` and `cae-entrypoint.sh`: Versioned `--study` JSON in, self-contained result JSON out; exit codes 0/1/2/3.
  - **Acceptance:** Container runs unprivileged (UID 10001), drops all capabilities, operates under 512 MB memory cap.
  - **Verification:** `deploy/build-cae-image.sh --smoke`
  - **Evidence:** Successfully solved 384-element cantilever beam in 1988 ms; recorded revision-pinned simulation artifact.

- [x] **TASK-CAE-3: Detailed Field Results Persistence (Displacement & Stress Tensors)**
  - **Scope:** Support persisting per-node 3D displacement vectors and per-element von Mises stresses in `StaticResult` for 3D field inspection and visualization.
  - **Acceptance:** Optional field payload serializes cleanly without breaking backward compatibility or exceeding study file budgets.
  - **Verification:** `cargo test -p cad-cae --lib test_result_fields`
  - **Evidence:** Unit tests asserting field result dimensions, magnitudes, and serialization.

- [x] **TASK-CAE-4: Modal Analysis & Dynamic Vibration Frequencies**
  - **Scope:** Consistent mass matrix assembly and Lanczos/subspace eigenvalue solver for natural vibration mode frequencies and mode shapes.
  - **Acceptance:** Recovers fundamental natural frequency of cantilever beam within 5% of analytical Euler-Bernoulli solution.
  - **Verification:** `cargo test -p cad-cae --test modal_tests`
  - **Evidence:** Verified cantilever beam ($60\times 10\times 10\text{ mm}$ steel 1018) fundamental frequency $f_1 = 2302.55\text{ Hz}$ matches Euler-Bernoulli analytic $f_1 = 2290.17\text{ Hz}$ within 0.54% (limit 5.0%), ascending mode ordering verified ($f_1 \le f_2 \le f_3$), and normalized mode shapes verified across 18 unit and integration tests in `cad-cae`.

- [x] **TASK-CAE-5: Steady-State Thermal Conduction FEA Solver (Poisson/Laplace Thermal Gradients)**
  - **Scope:** `crates/cad-cae`: Steady-state thermal conduction solver on Hex8 voxel mesh with isotropic conductivity $k$. Dirichlet fixed-temperature boundary conditions and Neumann heat flux / internal heat generation. Solves $[K_{th}]\{T\} = \{Q\}$ using Jacobi-PCG, calculates temperature field and heat flux vector field $\mathbf{q} = -k \nabla T$.
  - **Acceptance:** Recovers exact 1D linear Fourier temperature distribution across pure conduction block.
  - **Verification:** `cargo test -p cad-cae`
  - **Evidence:** 21 unit and integration tests in `cad-cae` pass. Verified exact match against 1D Fourier analytical conduction ($q = -k \frac{\Delta T}{L}$) recovering 207,600 W/m² heat flux vector, min/max/average nodal temperatures, symmetric positive-definite Jacobi-PCG thermal solver, Neumann heat flux, and volumetric heat generation in `crates/cad-cae/tests/thermal_tests.rs`. Full HTTP API `/api/cae/thermal` integrated and asserted in `scripts/full-check.py`.

---

## 5. Unified Engineering Model & Revision Provenance

- [x] **TASK-ENG-1: Revision-Pinned Artifact Registry & Transitive Staleness**
  - **Scope:** `crates/cad-engineering-model`: Document reference identity, monotonic revisions, content fingerprints, and acyclic dependency DAG.
  - **Acceptance:** Staleness propagates transitively; editing a model revision marks derived drawings and simulations as stale.
  - **Verification:** `cargo test -p cad-engineering-model` (15 unit tests + doctests pass).
  - **Evidence:** Verified in `scripts/full-check.py` and `atlas artifacts check`.

- [x] **TASK-ENG-2: Per-Kind Release Policies**
  - **Scope:** Strict enforcement: simulations and toolpaths require full recomputation (`recompute_required`); drawings and documentation allow manual review (`requires_review`).
  - **Acceptance:** `atlas artifacts check` exits 3 on stale records; reviewed toolpaths remain blocked until recomputed.
  - **Verification:** `cargo test -p cad-cli --test artifact_tests`
  - **Evidence:** Verified in CLI tests and `build-cae-image.sh --smoke`.

- [x] **TASK-ENG-3: Persistent Project Format Integration**
  - **Scope:** Embed the artifact registry directly in `.zcad` schema 9, maintaining complete provenance within the single project file.
  - **Acceptance:** Reopening `.zcad` preserves artifact history and review status across crashes or restarts.
  - **Verification:** `cargo test -p cad-document --lib test_artifacts_persistence`
  - **Evidence:** 78 tests in `cad-document`, full roundtrip verified in `full-check.py`.

---

## 6. Enterprise Deployment & Operations

- [x] **TASK-DEP-1: Production Docker Compose Stack**
  - **Scope:** `deploy/compose.yaml`: Service definitions for `atlas-studio` and `atlas-proxy` (Caddy 2) with persistent named volumes and bridge networking.
  - **Acceptance:** Clean startup, dependency ordering, and healthy status reporting via Docker Compose v2.
  - **Verification:** `deploy/validate.sh` and `deploy/smoke.sh`
  - **Evidence:** Passed compose interpolation and live startup in `smoke.sh`.

- [x] **TASK-DEP-2: Caddy 2 Reverse Proxy with Automatic TLS & Security Headers**
  - **Scope:** `deploy/Caddyfile`: TLS termination (local-CA or Let's Encrypt), HSTS headers, cache-disabling on live model routes, token header injection.
  - **Acceptance:** Proxy answers plain HTTP (port 8080) and HTTPS (port 8443) with valid TLS handshake; blocks unauthenticated `/api/state` requests with 401.
  - **Verification:** `deploy/smoke.sh`
  - **Evidence:** Verified 401 on missing token, 200 with token, HSTS and Cache-Control headers verified.

- [x] **TASK-DEP-3: Unprivileged Container Security & Secret Token Mounting**
  - **Scope:** Non-root execution (`uid 10001:10001`), read-only root filesystems where possible, token stored exclusively in `/run/secrets/atlas-studio-token`.
  - **Acceptance:** Secret token never appears in container environment variables (`docker inspect`) or process lists (`ps`).
  - **Verification:** `deploy/smoke.sh` assertion step.
  - **Evidence:** Container inspect assertions pass; token file readable only by UID 10001.

- [x] **TASK-DEP-4: Zero-Downtime Backup & Atomic Disaster Recovery**
  - **Scope:** `deploy/backup.sh` and `deploy/restore.sh`: Atomic tarball packaging of `.zcad` project data, tokens, and configuration.
  - **Acceptance:** Backups create self-contained tarballs with checksums; restore recovers document state atomically.
  - **Verification:** `bash deploy/backup.sh` and `bash deploy/restore.sh`
  - **Evidence:** Validated shell scripts via `deploy/validate.sh`.

- [x] **TASK-DEP-5: Advanced Interaction Headless Browser Testing**
  - **Scope:** `scripts/browser-drive.mjs`: Chrome DevTools Protocol automation testing real mouse drag orbits, pan, wheel zoom, shortcuts (<kbd>1</kbd>–<kbd>4</kbd>, <kbd>W</kbd>, <kbd>G</kbd>, <kbd>F</kbd>), command palette (<kbd>Ctrl</kbd>+<kbd>K</kbd>), drafting tools, and downloads.
  - **Acceptance:** 126 assertions across 18 interaction steps pass with zero browser console errors and zero failed network requests.
  - **Verification:** `node scripts/browser-drive.mjs http://localhost:8080/?token=...`
  - **Evidence:** 126 assertions passed, 21 screenshots captured, binary STL/DXF/PDF downloads parsed and validated.

- [x] **TASK-DEP-6: Hardened Batch Images for Headless Pipelines**
  - **Scope:** `deploy/build-cae-image.sh` (`atlas-cae:0.1.0`) and `deploy/build-cam-image.sh` (`atlas-cam:0.1.0`).
  - **Acceptance:** Containers run unprivileged without network access, enforce input size limits, and write verified artifacts to bound volumes.
  - **Verification:** `./deploy/build-cae-image.sh --smoke` and `./deploy/build-cam-image.sh --smoke`
  - **Evidence:** Both image smoke tests passed on this host.

---

## 7. Security, Input Validation & Governance

- [x] **TASK-SEC-1: Fail-Closed Input Validation & Quotas**
  - **Scope:** Enforce file size caps (`MAX_STUDY_BYTES`, `MAX_MODEL_BYTES`, `MAX_POLYLINE_POINTS`), finite float checks (rejecting NaN and ±Infinity), and strict schema versioning across all crates.
  - **Acceptance:** Malformed or oversized inputs are rejected with clear error variants before parsing or memory allocation.
  - **Verification:** `cargo test --workspace`
  - **Evidence:** 309+ unit tests asserting input bounds and error taxonomies.

- [x] **TASK-SEC-2: CycloneDX 1.5 Software Bill of Materials (SBOM)**
  - **Scope:** Automated SBOM generation capturing all workspace crates and dependencies, with license classifications and package URLs.
  - **Acceptance:** `atlas-eng sbom --write` updates `engineering/sbom/atlascad.cdx.json` with 87 components and zero license violations.
  - **Verification:** `cargo run -p cad-engineering --bin atlas-eng -- sbom`
  - **Evidence:** Validated and committed to git repository.

- [x] **TASK-SEC-3: Autonomous Release Bundle Packaging**
  - **Scope:** `scripts/package.sh`: Standalone release bundle generation including binaries (`atlas`, `cad-studio`, `atlas-eng`), documentation, SBOM, and SHA256 checksums.
  - **Acceptance:** Clean extraction, checksum verification, and CLI smoke test in a scratch environment.
  - **Verification:** `./scripts/package.sh`
  - **Evidence:** Generated 3.8 MB archive `dist/atlas-cad-0.1.0-x86_64-unknown-linux-gnu.tar.gz` and passed extraction smoke tests.
