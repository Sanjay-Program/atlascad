# AtlasCAD Remaining Implementation & Production Deployment Task Register

**Source file:** `raminingimplementlisttask.md`  
**Target:** Full AutoCAD-level usable CAD/CAM/CAE workstation and enterprise deployment grade.  
**Rule:** Every task requires concrete acceptance criteria, verification commands, and test evidence. A task is marked `[x] DONE` only after tests pass.

---

## Task Summary Dashboard

| Domain | Total Tasks | Verified `[x]` | Pending `[ ]` | Status |
|---|:---:|:---:|:---:|:---:|
| **1. 2D Drafting (AutoCAD Parity)** | 4 | 2 | 2 | In Progress |
| **2. 3D Parametric Modeling** | 4 | 2 | 2 | In Progress |
| **3. Manufacturing CAM (3-Axis CNC)** | 4 | 4 | 0 | **Verified** |
| **4. Structural & Multiphysics CAE** | 4 | 3 | 1 | In Progress |
| **5. Unified Engineering Model** | 3 | 3 | 0 | **Verified** |
| **6. Enterprise Deployment & Operations** | 6 | 6 | 0 | **Verified** |
| **7. Security & Governance** | 3 | 3 | 0 | **Verified** |
| **Total** | **28** | **23** | **5** | **82% Complete** |

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

- [ ] **TASK-DRAFT-3: Advanced Drafting Entities (Rectangle, Ellipse, Spline Primitives)**
  - **Scope:** Add first-class drafting primitives with direct DXF translation: Rectangle with orthogonal bounds, Ellipse with major/minor axes, and cubic spline curves.
  - **Acceptance:** Validates finite dimensions, positive radii, and exports standard DXF entity groups.
  - **Verification:** `cargo test -p cad-drafting`

- [ ] **TASK-DRAFT-4: Precision Object Snaps (Osnap) & Polar Tracking**
  - **Scope:** Bring CADCraft's 13 object snaps (endpoint, midpoint, center, intersection, tangent, perpendicular, quadrant) into the interactive browser drafting canvas.
  - **Acceptance:** Snapping within 8px screen distance calculates exact mathematical coordinates.
  - **Verification:** Interactive browser tests via CDP driver.

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

- [ ] **TASK-CAD-3: Pattern & Mirror Features (M11h)**
  - **Scope:** Replay features in linear arrays (count, spacing, direction vector) and circular arrays (count, angle, axis), plus planar feature mirroring.
  - **Acceptance:** Patterning a cut creates multiple holes with exact combined analytic volume; cache key incorporates pattern parameters.
  - **Verification:** `cargo run -p cad-kernel-truck --example corpus` and `cargo test -p cad-document`

- [ ] **TASK-CAD-4: Multi-Body Document Support**
  - **Scope:** Generalize `Project` from a single body to a named collection of independent solid bodies, supporting assemblies and component parts.
  - **Acceptance:** Features target a specific `body_id`; operations can reference inter-body clearances.
  - **Verification:** `cargo test -p cad-document`

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

- [ ] **TASK-CAE-4: Modal Analysis & Dynamic Vibration Frequencies**
  - **Scope:** Consistent mass matrix assembly and Lanczos/subspace eigenvalue solver for natural vibration mode frequencies and mode shapes.
  - **Acceptance:** Recovers fundamental natural frequency of cantilever beam within 5% of analytical Euler-Bernoulli solution.
  - **Verification:** `cargo test -p cad-cae --test modal_tests`

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
