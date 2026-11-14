# AtlasCAD — Multidisciplinary Implementation Roadmap

## Overview

This roadmap defines the staged engineering path to evolve AtlasCAD into the definitive open-source-powered multidisciplinary engineering platform.

---

## Phase 0: Repository Foundation & Unified Architecture (Current Milestone)
- **Objective:** Establish the clean `atlas-*` crate architecture alongside existing production crates.
- **Deliverables:**
  - `atlas-core`: Central contracts, identifiers, error handling, events.
  - `atlas-dependency-graph`: Directed acyclic dependency graph with automated downstream invalidation.
  - `atlas-engine-manager`: Dynamic solver registration, process isolation, and health discovery.
  - `atlas-simulation-core`: Multidisciplinary simulation problem definition and convergence telemetry.
  - Complete architecture, integration, legal, and validation documentation.
  - Maintain 100% test pass rate (514 assertions) with 0 clippy warnings.

---

## Phase 1: Unified CAD & Geometry Foundation
- **Objective:** Consolidate B-Rep solid modeling, 2D drafting, and geometric constraints.
- **Deliverables:**
  - Full truck B-Rep solid kernel validation.
  - OCCT (Open CASCADE) C ABI bridge for STEP AP242 and IGES.
  - SolveSpace 2D/3D geometric constraint solver adapter.
  - Manifold watertight mesh boolean integration.
  - ACID document transactions and crash recovery.

---

## Phase 2: Mechanical Engineering & Structural FEA
- **Objective:** End-to-end CAD-to-FEA modeling and simulation workflows.
- **Deliverables:**
  - Automated tetrahedral/hexahedral meshing via Gmsh (`4.13.1`).
  - CalculiX (`ccx 2.21`) non-linear implicit structural solver adapter.
  - Static structural stress (Von Mises, Tresca, Principal stresses).
  - Modal dynamic vibration frequencies and mode shapes.
  - Steady-state and transient thermal conduction.
  - Multidisciplinary Result Viewer HUD with interactive scalar heatmaps.

---

## Phase 3: Computational Fluid Dynamics (CFD) & Aerodynamics
- **Objective:** Compressible and incompressible fluid dynamics.
- **Deliverables:**
  - OpenFOAM (`v11`) case generation and automated solver runner (`simpleFoam`, `pisoFoam`).
  - SU2 (`v8.0.1`) compressible external aerodynamics and aircraft lift/drag polar calculations.
  - Surface pressure distributions, skin friction, and streamline generation.
  - Coupling via preCICE for fluid-structure interaction (FSI).

---

## Phase 4: Electrical Power & Electronics (ECAD / SPICE)
- **Objective:** Integrated schematic capture, circuit simulation, and grid analysis.
- **Deliverables:**
  - ngspice (`ngspice-43`) analog, digital, and mixed-signal transient solvers.
  - KiCad (`8.0.2`) schematic DRC and PCB netlist export.
  - pandapower (`2.14.11`) balanced 3-phase AC load flow and IEC 60909 short-circuit analysis.
  - OpenDSS (`9.6.1`) distribution feeder simulation and renewable harmonics.

---

## Phase 5: Architecture, BIM & Civil Engineering
- **Objective:** OpenBIM coordination and civil infrastructure analysis.
- **Deliverables:**
  - IfcOpenShell (`0.7.0`) IFC4 building element extraction and spatial hierarchy.
  - EnergyPlus (`24.1.0`) whole-building thermal energy and HVAC sizing simulation.
  - OpenSees (`3.6.0`) non-linear seismic frame response and interstorey drift analysis.
  - EPA SWMM (`5.2.4`) urban stormwater runoff and drainage network simulation.
  - EPANET (`2.2.0`) pressurized drinking water distribution hydraulic heads and water age.

---

## Phase 6: Chemical, Process & Molecular Engineering
- **Objective:** Thermodynamics, process flowsheets, and molecular structures.
- **Deliverables:**
  - Cantera (`3.0.0`) chemical kinetics, combustion, and transport properties.
  - DWSIM (`8.6.0`) vapor-liquid equilibrium (VLE) flash calculations and distillation columns.
  - RDKit (`2024.03.1`) molecular structure conformers and cheminformatics descriptors.

---

## Phase 7: Robotics, Multibody Dynamics & Aerospace
- **Objective:** Kinematic chains, flight dynamics, and contact physics.
- **Deliverables:**
  - MuJoCo (`3.1.5`) multi-joint contact physics and forward dynamics.
  - OpenVSP (`3.38.0`) parametric aircraft vehicle geometry generation.
  - JSBSim (`1.2.0`) 6-DOF atmospheric flight dynamics and autopilot simulation.
  - URDF kinematic chain parsing and ROS2 workcell bridge.

---

## Phase 8: Manufacturing, CNC CAM & Additive
- **Objective:** Production-grade subtractive and additive toolpath generation.
- **Deliverables:**
  - 3-axis and 5-axis CNC roughing, pocketing, and contouring toolpaths.
  - Machine-specific ISO G-code postprocessors (Fanuc, Haas, Heidenhain, Siemens).
  - Taylor tool-life feeds & speeds optimization.
  - Layer slicing, support generation, and toolpath simulation for metal/polymer 3D printing.

---

## Phase 9: AI Assistance, Digital Twins & Enterprise
- **Objective:** Autonomous engineering workflows, collaboration, and edge execution.
- **Deliverables:**
  - Structured engineering intent planning and permission-gated execution.
  - Multi-user engineering project sync with operational transform/CRDTs.
  - Cloud HPC job submission for heavy CFD/FEA solvers.
  - Automated compliance reporting and regulatory audit trail generation.
