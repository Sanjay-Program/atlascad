# Engine Integration Status Registry

Status is recorded against what was actually executed. The previous version of this registry marked
21 external engines `ADAPTED` with every gate `PASS`; no adapter or solver run backed those rows, so
they have been replaced with the measured state below.

## Status vocabulary

| Status | Meaning |
| --- | --- |
| **VERIFIED (built-in)** | AtlasCAD's own code runs the case and a workspace test or the live demo asserts the result. |
| **BENCHMARK ADAPTER** | A real installed solver is executed for a deterministic benchmark through `/api/engines/benchmark` and `/api/engines/health`; the solver's own output is parsed. That is the binary itself for CalculiX, ngspice and Gmsh, and a separate interpreter running the pandapower or Cantera library for those two. General job execution does not exist, so `/api/engines/run` refuses with `501`. |
| **NOT IMPLEMENTED** | Registry entry and installation guide only. No binary is invoked; `/api/engines/run` and `/api/engines/benchmark` return `501` with "no solver was run". |

## Registry (24 engines)

| Engine | Discipline | Execution adapter | `/api/engines/run` | Evidence |
| --- | --- | --- | --- | --- |
| `truck` | Mechanical CAD / geometry kernel | **VERIFIED (built-in)** | n/a (crate API) | Workspace tests; parametric rebuild and export checks |
| Built-in CAE (`cad-cae`) | Linear static, modal, steady thermal | **VERIFIED (built-in)** | n/a (crate API) | Analytic comparisons in `cad-cae` tests and the live demo |
| Built-in CAM (`cad-cam`) | 3-axis toolpaths, G-code | **VERIFIED (built-in)** | n/a (crate API) | `cad-cam` tests; live-demo toolpath and G-code assertions |
| `calculix` | Structural FEA | **BENCHMARK ADAPTER + document solve** (`atlas-engine-manager::calculix`) | Refuses (`501`) | Real `ccx` 2.23 axial-tension run, 0.404% vs `F L/(A E)`; `/api/engines/solve-document` solves the active model's Gmsh mesh (2,400 N, reactions balanced to 7.5e-7 %); see [`ENGINE_BENCHMARKS.md`](../validation/ENGINE_BENCHMARKS.md) |
| `ngspice` | Electronics / SPICE | **BENCHMARK ADAPTER** (`atlas-engine-manager::ngspice`) | Refuses (`501`) | Real `ngspice` 41 RC step transient, 0.0101% vs `1 − exp(−t/RC)` |
| `gmsh` | Meshing | **BENCHMARK ADAPTER + document meshing** (`atlas-engine-manager::gmsh`) | Refuses (`501`) | Real `gmsh` 4.15.2: cube benchmark 726 tetrahedra at 1000 mm³, and `/api/engines/mesh-document` meshes the active model (6,369 tetrahedra, 12,000 mm³ within 3e-14%); the same mesh is handed to CalculiX by `/api/engines/solve-document` |
| `openfoam` | CFD | **NOT IMPLEMENTED** | Refuses (`501`) | No binary, no adapter |
| `su2` | Aerodynamics | **NOT IMPLEMENTED** | Refuses (`501`) | No binary, no adapter |
| `ifcopenshell` | BIM | **NOT IMPLEMENTED** | Refuses (`501`) | No binary, no adapter |
| `opensees` | Civil / seismic | **NOT IMPLEMENTED** | Refuses (`501`) | No binary, no adapter |
| `energyplus` | Building energy | **NOT IMPLEMENTED** | Refuses (`501`) | No binary, no adapter |
| `kicad` | PCB | **NOT IMPLEMENTED** | Refuses (`501`) | No binary, no adapter |
| `pandapower` | Power systems | **BENCHMARK ADAPTER** (`atlas-engine-manager::pandapower`) | Refuses (`501`) | Real pandapower 3.2.1 two-bus power flow, 2.5e-14 % from the exact voltage root, 5 iterations |
| `opendss` | Distribution | **NOT IMPLEMENTED** | Refuses (`501`) | No binary, no adapter |
| `cantera` | Chemical kinetics | **BENCHMARK ADAPTER** (`atlas-engine-manager::cantera`) | Refuses (`501`) | Real Cantera 3.2.0 thermochemistry: methane LHV 802.557 kJ/mol vs the standard 802.3 (0.032 %), flame temperature 2,224.6 K |
| `dwsim` | Process flowsheet | **NOT IMPLEMENTED** | Refuses (`501`) | No binary, no adapter |
| `rdkit` | Molecular chemistry | **NOT IMPLEMENTED** | Refuses (`501`) | No binary, no adapter |
| `openvsp` | Aerospace design | **NOT IMPLEMENTED** | Refuses (`501`) | No binary, no adapter |
| `jsbsim` | Flight dynamics | **NOT IMPLEMENTED** | Refuses (`501`) | No binary, no adapter |
| `mujoco` | Robotics dynamics | **NOT IMPLEMENTED** | Refuses (`501`) | No binary, no adapter |
| `epanet` | Water hydraulics | **NOT IMPLEMENTED** | Refuses (`501`) | No binary, no adapter |
| `epaswmm` | Stormwater | **NOT IMPLEMENTED** | Refuses (`501`) | No binary, no adapter |
| `modflow` | Groundwater | **NOT IMPLEMENTED** | Refuses (`501`) | No binary, no adapter |
| `occt` | STEP interchange | **NOT IMPLEMENTED** | Refuses (`501`) | STEP read/write is handled by `cad-io`, not by OCCT |

## Summary

- **3** built-in engines verified by tests.
- **5** external engines with a real, benchmarked adapter (CalculiX, ngspice, Gmsh, pandapower,
  Cantera); two of them are library adapters driven through a separate Python interpreter.
- **16** registry entries with no execution adapter; they refuse instead of returning a result.

Adding an engine means implementing a process adapter that runs the real binary and parses its
output, then verifying it against a closed-form or published reference. Registering a row in the
engine list is not integration.
