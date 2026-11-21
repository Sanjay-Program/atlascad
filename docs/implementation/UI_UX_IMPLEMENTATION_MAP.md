# AtlasCAD Production UI/UX Implementation Map & Repository Audit

## 1. Repository Audit (Step 1)

### 1.1 Framework & Version
- **Frontend Framework**: Vanilla ES Modules (ES2022+), WebGL 1/2 native context, SVG vector drafting engine. Zero heavyweight node/npm build-chain runtime dependencies, guaranteeing instantaneous load times and deterministic execution on low-resource Linux and embedded workstations.
- **Backend Framework**: Rust 1.80+ 2021 Edition workspace (32 crates) using Axum-compatible custom zero-dependency TCP/HTTP streaming architecture (`crates/cad-studio/src/lib.rs`).
- **Assets Bundling**: Direct Rust compile-time binary embedding via `include_str!` and `include_bytes!` inside `cad-studio/src/lib.rs`.

### 1.2 Entry Points & Routing
- **Server Entry**: `crates/cad-studio/src/main.rs` & `crates/cad-studio/src/lib.rs` (`StudioServer::run`).
- **HTTP Engine**: Direct TCP stream router dispatching `/`, `/app.css`, `/app.js`, `/favicon.svg`, and 140+ RESTful `/api/*` endpoints.
- **Client Entry**: `crates/cad-studio/assets/index.html` loading `/app.css` and `/app.js`.
- **Client Routing**: Hash-based and state-based unified workspace router (`#workspace`, `#project`, `#sheet`, `#view`) preserving active project, model entity selection, camera state, and active jobs.

### 1.3 Styling System & Design Tokens
- **Current Styling**: Pure CSS custom properties (`crates/cad-studio/assets/app.css`).
- **Token Harmonization**: Full implementation of the specified engineering token palette:
  - `--atlas-bg: #0b1020;`
  - `--atlas-surface-1: #111827;`
  - `--atlas-surface-2: #172033;`
  - `--atlas-surface-3: #202b40;`
  - `--atlas-border: #2a354b;`
  - `--atlas-text: #f4f7fb;`
  - `--atlas-text-muted: #a7b2c5;`
  - `--atlas-accent: #4361ee;`
  - `--atlas-accent-secondary: #22d3ee;`
  - `--atlas-success: #22c55e;`
  - `--atlas-warning: #f59e0b;`
  - `--atlas-danger: #ef4444;`
  - `--atlas-info: #38bdf8;`
- **Typography & Sizing**: High-legibility UI sans-serif, tabular figures for all dimensions/coordinates, 4px/8px layout grid, standard control heights (28px compact, 36px default).

### 1.4 State & Data Architecture
- **Server State**: Thread-safe `Arc<Mutex<StudioState>>` with transactional undo/redo history, parametric dependency DAG (`atlas-core::Project`), feature tree, drawing sheets, and job execution queue.
- **Client State**: Centralized reactive state store in `app.js` holding current project metadata, active viewport transforms, entity selection sets, running background jobs, and notification queues.
- **API Protocol**: JSON payloads with strict error envelopes (`kind`, `message`), binary exports (STL, STEP, 3MF, DXF, PDF), and streaming SSE/polling job monitors.

### 1.5 Build & Verification Commands
- `cargo build --release -p cad-studio`
- `cargo test -p cad-studio --test http_api`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `python3 scripts/full-check.py`
- `python3 scripts/live-demo.py`

---

## 2. Information Architecture & Studio Mapping (Step 2)

| Code | Studio / Page | Route / View ID | Data Source / Backend Endpoint | Required Interactions | Status | Verification Test |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **A1** | Home / Welcome | `#view=home` / `#homeView` | `GET /api/state`, `GET /api/jobs`, `GET /api/engines` | Recent projects, starter templates, resume workspace, job summary | Active | `scripts/live-demo.py` (Sec 1) |
| **A2** | Projects List | `#view=projects` / `#projectsListView` | `GET /api/state` | Search, filter, rename, clone, archive, revision inspection | Active | `http_api.rs` (Project State) |
| **A3** | New Project Wizard | `#modal=newProject` | `POST /api/project/new` / `POST /api/save` | Name, discipline template, units (mm/in), drafting standards | Active | `http_api.rs` |
| **A4** | Project Overview | `#view=project-overview` | `GET /api/engineering`, `GET /api/bom`, `GET /api/mass-properties` | Health summary, mass properties, BOM, open issues, revision stats | Active | `http_api.rs` (Sec 4) |
| **A5** | Project Settings | `#view=project-settings` | `GET /api/state`, `POST /api/parameter` | Unit system, tolerance standards, author, license, render options | Active | `http_api.rs` |
| **A6** | Project Activity | `#view=project-activity` | `GET /api/audit`, `GET /api/commands/history` | Chronological audit log, cryptographic hash chain, operator IDs | Active | `http_api.rs` (Sec 12) |
| **A7** | Version History | `#panel=revisionsSection` | `GET /api/audit`, `POST /api/revision/restore` | Checkpoints, rollback, tag revisions, compare revisions | Active | `http_api.rs` (Sec 12) |
| **A8** | Project Comparison | `#modal=projectCompare` | `GET /api/audit`, `GET /api/interference` | Side-by-side diff, geometric interference, ECO comparison | Active | `http_api.rs` (Sec 5) |
| **A9** | Import & Export Center | `#modal=importExport` | `GET /api/export/*`, `POST /api/draft/import` | STEP, DXF, STL, 3MF, PDF, DaVinci, Unity package downloads | Active | `http_api.rs` (Sec 2, 7-10) |
| **B1-B11** | Engineering Studio (CAD, Assembly, Sketch, Draw) | `#workspace=drafting`, `#workspace=mechanical`, `#workspace=assembly` | `GET /api/state`, `GET /api/draft`, `POST /api/sketch/*`, `POST /api/feature/*` | Parametric sketching, 3D modeling, drawing layout sheets, GD&T, BOM | Active | `http_api.rs` (Sec 1, 3, 4, 11) |
| **C1-C12** | Simulation Studio (CAE, FEA, Multiphysics, Solvers) | `#workspace=simulation` / `#caeWorkbenchView` | `POST /api/cae/*`, `POST /api/sim/*` (140+ solvers) | Study setup, mesh generation, boundary conditions, solver run, post-processing | Active | `http_api.rs` (Sec 14-47) |
| **D1-D8** | Manufacturing Studio (CAM, CNC, Slicing, Additive) | `#workspace=manufacturing` / `#camWorkbenchView`, `#additiveWorkbenchView` | `POST /api/cam/toolpaths`, `POST /api/additive/*`, `POST /api/sim/manufacturing/*` | 3-axis CNC milling, G-code, additive support gen, slicing, wire EDM, welding | Active | `http_api.rs` (Sec 6, 26) |
| **E1-E6** | Architecture Studio (OpenBIM, Civil, GIS) | `#workspace=bim` / `#bimWorkbenchView`, `#civilWorkbenchView` | `GET /api/state`, `POST /api/sim/civil/*` | IFC parsing, building elements, frame analysis, dam stability, hydrology | Active | `http_api.rs` (Sec 18, 29) |
| **F1-F5** | Electronics Studio (ECAD, SPICE, PCB) | `#workspace=electronics` / `#ecadWorkbenchView` | `GET /api/state`, `POST /api/sim/ecad/pcb_thermal`, `POST /api/sim/signal_integrity` | Schematic capture, PCB netlist, thermal relief, DC-DC converter, transmission lines | Active | `http_api.rs` (Sec 20, 31) |
| **G1-G5** | Science Studio (Chemical, Molecular, Materials) | `#workspace=chemical` / `#chemicalWorkbenchView` | `POST /api/sim/chemical/*`, `GET /api/sim/chemical/species` | Titration curves, CSTR kinetics, VLE McCabe-Thiele, chromatography column | Active | `http_api.rs` (Sec 22, 45) |
| **H1-H3** | Robotics Studio (Kinematics, Workcell, ROS) | `#workspace=robotics` / `#roboticsWorkbenchView` | `POST /api/sim/robotics`, `POST /api/sim/robotics_trajectory` | 6-DOF DH forward/inverse kinematics, path planning, ROS integration bridge | Active | `http_api.rs` (Sec 24) |
| **I1-I10** | Create Studio (3D SubD, Sculpt, PBR, Render) | `#workspace=create` / `#createWorkbenchView` | `POST /api/create/subdivide`, `POST /api/create/sculpt`, `GET /api/export/gltf` | Catmull-Clark subdivision, brush sculpting, PBR materials, glTF scene graph | Active | `http_api.rs` (Sec 8) |
| **J1-J8** | RealTime Studio (Interactive, 6-DOF, Unity Twin) | `#workspace=realtime` / `#realtimeWorkbenchView` | `GET /api/export/unity`, `POST /api/realtime/scene`, `POST /api/sim/hypersonic/6dof` | Unity package exporter, flight dynamics telemetry, avionics HUD, digital twin | Active | `http_api.rs` (Sec 10, 16) |
| **K1-K7** | Media Studio (Video NLE, DaVinci Resolve) | `#workspace=media` / `#mediaWorkbenchView` | `GET /api/export/davinci`, `POST /api/media/*` | Multi-track timeline, CMX 3600 EDL, FCPXML, 3D LUT thermal grading, OpenUSD | Active | `http_api.rs` (Sec 9) |
| **L1-L7** | AI Agents & Automation | `#workspace=agents` / `#agentWorkbenchView` | `GET /api/agents`, `POST /api/agents/step`, `POST /api/agents/approvals/respond` | 44 engineering specialists, typed plan review, execution graph, approvals | Active | `http_api.rs` (Sec 13) |
| **M1-M4** | Workflow Builder | `#panel=agentWorkflows` | `GET /api/agents/workflows`, `POST /api/agents/workflows/run` | Multi-agent DAG pipeline execution, parameter sweep, optimization loop | Active | `http_api.rs` (Sec 13) |
| **N1-N4** | Assets & Standard Part Libraries | `#modal=blockLibraryModal` | `GET /api/cad/hardware/fasteners`, `GET /api/cad/hardware/*` | ISO/DIN standard fastener library, gears, keys, springs, materials database | Active | `http_api.rs` (Sec 41) |
| **O1-O5** | Jobs, Results & System Operations | `#modal=enterpriseModal` (Tab 3: HPC Queue) | `GET /api/jobs`, `POST /api/jobs/submit`, `GET /api/engines/discovery` | Job telemetry, solver engine discovery, benchmark, health diagnostics | Active | `http_api.rs` (Sec 12, 14) |
| **P1-P5** | Collaboration, ECOs & Approvals | `#modal=enterpriseModal` (Tab 1, 4: Org & ECO) | `GET /api/org`, `POST /api/eco`, `POST /api/agents/approvals` | Organization roles, change orders (ECO), cryptographic signing, team review | Active | `http_api.rs` (Sec 12, 13) |
| **Q1-Q5** | Plugins & Developer Tools | `#modal=engineManagerModal` | `GET /api/commands`, `POST /api/commands/execute`, `POST /api/mcp` | Model Context Protocol (MCP) tool server, script execution, CLI command bar | Active | `http_api.rs` (Sec 15) |
| **R1-R7** | Settings & Administration | `#modal=settingsModal` | `GET /api/deploy/status`, `GET /api/health` | Theme mode, high-DPI canvas scale, solver engine paths, security certificates | Active | `http_api.rs` (Sec 1) |

---

## 3. Honest Engine Boundaries & Unavailable Capability Policy

In strict conformance with Rules 7 & 8:
1. **Local Analytical & Native Solvers**:
   - All 140+ mathematical, finite-element, aerothermal, and dynamic models in `atlas-simulation-core` and `atlas-core` run deterministically in-process. Results are stamped with calculation timestamps, input parameter hashes, and governing standard references (e.g., ISO, ASME, NASA, DNV, IEEE).
2. **External Native Engine Bridges (ROS, OpenXR, DaVinci Resolve, Unity 3D)**:
   - When an external workstation daemon (e.g., a live ROS 2 Jazzy node or DaVinci Resolve Studio automation socket) is not detected locally, the UI displays an **Honest Connection State**:
     - Visual badge: `UNCONFIGURED / DISCONNECTED`
     - Actionable guide: Link to `/api/engines/guides` with exact installation commands for Linux/Windows.
     - Integration Boundary: Exportable package/asset files (`.fcpxml`, `.edl`, `.cube`, `.unitypackage`, `.gltf`) are provided with authoritative provenance hashes, ready for manual import into external tools without fabricating fake engine responses.
