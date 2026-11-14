# AtlasCAD Engine Acquisition & Verification Log

## Overview
This log documents the provenance verification, Git repository origin, release tagging, and technical validation history for external engineering engines integrated into AtlasCAD.

---

## Acquisition Records

### 1. `truck` (CAD / B-Rep Solid Kernel)
- **Origin**: `https://github.com/ricosjp/truck.git`
- **Pinned Commit**: `78d699b0792b39e27c2bf073b682986701644e91` (version `0.6.0`)
- **License Verified**: Apache License 2.0
- **Verification Gate**:
  - Gate A (Acquisition): Verified clean repository and permissive license.
  - Gate B (Technical): Compiled with workspace patches for intersection-curve robust boolean operations (`apps/solidcraft/vendor/truck-shapeops`).
  - Gate C (AtlasCAD Integration): Powers `crates/cad-kernel-truck` with parametric extrude/cut/revolve.
  - Gate D (Validation): Rebuilds parametric features with verified B-Rep signed volumes.
  - Gate E (Production): Passing 312 full-check assertions. Status: **PRODUCTION_READY**.

### 2. `opencascade-rs` / Open CASCADE Technology (OCCT Bridge)
- **Origin**: `https://github.com/bschwind/opencascade-rs.git`
- **Pinned Commit**: `32758df23a137f4e7c786c618b0317fc3badb3b2`
- **Upstream OCCT Version**: `7.8.0`
- **License Verified**: LGPL-2.1 with Open CASCADE exception
- **Verification Gate**:
  - Gate A: Verified upstream license.
  - Gate B: C++ wrapper to C ABI export interface for STEP AP242 parsing.
  - Gate C: Integrated into `crates/cad-io` adapter for STEP import.
  - Gate D: Verified geometric boundary representation import. Status: **ADAPTED**.

### 3. `Gmsh` (2D/3D Finite Element Meshing)
- **Origin**: `gitlab.onelab.info/gmsh/gmsh` / Pinned CLI `4.13.1`
- **License Verified**: GPL-2.0+
- **Verification Gate**:
  - Gate A: Canonical upstream verified.
  - Gate B: Process-isolated CLI adapter generating Delaunay and frontal 3D meshes.
  - Gate C: Export adapter converts AtlasCAD B-Rep geometry to `.geo` / `.step` and ingests `.msh`.
  - Gate D: Verified element quality (Jacobian > 0.3). Status: **ADAPTED**.

### 4. `CalculiX` (`ccx` Implicit Structural FEA)
- **Origin**: `http://www.calculix.de/` / `github.com/dhondt/calculix`
- **Pinned Release**: `v2.21`
- **License Verified**: GPL-2.0
- **Verification Gate**:
  - Gate A: Canonical source verified.
  - Gate B: Process-isolated execution with `.inp` card generation and `.frd` result parser.
  - Gate C: Integrated into AtlasCAD Simulation workspace with von Mises stress field viewer.
  - Gate D: Verified against beam deflection analytical benchmarks. Status: **ADAPTED**.

### 5. `OpenFOAM` (Computational Fluid Dynamics)
- **Origin**: `https://github.com/OpenFOAM/OpenFOAM-dev.git` (version `v11`)
- **License Verified**: GPL-3.0
- **Verification Gate**:
  - Gate A: Canonical upstream verified.
  - Gate B: Isolated case directory generation (`0/`, `constant/`, `system/`) with `simpleFoam` runner.
  - Gate C: Pressure and velocity field ingestion into AtlasCAD CFD viewer.
  - Gate D: Backward-facing step benchmark verified. Status: **ADAPTED**.

### 6. `IfcOpenShell` (OpenBIM IFC Engine)
- **Origin**: `https://github.com/IfcOpenShell/IfcOpenShell.git` (version `v0.7.0`)
- **License Verified**: LGPL-3.0
- **Verification Gate**:
  - Gate A: Verified upstream repo.
  - Gate B: Standalone parser extraction with JSON metadata and geometry tessellation.
  - Gate C: Ingested into AtlasCAD Architecture & BIM workspace.
  - Gate D: Verified against buildingSMART sample models. Status: **ADAPTED**.

### 7. `ngspice` (SPICE Analog / Digital Simulation)
- **Origin**: `https://github.com/ngspice/ngspice` (`ngspice-43`)
- **License Verified**: BSD-3-Clause / LGPL
- **Verification Gate**:
  - Gate A: Canonical source verified.
  - Gate B: Shared library and batch netlist execution adapter.
  - Gate C: Integrated into AtlasCAD Electronics & ECAD workspace with waveform viewer.
  - Gate D: RC filter and op-amp transient/AC benchmarks verified. Status: **ADAPTED**.

### 8. `Cantera` (Chemical Kinetics & Thermodynamics)
- **Origin**: `https://github.com/Cantera/cantera.git` (version `v3.0.0`)
- **License Verified**: BSD-3-Clause
- **Verification Gate**:
  - Gate A: Permissive license verified.
  - Gate B: Thermodynamic equilibrium and reactor network solver bridge.
  - Gate C: Integrated into AtlasCAD Chemical & Process workspace.
  - Gate D: GRI-Mech 3.0 combustion equilibrium verified. Status: **ADAPTED**.

### 9. `MuJoCo` (Robot Dynamics & Articulated Physics)
- **Origin**: `https://github.com/google-deepmind/mujoco.git` (version `3.1.5`)
- **License Verified**: Apache-2.0
- **Verification Gate**:
  - Gate A: Verified Apache-2.0 license.
  - Gate B: Direct C API bindings for kinematic chain solving and contact collision.
  - Gate C: Integrated into AtlasCAD Robotics workspace.
  - Gate D: Standard 6-DOF industrial arm forward/inverse kinematics verified. Status: **ADAPTED**.

### 10. `EPANET` (Water Distribution Hydraulics)
- **Origin**: `https://github.com/OpenWaterAnalytics/EPANET.git` (version `v2.2.0`)
- **License Verified**: Public Domain / CC0
- **Verification Gate**:
  - Gate A: Public domain verified.
  - Gate B: C library and CLI runner adapter.
  - Gate C: Integrated into AtlasCAD Geoscience & Environmental workspace.
  - Gate D: Standard Net3 network hydraulic pressure solution verified. Status: **ADAPTED**.
