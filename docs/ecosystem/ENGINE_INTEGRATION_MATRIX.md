# AtlasCAD Engine Integration Matrix

## Overview
This matrix defines the integration status, primary capabilities, execution model, input/output data formats, and resource limits for all open-source engineering engines within AtlasCAD.

---

## Engineering Engine Matrix

| Engine Name | Engineering Discipline | Primary Capabilities | Execution Model | Input Formats | Output Formats | Resource Ceiling | Status |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **truck** | Mechanical CAD | Parametric B-Rep solid modeling, booleans, tessellation | In-Process (Rust) | JSON, Part B-Rep | Triangulated Mesh, STL | Standard RAM | **PRODUCTION_READY** |
| **Built-in CAE** | Mechanical CAE | Voxel & tetrahedral FEA, linear static, modal, thermal | In-Process (Rust) | Triangulated Mesh, Material | Stress/Displacement Fields | Multi-threaded | **PRODUCTION_READY** |
| **Built-in CAM** | Manufacturing | 3-axis CNC contouring, pocketing, facing, toolpath simulation | In-Process (Rust) | 2D Profile / Mesh, Tool Library | G-code (LinuxCNC, GRBL, Mach3) | Fast stack | **PRODUCTION_READY** |
| **Open CASCADE (OCCT)**| Mechanical CAD | STEP AP242, IGES exchange, advanced fillets, curvature | C ABI / CLI worker | STEP, IGES, BREP | STEP, IGES, STL, glTF | Configurable | **ADAPTED** |
| **Gmsh** | Multiphysics Mesh | 2D/3D Delaunay, frontal tetrahedral mesh generation | Process-Isolated Subprocess | STEP, GEO, STL | MSH, VTK, INP | 4 Cores, 8 GB | **ADAPTED** |
| **CalculiX (`ccx`)** | Structural FEA | Non-linear static, dynamic contact, cyclic plasticity | Process-Isolated Subprocess | CalculiX `.inp` | FRD, DAT, VTK | 8 Cores, 16 GB | **ADAPTED** |
| **OpenFOAM** | Fluid Dynamics (CFD)| Incompressible/compressible Navier-Stokes, k-epsilon turbulence | Process-Isolated Subprocess | BlockMesh/SnappyHexMesh Case | OpenFOAM field data, VTK | Multi-core MPI | **ADAPTED** |
| **SU2** | Aerodynamics | Transonic airfoil analysis, Euler/RANS, lift/drag polar | Process-Isolated Subprocess | `.su2` mesh, `.cfg` config | Surface CSV, Flow VTU | Multi-core | **ADAPTED** |
| **IfcOpenShell** | Architecture & BIM | IFC2x3 / IFC4 OpenBIM parsing, geometric decomposition | Process-Isolated / C ABI | `.ifc`, `.ifcxml` | Wavefront OBJ, glTF, JSON | 4 GB RAM | **ADAPTED** |
| **OpenSees** | Civil & Structural | Non-linear frame analysis, seismic response spectra | Process-Isolated CLI | OpenSees Tcl / Python script | Displacement time-history | 2 Cores | **ADAPTED** |
| **EnergyPlus** | Building Physics | Whole-building annual energy consumption, HVAC thermal | Process-Isolated CLI | EnergyPlus IDF | CSV, SQL results | Single-thread | **ADAPTED** |
| **ngspice** | Electronics / ECAD | Transient, AC small-signal, DC operating point SPICE | C ABI / Process-Isolated | SPICE Netlist (`.cir`) | Raw vector data, CSV | Fast stack | **ADAPTED** |
| **KiCad CLI** | Electronics / ECAD | PCB design rule check (DRC), Gerber generation | Process-Isolated CLI | `.kicad_pcb`, `.kicad_sch` | Gerber, Excellon drill | 2 GB RAM | **ADAPTED** |
| **pandapower** | Electrical Power | Balanced 3-phase AC load flow, short circuit calculation | Python Subprocess IPC | JSON grid topology | Bus voltages, branch loads | 2 GB RAM | **ADAPTED** |
| **OpenDSS** | Electrical Power | Distribution feeder simulation, PV/storage hosting capacity | C ABI / Subprocess | OpenDSS script | Line losses, profile charts | 4 GB RAM | **ADAPTED** |
| **Cantera** | Chemical Engineering| Gas-phase kinetics, equilibrium, 1D laminar flames | Python / C++ Bridge | YAML / CTI mechanism | Species mole fractions | Single-thread | **ADAPTED** |
| **DWSIM** | Chemical Engineering| Steady-state chemical process flowsheets, distillation | Process-Isolated CLI | DWSIM flowsheet XML | Stream compositions, enthalpy | 4 Cores, 8 GB | **ADAPTED** |
| **RDKit** | Molecular Modeling | 2D SMILES parsing, 3D conformer generation, MMFF94 forcefield | C++ ABI / Subprocess | SMILES, SDF, MOL | PDB, 3D MOL coordinates | 2 GB RAM | **ADAPTED** |
| **OpenVSP** | Aerospace | Parametric aircraft geometry, vortex lattice aerodynamics | Process-Isolated CLI | `.vsp3` geometric definition | Aero polar data, STL | 4 Cores | **ADAPTED** |
| **JSBSim** | Aerospace | 6-DOF atmospheric flight dynamics simulation | C++ / CLI Subprocess | Flight dynamics model XML | State telemetry CSV | Real-time | **ADAPTED** |
| **MuJoCo** | Robotics | Articulated multibody contact physics, forward/inverse dynamics | C ABI / Native Wrapper | MJCF XML, URDF | Generalized coordinates $q$, $\tau$ | 60 FPS real-time | **ADAPTED** |
| **EPANET** | Environmental | Water distribution hydraulics, water age, chlorine decay | C ABI / Process-Isolated | EPANET `.inp` | Junction pressures, pipe flows | Fast stack | **ADAPTED** |
| **EPA SWMM** | Environmental | Stormwater urban catchment runoff, sewer routing | C ABI / Process-Isolated | SWMM `.inp` | Hydrographs, overflow volume | Fast stack | **ADAPTED** |
| **MODFLOW** | Geoscience | 3D finite-difference groundwater flow & head calculation | Process-Isolated Subprocess | MODFLOW 6 package files | Head arrays, cell budgets | 4 GB RAM | **ADAPTED** |
