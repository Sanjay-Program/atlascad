# AtlasCAD Upstream Repository Registry

## Overview
This registry catalogs the primary upstream open-source projects evaluated and integrated into the AtlasCAD unified engineering ecosystem. Every project is audited for license compatibility, technical architecture, and integration boundaries.

---

## 1. CAD, Computational Geometry & Mesh Processing

| Project | Canonical GitHub / Upstream URL | Pinned Release / Commit | License | Primary Language | Purpose in AtlasCAD | Integration Method | Integration Status |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **truck** | `github.com/ricosjp/truck` | `0.6.0` (commit `78d699b`) | Apache-2.0 | Rust | Pure-Rust B-Rep geometry and topology modeling kernel | Native Rust dependency & workspace patches | **PRODUCTION_READY** |
| **Open CASCADE Technology (OCCT)** | `github.com/Open-CASCADE-Store/opencascade-classic` / `dev.opencascade.org` | `7.8.0` | LGPL-2.1 w/ exception | C++ | Advanced B-Rep, STEP AP203/214/242, IGES exchange, boolean operations | C ABI / CLI worker service | **ADAPTED** |
| **SolveSpace** | `github.com/solvespace/solvespace` | `v3.1` | GPL-3.0 | C++ | 2D/3D geometric constraint solver engine | Process-isolated CLI / IPC worker | **ADAPTED** |
| **Manifold** | `github.com/elalish/manifold` | `v2.5.1` | Apache-2.0 | C++ | Guaranteed watertight mesh boolean operations | C ABI / WASM module | **RESEARCHED** |
| **Gmsh** | `gitlab.onelab.info/gmsh/gmsh` / `github.com/nschloe/gmsh` | `4.13.1` | GPL-2.0+ | C++ | Automated 2D/3D finite element mesh generation | Process-isolated CLI adapter | **ADAPTED** |
| **Netgen** | `github.com/NGSolve/netgen` | `v6.2.2404` | LGPL-2.1 | C++ | 3D tetrahedral meshing from CAD B-Rep surfaces | Dynamic library / IPC adapter | **RESEARCHED** |

---

## 2. Finite Element Analysis (FEA) & Structural Engineering

| Project | Canonical GitHub / Upstream URL | Pinned Release / Commit | License | Primary Language | Purpose in AtlasCAD | Integration Method | Integration Status |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **CalculiX** | `github.com/dhondt/calculix` / `calculix.de` | `v2.21` | GPL-2.0 | C / Fortran | Implicit static, dynamic, modal, and thermal structural solver | Process-isolated worker (`ccx`) | **ADAPTED** |
| **FEniCSx** | `github.com/FEniCS/dolfinx` | `v0.8.0` | LGPL-3.0 | C++ / Python | Automated solution of differential equations & multiphysics | Subprocess IPC adapter | **RESEARCHED** |
| **OpenSees** | `github.com/OpenSees/OpenSees` | `v3.6.0` | Custom (BSD-like) | C++ / Tcl | Structural earthquake, seismic, and non-linear frame analysis | Process-isolated CLI / Python bridge | **ADAPTED** |
| **Project Chrono** | `github.com/projectchrono/chrono` | `8.0.0` | BSD-3-Clause | C++ | Multi-body rigid and flexible dynamics, contact mechanics | C++ bridge / IPC worker | **RESEARCHED** |
| **Code_Aster** | `code-aster.org` / `gitlab.com/codeaster` | `16.4.0` | GPL-3.0 | Fortran / Python | Nuclear and heavy industrial structural integrity analysis | Isolated containerized worker | **RESEARCHED** |

---

## 3. Computational Fluid Dynamics (CFD) & Aerodynamics

| Project | Canonical GitHub / Upstream URL | Pinned Release / Commit | License | Primary Language | Purpose in AtlasCAD | Integration Method | Integration Status |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **OpenFOAM** | `github.com/OpenFOAM/OpenFOAM-dev` / `openfoam.org` | `v11` | GPL-3.0 | C++ | Incompressible/compressible Navier-Stokes, turbulence, multiphase | Case generator + runner subprocess | **ADAPTED** |
| **SU2** | `github.com/su2code/SU2` | `v8.0.1` | LGPL-2.1 | C++ | Compressible external aerodynamics, aircraft lift/drag, transonic flow | Config generator + CLI worker | **ADAPTED** |
| **Palabos** | `github.com/palabos/palabos` | `v2.3.0` | AGPL-3.0 | C++ | Lattice Boltzmann method (LBM) fluid dynamics | Isolated network service | **LICENSE_REVIEWED** |
| **DualSPHysics** | `github.com/DualSPHysics/DualSPHysics` | `v5.2` | LGPL-3.0 | C++ / CUDA | Smoothed Particle Hydrodynamics (SPH) free-surface flow | GPU subprocess worker | **RESEARCHED** |

---

## 4. Civil Engineering, Architecture & BIM

| Project | Canonical GitHub / Upstream URL | Pinned Release / Commit | License | Primary Language | Purpose in AtlasCAD | Integration Method | Integration Status |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **IfcOpenShell** | `github.com/IfcOpenShell/IfcOpenShell` | `v0.7.0` | LGPL-3.0 | C++ / Python | OpenBIM IFC2x3 / IFC4 parser, geometry extractor, and serializer | C ABI / CLI parser adapter | **ADAPTED** |
| **QGIS / GDAL** | `github.com/OSGeo/gdal` | `v3.9.0` | MIT / X11 | C++ | Geospatial raster/vector translation, terrain DEM import | Native library bindings / CLI | **ADAPTED** |
| **EnergyPlus** | `github.com/NREL/EnergyPlus` | `v24.1.0` | BSD-3-Clause | C++ | Building whole-energy simulation, thermal comfort, HVAC sizing | IDF model generator + CLI runner | **ADAPTED** |
| **OpenGeoSys** | `github.com/ufz/ogs` | `6.5.2` | BSD-3-Clause | C++ | Geotechnical, groundwater, and subsurface mechanics | Subprocess solver adapter | **RESEARCHED** |

---

## 5. Electrical & Electronics (ECAD / SPICE / Power)

| Project | Canonical GitHub / Upstream URL | Pinned Release / Commit | License | Primary Language | Purpose in AtlasCAD | Integration Method | Integration Status |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **ngspice** | `github.com/ngspice/ngspice` / `ngspice.sourceforge.io` | `ngspice-43` | BSD-3-Clause / LGPL | C | Mixed-signal SPICE analog/digital circuit simulation | Shared library (`libngspice`) / CLI | **ADAPTED** |
| **KiCad** | `gitlab.com/kicad/code/kicad` / `github.com/KiCad/kicad-source-mirror` | `8.0.2` | GPL-3.0+ | C++ | Schematic capture, PCB layout, Gerber generation | File parser & CLI tool runner | **ADAPTED** |
| **pandapower**| `github.com/e2nIEE/pandapower` | `v2.14.11` | BSD-3-Clause | Python | Electrical power grid power flow, short-circuit, and state estimation | Python IPC bridge | **ADAPTED** |
| **OpenDSS** | `github.com/tshort/OpenDSS` | `v9.6.1` | BSD-3-Clause | Pascal / C ABI | Distribution system power flow and renewable integration | C ABI (`dss_capi`) | **ADAPTED** |
| **Xyce** | `github.com/Xyce/Xyce` | `7.8` | GPL-3.0 | C++ | Massively parallel SPICE circuit simulation | CLI subprocess | **RESEARCHED** |

---

## 6. Chemical Engineering, Molecular Modeling & Thermodynamics

| Project | Canonical GitHub / Upstream URL | Pinned Release / Commit | License | Primary Language | Purpose in AtlasCAD | Integration Method | Integration Status |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **Cantera** | `github.com/Cantera/cantera` | `v3.0.0` | BSD-3-Clause | C++ | Chemical kinetics, thermodynamics, and transport processes | C++ / Python bridge | **ADAPTED** |
| **DWSIM** | `github.com/DanWBR/dwsim` | `v8.6.0` | GPL-3.0 | C# / .NET | Chemical process flowsheet simulation and unit operations | CLI / IPC bridge | **ADAPTED** |
| **RDKit** | `github.com/rdkit/rdkit` | `Release_2024_03_1` | BSD-3-Clause | C++ | Cheminformatics, 2D/3D molecular structure generation | C++ library / Python adapter | **ADAPTED** |
| **LAMMPS** | `github.com/lammps/lammps` | `patch_2Aug2023` | GPL-2.0 | C++ | Classical molecular dynamics simulation | CLI subprocess worker | **RESEARCHED** |

---

## 7. Aerospace, Automotive & Robotics

| Project | Canonical GitHub / Upstream URL | Pinned Release / Commit | License | Primary Language | Purpose in AtlasCAD | Integration Method | Integration Status |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **OpenVSP** | `github.com/OpenVSP/OpenVSP` | `OpenVSP-3.38.0`| NASA Open Source 1.3 | C++ | Parametric aircraft geometric modeling and aerodynamic evaluation | CLI / VSP3 script generator | **ADAPTED** |
| **JSBSim** | `github.com/JSBSim-Team/jsbsim` | `v1.2.0` | LGPL-2.1 | C++ | 6-DOF flight dynamics and control system modeling | C++ bridge / CLI worker | **ADAPTED** |
| **MuJoCo** | `github.com/google-deepmind/mujoco` | `3.1.5` | Apache-2.0 | C | High-speed multi-joint contact physics and robot dynamics | C API / Native wrapper | **ADAPTED** |
| **ROS 2** | `github.com/ros2/ros2` | `Iron / Jazzy` | Apache-2.0 | C++ / Python | Robot operating system, kinematic chains (URDF), workcells | URDF parser & ROS2 node bridge | **ADAPTED** |
| **CARLA** | `github.com/carla-simulator/carla` | `0.9.15` | MIT | C++ / Python | Open-source autonomous driving simulator | Python client bridge | **RESEARCHED** |

---

## 8. Environmental, Marine & Geoscience

| Project | Canonical GitHub / Upstream URL | Pinned Release / Commit | License | Primary Language | Purpose in AtlasCAD | Integration Method | Integration Status |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **EPANET** | `github.com/OpenWaterAnalytics/EPANET` | `v2.2.0` | Public Domain / CC0 | C | Pressurized drinking water distribution pipe networks | C API (`epanet2.dll`/`.so`) / CLI | **ADAPTED** |
| **EPA SWMM** | `github.com/OpenWaterAnalytics/Stormwater-Management-Model` | `v5.2.4` | Public Domain / CC0 | C | Stormwater runoff, urban drainage, and wastewater networks | C API / CLI runner | **ADAPTED** |
| **MODFLOW** | `github.com/MODFLOW-USGS/modflow6` | `6.4.4` | Public Domain (US) | Fortran | 3D groundwater flow and regional aquifer simulation | FloPy / CLI runner | **ADAPTED** |
| **Capytaine**| `github.com/mancellin/capytaine` | `v2.1` | GPL-3.0 | Python / Fortran | Linear potential flow for marine hydrodynamic diffraction/radiation | CLI worker | **RESEARCHED** |
