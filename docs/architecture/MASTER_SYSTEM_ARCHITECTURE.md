# AtlasCAD — Master System Architecture & Crate Dependency Structure
## Rust-First, Memory-Safe Engineering Creation, Multiphysics & Manufacturing Operating System

**Document Version:** 2.0.0  
**Classification:** Master System Architecture  
**Safety Mandate:** `#![forbid(unsafe_code)]`, zero unhandled panics, zero memory leaks.  
**Author:** AtlasCAD Lead Software Architect & Computational Engineering Group  

---

## 1. Architectural Principles

1. **Rust-First Core:** All foundation data structures, geometry kernels, constraint solvers, toolpath planners, and analytical physics engines are authored in Rust under `#![forbid(unsafe_code)]`.
2. **Modular Crate Topology:** The workspace is decomposed into 27 decoupled crates with explicit boundaries. Upstream dependencies flow unidirectionally toward lower-level foundation crates.
3. **Local-First & Air-Gapped Ready:** The application executes locally without mandatory external network or cloud connectivity, guaranteeing IP sovereignty and aerospace/defense air-gap compatibility.
4. **Hardware-Accelerated Single-Process UX:** High-performance WebGL 3D viewport rendering at 60 FPS, decoupled from asynchronous background computation tasks via non-blocking channels.
5. **Universal In-Memory Engineering Graph:** A unified directed graph connecting Geometry $\to$ Mesh $\to$ CAE $\to$ CAM $\to$ Additive $\to$ Drawings $\to$ BoQ.

---

## 2. Workspace Crate Topology

```
                                  [ cad-studio ]
                    (HTTP Server, WebGL Assets, Studio State)
                                       │
     ┌──────────────────┬──────────────┴───────────────┬──────────────────┐
     ▼                  ▼                              ▼                  ▼
[ cad-drafting ]   [ cad-cae ]                    [ cad-cam ]    [ atlas-additive ]
(2D Layouts/DXF) (Linear FEA/Modal)            (3/4/5-Axis CAM)  (Slicer & G-Code)
     │                  │                              │                  │
     └──────────────────┼──────────────────────────────┼──────────────────┘
                        ▼                              ▼
             [ atlas-simulation-core ]      [ cad-engineering-model ]
         (11 Cross-Disciplinary Solvers)       (Mass Props, Interf, BoQ)
                        │                              │
                        └──────────────┬───────────────┘
                                       ▼
                             [ cad-kernel-truck ]
                       (Analytical B-Rep, NURBS, CSG)
                                       │
                                       ▼
                             [ atlas-feature-tree ]
                       (Parametric Timeline, Rollback)
                                       │
                                       ▼
                          [ atlas-dependency-graph ]
                       (Cascade Invalidation Engine)
                                       │
                                       ▼
                       [ atlas-audit & atlas-compliance ]
                    (128-bit Hash Chain, ISO 9001, AS9100)
```

---

## 3. Core Subsystem Crate Catalog

### 3.1 Kernel & Modeling Foundations
- **`cad-kernel-truck`:** Analytical B-Rep solid modeler built on Truck. Manages topological vertices, edges, wires, faces, shells, and solids with exact NURBS evaluators.
- **`atlas-feature-tree`:** Directed acyclic parametric feature timeline supporting dynamic rollback, parameter re-evaluation, and feature suppression.
- **`atlas-dependency-graph`:** Downstream dependency invalidator. Dynamically flags simulation meshes, CAM paths, and technical drawings as dirty when master CAD parameters mutate.

### 3.2 Manufacturing Platforms
- **`cad-cam`:** Subtractive CNC machining engine. Generates 2.5D facing and pocketing, rotary 4-axis continuous/indexed cylindrical toolpaths, and 5-axis simultaneous contouring with modal $(I, J, K)$ surface normal orientation vectors. Includes heightmap stock removal simulation and tool holder clearance checking.
- **`atlas-additive`:** Complete 3D printing and slicing engine. Features Dolenc & Mäkelä adaptive layer height calculation ($h = c_{allowable} / \cos\theta$), 5 infill patterns (Rectilinear, Grid, Honeycomb, Gyroid, Concentric), volumetric extrusion kinematics ($E = \frac{w \cdot h \cdot L}{\pi (d/2)^2}$), and multi-dialect RepRap/Marlin/Klipper G-code emission.

### 3.3 Multiphysics Simulation Core
- **`cad-cae`:** Structural mechanics engine. Provides 3D voxelization, linear static finite element analysis, modal vibration (fundamental frequencies and mode shapes), and steady-state thermal conductivity.
- **`atlas-simulation-core`:** Standardized cross-disciplinary simulation engine with 11 analytical solvers:
  1. *CFD & Hydraulics:* Darcy-Weisbach head loss, Colebrook-White friction factor, Reynolds number regimes.
  2. *External Aerodynamics:* Wing lift/drag coefficients, supersonic normal shock wave jump conditions via Rankine-Hugoniot.
  3. *Chemical Thermodynamics:* Rachford-Rice vapor-liquid equilibrium (VLE) flash solver.
  4. *Molecular Dynamics:* Lennard-Jones (12-6) interatomic potential and Velocity-Verlet integrator.
  5. *Robotics Kinematics:* Denavit-Hartenberg forward kinematics, geometric Jacobian ($6 \times N$), Yoshikawa manipulability index, and Damped Least-Squares (DLS) Inverse Kinematics.
  6. *Electrical Power Grid:* Bus Admittance Matrix $Y_{bus}$ construction, complex arithmetic, and transmission loss calculation.
  7. *Generative Engineering:* SIMP density topology optimization, Sigmund spatial sensitivity filter, and watertight 3D manifold mesh reconstruction.
  8. *Geotechnical Mechanics:* Rankine active and passive lateral earth pressure, overturning moments.
  9. *Biomedical Engineering:* Moens-Korteweg arterial pulse wave velocity (PWV), Womersley oscillatory flow regime, orthopedic cortical bone stress and Factor of Safety.
  10. *Marine Engineering:* Naval hydrostatics (displaced volume, Morrish vertical center of buoyancy, metacentric height $GM$, righting moment), and Airy linear surface gravity wave dispersion via Newton-Raphson iteration.
  11. *Materials Database:* Unified mechanical, thermal, electrical, and fluid material registry.

### 3.4 Enterprise Governance & Compliance
- **`atlas-audit`:** Cryptographic 128-bit FNV-1a continuous hash chain logging every engineering operation and design modification into an immutable audit trail.
- **`atlas-compliance`:** Regulatory engineering dossier generator validating models against ISO 9001 (Quality Management), AS9100 Rev D (Aerospace), and 21 CFR Part 11 (FDA Electronic Signatures).
- **`atlas-job-queue`:** Distributed priority simulation scheduler managing multi-tenant core-hour and storage quotas.
- **`atlas-organization`:** Multi-tiered enterprise subscription accounting and Engineering Change Order (ECO) lifecycle approval state machine.
