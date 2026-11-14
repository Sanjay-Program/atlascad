# AtlasCAD — Upstream Engineering Engine Research & Technical Assessment
## Systematic Audit of Open-Source Computational Engines Across 20 Engineering Disciplines

**Document Version:** 2.0.0  
**Classification:** Research, Engine Architecture & Licensing Assessment  
**Author:** AtlasCAD Upstream Integration & Research Group  

---

## 1. Research Scope and Methodology

To deliver an industrial-grade, multi-disciplinary engineering suite without proprietary vendor lock-in, AtlasCAD investigates verified open-source engineering engines. Each candidate engine is audited against:
1. **Mathematical Accuracy & Rigor:** Established numerical validation against canonical benchmarks and closed-form solutions.
2. **Architecture & Thread Safety:** Suitability for embedded or sidecar execution without process memory corruption.
3. **Licensing & Distribution:** Permissive (MIT, Apache 2.0, BSD) vs. Weak Copyleft (LGPL, MPL) vs. Strong Copyleft (GPL, AGPL) compliance.
4. **Rust Compatibility & FFI Overhead:** Clean C ABI or IPC interface allowing memory-safe orchestration.

---

## 2. Cross-Disciplinary Engine Audit

### 2.1 Geometry & Boundary Representation (B-Rep) Kernels

| Project | Canonical Repository | Primary Role | License | Integration Feasibility & Strategy |
|---|---|---|---|---|
| **Truck** | [ricosjp/truck](https://github.com/ricosjp/truck) | Native Rust B-Rep & NURBS modeling kernel | Apache 2.0 | **Primary Native Kernel.** Zero FFI overhead, memory safe, analytical topology (shells, faces, wires, edges). |
| **Open CASCADE (OCCT)** | [Open-Cascade-SAS/OCCT](https://github.com/Open-Cascade-SAS/OCCT) | Comprehensive industrial B-Rep modeling & STEP I/O | LGPL 2.1 w/ Exception | Sidecar / C-ABI bridge for legacy STEP/IGES file translation. |
| **Manifold** | [elalish/manifold](https://github.com/elalish/manifold) | High-speed 3D Boolean operations & mesh repair | Apache 2.0 | Embedded C++/Rust bridge for ultra-fast CSG preview and polygon solid ops. |
| **CGAL** | [CGAL/cgal](https://github.com/CGAL/cgal) | Computational geometry algorithms (Voronoi, Delaunay) | GPL / LGPL | Reference library for complex mesh generation and polygon clipping. |

### 2.2 Structural, Thermal & Finite Element Analysis (FEA)

| Project | Canonical Repository | Primary Role | License | Integration Feasibility & Strategy |
|---|---|---|---|---|
| **CalculiX** | [Dhondtguido/CalculiX](https://github.com/Dhondtguido/CalculiX) | ABAQUS-compatible implicit structural & thermal solver | GPL 2.0 | **Primary External Structural Solver.** Executed via isolated process adapter (`ccx`). |
| **Gmsh** | [live-clones/gmsh](https://github.com/live-clones/gmsh) | Automatic 1D/2D/3D finite element mesh generator | GPL 2.0 w/ Exception | Standard tetrahedral and hexahedral mesh generation via C API or IPC. |
| **FEniCS / DOLFINx** | [FEniCS/dolfinx](https://github.com/FEniCS/dolfinx) | Automated high-order PDE discretization | LGPL 3.0 | Research solver for advanced continuum mechanics formulations. |
| **MOOSE** | [idaholab/moose](https://github.com/idaholab/moose) | Multiphysics Object-Oriented Simulation Environment | LGPL 2.1 | Large-scale nuclear, geological, and thermal-structural coupled multiphysics. |

### 2.3 Computational Fluid Dynamics (CFD) & Aerodynamics

| Project | Canonical Repository | Primary Role | License | Integration Feasibility & Strategy |
|---|---|---|---|---|
| **OpenFOAM** | [OpenFOAM/OpenFOAM-dev](https://github.com/OpenFOAM/OpenFOAM-dev) | Finite volume continuum fluid mechanics | GPL 3.0 | Workstation-grade steady-state (`simpleFoam`) and transient (`pimpleFoam`) CFD sidecar. |
| **SU2** | [su2code/SU2](https://github.com/su2code/SU2) | Compressible aerodynamics & shape optimization | LGPL 2.1 | Primary aerospace external aerodynamics engine with adjoint optimization. |
| **DualSPHysics** | [DualSPHysics/DualSPHysics](https://github.com/DualSPHysics/DualSPHysics) | Smoothed Particle Hydrodynamics (SPH) for free-surface flows | LGPL 3.0 | GPU-accelerated coastal, dam-break, and wave impact simulations. |

### 2.4 Additive Manufacturing & Slicing Engines

| Project | Canonical Repository | Primary Role | License | Integration Feasibility & Strategy |
|---|---|---|---|---|
| **PrusaSlicer** | [prusa3d/PrusaSlicer](https://github.com/prusa3d/PrusaSlicer) | FDM/FFF/SLA path generation | AGPL 3.0 | Command-line CLI invocation for industrial G-code verification. |
| **OrcaSlicer** | [SoftFever/OrcaSlicer](https://github.com/SoftFever/OrcaSlicer) | Advanced multi-material calibration & pressure advance | AGPL 3.0 | Upstream reference for calibration algorithms and dialect postprocessing. |
| **Atlas-Additive** | Native Workspace Crate | Native Rust Slicing & G-Code Emission Engine | MIT / Apache 2.0 | **Primary Native Engine.** In-memory polygon contouring, adaptive layer height, Gyroid/Honeycomb infill, volumetric G-code. |

### 2.5 Subtractive CAM & Machine Simulation

| Project | Canonical Repository | Primary Role | License | Integration Feasibility & Strategy |
|---|---|---|---|---|
| **OpenCAMLib** | [aewallin/opencamlib](https://github.com/aewallin/opencamlib) | Drop-cutter algorithms & 3D toolpath generation | LGPL 2.1 | C++ library adapter for advanced 3D surface finish passes. |
| **CAMotics** | [CauldronDevelopmentLLC/CAMotics](https://github.com/CauldronDevelopmentLLC/CAMotics) | 3-axis CNC simulation & cut verification | GPL 3.0 | Heightmap stock carving and holder clearance verification reference. |
| **Cad-CAM** | Native Workspace Crate | Native Rust 2.5D/3/4/5-Axis CAM Engine | MIT / Apache 2.0 | **Primary Native Engine.** Facing, pocketing, rotary 4-axis, simultaneous 5-axis $(I, J, K)$ G-code emission, heightmap stock removal. |

### 2.6 Architecture & OpenBIM

| Project | Canonical Repository | Primary Role | License | Integration Feasibility & Strategy |
|---|---|---|---|---|
| **IfcOpenShell** | [IfcOpenShell/IfcOpenShell](https://github.com/IfcOpenShell/IfcOpenShell) | IFC2x3, IFC4, IFC4x3 parsing, geometry, and validation | LGPL 3.0 | Standard enterprise BIM serialization and geometric model bridge. |
| **web-ifc** | [ThatOpen/engine_web-ifc](https://github.com/ThatOpen/engine_web-ifc) | Ultra-fast WebAssembly IFC parser and loader | MPL 2.0 | Client-side instant IFC loading in WebGL viewport. |
| **EnergyPlus** | [NREL/EnergyPlus](https://github.com/NREL/EnergyPlus) | Whole-building energy and thermal simulation | BSD-3-Clause | Standard building HVAC and solar heat gain calculation engine. |

### 2.7 Electrical, Electronics & Power Systems

| Project | Canonical Repository | Primary Role | License | Integration Feasibility & Strategy |
|---|---|---|---|---|
| **pandapower** | [e2nIEE/pandapower](https://github.com/e2nIEE/pandapower) | AC power system network analysis & load flow | BSD-3-Clause | Newton-Raphson power flow reference and grid topology modeling. |
| **ngspice** | [ngspice/ngspice](https://github.com/ngspice/ngspice) | Mixed-mode SPICE circuit simulator | BSD / GPL | Embedded SPICE engine for analog, digital, and transient simulation. |
| **KiCad** | [gitlab.com/kicad/code/kicad](https://gitlab.com/kicad/code/kicad) | Schematic capture & PCB layout suite | GPL 3.0 | Reference for Gerber RS-274X, IPC-2581, and footprint libraries. |

### 2.8 Chemical Process & Molecular Dynamics

| Project | Canonical Repository | Primary Role | License | Integration Feasibility & Strategy |
|---|---|---|---|---|
| **Cantera** | [Cantera/cantera](https://github.com/Cantera/cantera) | Chemical kinetics, thermodynamics & transport | BSD-3-Clause | Ideal gas, combustion, and electrochemical reaction kinetics. |
| **DWSIM** | [DanWBR/dwsim](https://github.com/DanWBR/dwsim) | Steady-state chemical process simulation | GPL 3.0 | VLE flash and distillation column thermodynamic property reference. |
| **LAMMPS** | [lammps/lammps](https://github.com/lammps/lammps) | Classical molecular dynamics simulator | GPL 2.0 | Particle-mesh Ewald and interatomic potential benchmark reference. |

### 2.9 Robotics & Multi-Body Dynamics

| Project | Canonical Repository | Primary Role | License | Integration Feasibility & Strategy |
|---|---|---|---|---|
| **Pinocchio** | [stack-of-tasks/pinocchio](https://github.com/stack-of-tasks/pinocchio) | Fast rigid body dynamics & spatial algebra | BSD-2-Clause | Spatial algebra algorithms for Featherstone dynamics and recursive Newton-Euler. |
| **MuJoCo** | [google-deepmind/mujoco](https://github.com/google-deepmind/mujoco) | Convex contact dynamics & multi-joint physics | Apache 2.0 | High-performance contact simulation and reinforcement learning control. |
| **Drake** | [RobotLocomotion/drake](https://github.com/RobotLocomotion/drake) | Model-based design and trajectory optimization | BSD-3-Clause | Trajectory optimization and numerical Inverse Kinematics benchmark reference. |

---

## 3. Integration Tier Classification

To maintain high responsiveness and strict process stability:
1. **Tier 1 (Native Rust Workspace Engines):** Executed directly in-memory within AtlasCAD. Includes Truck B-Rep, 2D Drafting, FEA linear solver, Additive Slicer, CAM toolpath engine, SIMP topology optimizer, and analytical cross-disciplinary solvers.
2. **Tier 2 (Sandboxed Embedded FFI / C-ABI):** Memory-isolated C/C++ static or dynamic libraries (e.g. Manifold, web-ifc) accessed via strictly bounded FFI.
3. **Tier 3 (External Sidecar Processes):** Heavyweight scientific solvers (CalculiX `ccx`, Gmsh, OpenFOAM, SU2) managed asynchronously by `atlas-job-queue` with file-based input/output validation and cryptographic hashing.
