# AtlasCAD — Upstream Repository Registry

## Verified Canonical Repositories


> **Status correction (2026-11-27).** `ADAPTED` previously meant only "source identified". Only
> CalculiX and ngspice have execution adapters (benchmark scope); every other external row is
> `NO ADAPTER`. See [`../ecosystem/INTEGRATION_PROGRESS.md`](../ecosystem/INTEGRATION_PROGRESS.md).

| Engine ID | Canonical Upstream Repository | Pinned Release / Commit | Primary Language | License | Target Integration Path | Current Status |
|:---|:---|:---|:---|:---|:---|:---|
| **truck** | `https://github.com/ricosjp/truck` | `0.6.0` (`78d699b`) | Rust | Apache-2.0 | Pure Rust crate in workspace | **BUILT-IN / TESTED** |
| **cad-cae** | Internal / AtlasCAD | `0.1.0` | Apache-2.0 / MIT | Rust | In-process native solver | **BUILT-IN / TESTED** |
| **cad-cam** | Internal / AtlasCAD | `0.1.0` | Apache-2.0 / MIT | Rust | In-process native toolpath | **BUILT-IN / TESTED** |
| **occt** | `https://github.com/Open-Cascade-SAS/OCCT` | `7.8.0` | C++ | LGPL-2.1 w/ exception | C ABI / CLI worker service | **NO ADAPTER** |
| **solvespace** | `https://github.com/solvespace/solvespace` | `v3.1` | C++ | GPL-3.0 | Process-isolated CLI worker | **NO ADAPTER** |
| **manifold** | `https://github.com/elalish/manifold` | `v2.5.1` | C++ | Apache-2.0 | C ABI / WASM module | **RESEARCHED** |
| **gmsh** | `https://gitlab.onelab.info/gmsh/gmsh` | `4.13.1` | C++ | GPL-2.0+ | Process-isolated CLI adapter | **NO ADAPTER** |
| **calculix** | `https://github.com/Dhondtguido/CalculiX` | `v2.21` | C / Fortran | GPL-2.0 | Process-isolated worker (`ccx`) | **BENCHMARK ADAPTER** |
| **openfoam** | `https://github.com/OpenFOAM/OpenFOAM-dev` | `v11` | C++ | GPL-3.0 | Case generator + runner subprocess | **NO ADAPTER** |
| **su2** | `https://github.com/su2code/SU2` | `v8.0.1` | C++ | LGPL-2.1 | Config generator + CLI worker | **NO ADAPTER** |
| **ifcopenshell**| `https://github.com/IfcOpenShell/IfcOpenShell`| `v0.7.0` | C++ / Python | LGPL-3.0 | CLI parser & serializer | **NO ADAPTER** |
| **energyplus** | `https://github.com/NREL/EnergyPlus` | `v24.1.0` | C++ | BSD-3-Clause | IDF model generator + CLI runner | **NO ADAPTER** |
| **opensees** | `https://github.com/OpenSees/OpenSees` | `v3.6.0` | C++ / Tcl | BSD-like | Process-isolated CLI bridge | **NO ADAPTER** |
| **ngspice** | `https://github.com/ngspice/ngspice` | `ngspice-43` | C | BSD-3-Clause / LGPL | Shared library / CLI worker | **BENCHMARK ADAPTER** |
| **kicad** | `https://gitlab.com/kicad/code/kicad` | `8.0.2` | C++ | GPL-3.0+ | File parser & CLI tool runner | **NO ADAPTER** |
| **pandapower** | `https://github.com/e2nIEE/pandapower` | `v2.14.11` | Python | BSD-3-Clause | Python IPC worker bridge | **NO ADAPTER** |
| **opendss** | `https://github.com/tshort/OpenDSS` | `v9.6.1` | Pascal / C ABI | BSD-3-Clause | C ABI (`libdss_capi`) | **NO ADAPTER** |
| **cantera** | `https://github.com/Cantera/cantera` | `v3.0.0` | C++ | BSD-3-Clause | C++ / Python bridge | **NO ADAPTER** |
| **dwsim** | `https://github.com/DanWBR/dwsim` | `v8.6.0` | C# / .NET | GPL-3.0 | CLI / IPC bridge | **NO ADAPTER** |
| **rdkit** | `https://github.com/rdkit/rdkit` | `Release_2024_03_1` | C++ | BSD-3-Clause | C++ library / Python adapter | **NO ADAPTER** |
| **openvsp** | `https://github.com/OpenVSP/OpenVSP` | `OpenVSP-3.38.0` | C++ | NASA Open Source 1.3 | CLI / VSP3 script generator | **NO ADAPTER** |
| **jsbsim** | `https://github.com/JSBSim-Team/jsbsim` | `v1.2.0` | C++ | LGPL-2.1 | C++ bridge / CLI worker | **NO ADAPTER** |
| **mujoco** | `https://github.com/google-deepmind/mujoco` | `3.1.5` | C | Apache-2.0 | C API / Native wrapper | **NO ADAPTER** |
| **epanet** | `https://github.com/USEPA/EPANET` | `v2.2.0` | C | Public Domain / CC0 | C API (`libepanet2`) / CLI | **NO ADAPTER** |
| **swmm** | `https://github.com/USEPA/Stormwater-Management-Model` | `v5.2.4` | C | Public Domain / CC0 | C API / CLI runner | **NO ADAPTER** |
| **modflow** | `https://github.com/MODFLOW-ORG/modflow6` | `6.4.4` | Fortran | Public Domain (US) | FloPy / CLI runner | **NO ADAPTER** |
