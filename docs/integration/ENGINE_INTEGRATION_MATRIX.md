# AtlasCAD — Engine Integration Matrix

| Engine ID | Discipline | Upstream Version | Execution Model | Input Formats | Output Formats | Capabilities |
|:---|:---|:---|:---|:---|:---|:---|
| `truck` | Mechanical / CAD | `0.6.0` | InProcess | Native Rust structs | TriMesh, STEP | B-Rep solid modeling, booleans, sweeps |
| `cad-cae` | Simulation | `0.1.0` | InProcess | Native Rust mesh | Scalar field, Von Mises | Linear static FEA, modal vibration, thermal |
| `cad-cam` | Manufacturing | `0.1.0` | InProcess | Native solid body | ISO G-code, toolpath | 3-axis CNC roughing, contouring, speeds |
| `occt` | Mechanical / CAD | `7.8.0` | CAbi | STEP, IGES, BREP | BREP, STEP, STL | Industrial B-Rep solids, surface healing |
| `gmsh` | Meshing | `4.13.1` | ProcessIsolated | `.geo`, `.step` | `.msh`, `.vtk` | 2D/3D Delaunay & Frontal tetrahedral meshing |
| `calculix` | Simulation | `v2.21` | ProcessIsolated | `.inp` (Abaqus format)| `.dat`, `.frd` | Implicit non-linear FEA, contact, plasticity |
| `openfoam`| CFD | `v11` | ProcessIsolated | OpenFOAM case dicts | VTK, polyMesh fields | Multiphase, turbulence (k-epsilon, k-omega) |
| `su2` | Aerospace / CFD | `v8.0.1` | ProcessIsolated | `.cfg`, `.su2` | `.vtu`, `.csv` | Compressible Euler/Navier-Stokes, drag/lift |
| `ifcopenshell` | BIM / Civil | `v0.7.0` | ProcessIsolated | `.ifc` (IFC2x3 / IFC4)| JSON, OBJ, DAE | Spatial hierarchy, building elements |
| `energyplus` | BIM / Architecture | `v24.1.0` | ProcessIsolated | `.idf`, `.epw` | `.csv`, `.sql` | Hourly energy consumption, HVAC sizing |
| `opensees`| Civil / Structural | `v3.6.0` | ProcessIsolated | `.tcl`, Python | JSON, time-series | Non-linear pushover, dynamic seismic frames |
| `ngspice` | Electronics / ECAD| `ngspice-43`| CAbi / Subprocess | `.cir`, SPICE netlist | `.raw`, tabular CSV | DC sweep, AC frequency response, transient |
| `kicad` | Electronics / ECAD| `8.0.2` | ProcessIsolated | `.kicad_sch`, `.kicad_pcb` | Gerber, Excellon, DRC | Schematic parity, PCB design rules |
| `pandapower` | Power Systems | `v2.14.11` | ProcessIsolated | JSON net model | JSON bus/branch metrics | Newton-Raphson AC power flow, IEC 60909 |
| `opendss` | Power Systems | `v9.6.1` | CAbi | `.dss` script | Tabular CSV, binary | Distribution feeders, harmonics, solar PV |
| `cantera` | Chemical Engineering | `v3.0.0` | CAbi / Subprocess | `.yaml`, `.cti` | JSON, CSV profiles | Chemical kinetics, combustion, flame speed |
| `dwsim` | Process Flow | `v8.6.0` | ProcessIsolated | `.dwxmz`, XML | Property matrices | VLE/LLE thermodynamics, distillation |
| `rdkit` | Chemistry / Materials | `2024.03.1` | CAbi / Subprocess | SMILES, `.mol`, `.sdf` | 3D PDB, mol blocks | 2D/3D structure generation, conformers |
| `openvsp` | Aerospace | `OpenVSP-3.38.0`| ProcessIsolated | `.vsp3` script | `.stl`, `.tri`, polar | Parametric aircraft lofting, aero panels |
| `jsbsim` | Aerospace | `v1.2.0` | ProcessIsolated | XML flight model | Time-series telemetry | 6-DOF atmospheric flight dynamics |
| `mujoco` | Robotics | `3.1.5` | CAbi | MJCF (XML), URDF | Joint states, forces | Multi-joint contact physics, tendon actuators |
| `epanet` | Geoscience / Water | `v2.2.0` | CAbi | `.inp` water network | Binary / CSV heads | Pressurized pipe hydraulics, chemical decay |
| `swmm` | Geoscience / Water | `v5.2.4` | ProcessIsolated | `.inp` urban runoff | Hydrograph time-series | Stormwater runoff, overland flow, conduits |
| `modflow` | Geoscience / Hydro | `6.4.4` | ProcessIsolated | MF6 input packages | Head arrays, budgets | 3D groundwater flow in regional aquifers |
