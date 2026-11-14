# AtlasCAD — Initial Integration Plan

## 1. Overview & Integration Objectives

The objective is to integrate verified, world-class open-source engineering engines into AtlasCAD without creating an unmanageable monolithic binary or violating open-source software licenses.

AtlasCAD serves as the primary native workstation application; external tools are orchestrated via clean, typed Rust adapter boundaries, structured input generators, asynchronous runners, and telemetry result parsers.

---

## 2. Integration Tiers & Isolation Architecture

Every integrated engine is assigned an integration model based on its license and architectural interface:

```text
┌─────────────────────────────────────────────────────────────┐
│                      AtlasCAD Studio                        │
│             (Web GUI, Viewport, Ribbon, HUDs)               │
└──────────────────────────────┬──────────────────────────────┘
                               │ JSON / WebSocket REST API
┌──────────────────────────────▼──────────────────────────────┐
│                    AtlasCAD Rust Core                       │
│    (atlas-core, atlas-dependency-graph, atlas-engine-mgr)   │
└──────────┬───────────────────┬───────────────────┬──────────┘
           │                   │                   │
   [In-Process Rust]   [C ABI / Dynamic]   [Process-Isolated]
   ─────────────────   ─────────────────   ──────────────────
   • truck B-Rep       • OCCT (C ABI)      • CalculiX (ccx)
   • cad-cae           • ngspice (lib)     • OpenFOAM (CLI)
   • cad-cam           • MuJoCo (C API)    • Gmsh (CLI)
   • cad-constraints   • EPANET (lib)      • pandapower (IPC)
   • atlas-dep-graph   • OpenDSS (lib)     • SU2 (CLI)
```

1. **In-Process Rust (Tier A Permissive):**
   - Directly compiled into the binary.
   - Zero IPC overhead.
   - Guaranteed memory safety and thread safety.
2. **C ABI / Dynamic Link (Tier A & B Permissive / Weak Copyleft):**
   - Linked via dynamic C ABI boundaries.
   - Direct memory passing where feasible; protected by boundary wrappers.
3. **Process-Isolated IPC / Subprocess (Tier C Strong Copyleft - GPL-2.0 / GPL-3.0):**
   - Executed as standalone worker subprocesses.
   - Communicates strictly via standard input/output streams, temporary files, and JSON-RPC.
   - Guaranteed legal isolation complying with GPL/LGPL distribution rules.
4. **Network Daemon / Service (Tier D Network Copyleft - AGPL-3.0):**
   - Executed as network-isolated worker services over HTTP/gRPC.

---

## 3. Prioritized Initial Integration Batch

The initial integration batch targets the most critical engineering workflows:

| Engine | Discipline | Upstream Pinned Version | Integration Path | Status |
|:---|:---|:---|:---|:---|
| **truck** | 3D CAD B-Rep | `0.6.0` (pinned) | Pure Rust dependency | **PRODUCTION_READY** |
| **CalculiX** | Structural FEA | `2.21` | Process-isolated CLI (`ccx`) | **ADAPTED** |
| **Gmsh** | Mesh Generation | `4.13.1` | Process-isolated CLI | **ADAPTED** |
| **OpenFOAM** | CFD Fluid Dynamics | `v11` | Case generator & subprocess | **ADAPTED** |
| **SU2** | Aerodynamics | `8.0.1` | Config generator & CLI | **ADAPTED** |
| **ngspice** | ECAD / SPICE | `ngspice-43` | C ABI / CLI runner | **ADAPTED** |
| **pandapower**| Electrical Grid | `2.14.11` | Python IPC bridge | **ADAPTED** |
| **IfcOpenShell** | OpenBIM (IFC) | `0.7.0` | CLI parser & serializer | **ADAPTED** |
| **MuJoCo** | Robotics Physics | `3.1.5` | C API wrapper | **ADAPTED** |
| **EnergyPlus**| Building Energy | `24.1.0` | IDF generator & CLI | **ADAPTED** |
| **OpenVSP** | Aircraft Design | `3.38.0` | VSP3 script generator & CLI | **ADAPTED** |
| **EPANET** | Water Hydraulics | `2.2.0` | C ABI (`libepanet2`) | **ADAPTED** |

---

## 4. Execution Workflow

For every solver run:
1. **Input Generation:** AtlasCAD translates the active document geometry and parameters into the engine's native format (e.g., `.inp` for CalculiX, `.geo` for Gmsh, `.cir` for ngspice, `.vsp3` for OpenVSP).
2. **Job Launch:** `atlas-engine-manager` spawns the runner with strict CPU time, memory limits, and timeouts.
3. **Telemetry & Logs:** Standard output and error streams are captured in real time and displayed in the Engine Console.
4. **Result Parsing:** Output files (VTK, CSV, raw numbers) are parsed into standardized `EngineJobResult` scalar and tensor payloads.
5. **Viewport Display:** The Multidisciplinary Result Viewer HUD opens automatically, displaying the scalar heatmap, convergence status, calculation time, and peak metrics.
