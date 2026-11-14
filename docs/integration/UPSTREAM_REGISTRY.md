# AtlasCAD — Upstream Repository Registry & Provenance Log
## Verified Open-Source Engineering Repositories, Version Pins & Canonical Mirrors

**Document Version:** 2.0.0  
**Classification:** Upstream Registry & Provenance Record  
**Author:** AtlasCAD Open-Source Integration & DevOps Group  

---

## 1. Registry Architecture

AtlasCAD establishes formal provenance records for all upstream open-source computational repositories evaluated, integrated, or benchmarked across the 20 engineering domains.

```
Upstream Canonical Repo ──► License Audit ──► Version Pinning ──► FFI/IPC Adapter ──► AtlasCAD Crate
```

---

## 2. Upstream Repositories Master Matrix

| Subsystem | Canonical Repository | Primary Upstream URL | Pinned Release / Ref | License | Integration Method |
|---|---|---|---|---|---|
| **CAD Kernel** | `truck` | https://github.com/ricosjp/truck | v0.6.0 (vendor patched) | Apache 2.0 | Native Rust Crate (`cad-kernel-truck`) |
| **Mesh Gen** | `gmsh` | https://github.com/live-clones/gmsh | v4.13.1 | GPL 2.0 w/ Exception | Sidecar Executable & Mesh Parser |
| **FEA Solver** | `CalculiX` | https://github.com/Dhondtguido/CalculiX | v2.21 | GPL 2.0 | Asynchronous Sidecar via `atlas-job-queue` |
| **CFD Solver** | `OpenFOAM` | https://github.com/OpenFOAM/OpenFOAM-dev | v2406 | GPL 3.0 | Containerized / Sidecar Process |
| **Aerodynamics** | `SU2` | https://github.com/su2code/SU2 | v8.0.1 | LGPL 2.1 | Sidecar Process |
| **Multiphysics** | `preCICE` | https://github.com/precice/precice | v3.1.2 | LGPL 3.0 | IPC Coupling Interface |
| **BIM Parser** | `IfcOpenShell` | https://github.com/IfcOpenShell/IfcOpenShell | v0.8.0 | LGPL 3.0 | Python/C++ Sidecar & IFC4 Parser |
| **Web BIM** | `web-ifc` | https://github.com/ThatOpen/engine_web-ifc | v0.0.60 | MPL 2.0 | WebAssembly / JS Loader |
| **Building Energy**| `EnergyPlus` | https://github.com/NREL/EnergyPlus | v24.1.0 | BSD-3-Clause | Simulation Sidecar |
| **Geospatial** | `GDAL` | https://github.com/OSGeo/gdal | v3.9.1 | MIT / X11 | C-ABI Library Adapter |
| **Power Flow** | `pandapower` | https://github.com/e2nIEE/pandapower | v2.14.0 | BSD-3-Clause | Native Rust Analytical Solver Benchmark |
| **Circuit Sim** | `ngspice` | https://github.com/ngspice/ngspice | v43 | BSD / GPL | Embedded C-ABI Solver |
| **Thermodynamics** | `Cantera` | https://github.com/Cantera/cantera | v3.0.0 | BSD-3-Clause | C++ Bridge & Analytical Benchmark |
| **Process Sim** | `DWSIM` | https://github.com/DanWBR/dwsim | v8.8.0 | GPL 3.0 | Thermodynamic VLE Benchmark Reference |
| **Molecular** | `LAMMPS` | https://github.com/lammps/lammps | stable_2Aug2023 | GPL 2.0 | Native Velocity-Verlet Reference |
| **Robotics** | `Pinocchio` | https://github.com/stack-of-tasks/pinocchio | v3.1.0 | BSD-2-Clause | Native Geometric Jacobian Benchmark |
| **Physics Sim** | `MuJoCo` | https://github.com/google-deepmind/mujoco | v3.2.0 | Apache 2.0 | C-ABI Dynamic Library Adapter |
| **Additive Slicing** | `PrusaSlicer` | https://github.com/prusa3d/PrusaSlicer | version_2.8.0 | AGPL 3.0 | Command-Line Verification Engine |
| **CAM Drop-Cutter**| `OpenCAMLib` | https://github.com/aewallin/opencamlib | v2023.01 | LGPL 2.1 | C++ CAM Surface Adapter |
| **CNC Simulation** | `CAMotics` | https://github.com/CauldronDevelopmentLLC/CAMotics | v1.2.2 | GPL 3.0 | Heightmap Stock Carving Reference |
