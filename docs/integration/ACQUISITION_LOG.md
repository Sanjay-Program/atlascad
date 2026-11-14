# AtlasCAD — Upstream Acquisition Log

## Acquisition Gates & Verification Record

All upstream repositories are evaluated against five formal quality and legal gates:
- **Gate A (Provenance):** Verified official canonical repository, original authors, and upstream security advisories.
- **Gate B (Licensing):** Clean SPDX license identification, absence of unmanaged proprietary blobs, attribution rules checked.
- **Gate C (Buildability):** Reproducible compilation with standard toolchains on Linux x86_64 and Docker.
- **Gate D (Interface & Adaptability):** Validated CLI/C ABI/IPC entry points; input formats documentable and parseable.
- **Gate E (Numerical Validity):** Reference benchmarks compared against analytical solutions.

---

## Log Entries

| Timestamp (UTC) | Upstream ID | Canonical URL | Pinned Reference | Gate Status | Architecture Decision |
|:---|:---|:---|:---|:---|:---|
| 2026-11-14 | `truck` | `github.com/ricosjp/truck` | `0.6.0` (`78d699b`) | Passed Gates A–E | Vendor patches in `apps/solidcraft` to fix face-division booleans; keep in-process |
| 2026-11-14 | `calculix` | `github.com/Dhondtguido/CalculiX` | `v2.21` | Passed Gates A–D | Tier C GPL-2.0 isolation via subprocess runner `ccx`; `.inp` generator wired |
| 2026-11-14 | `openfoam` | `github.com/OpenFOAM/OpenFOAM-dev` | `v11` | Passed Gates A–D | Tier C GPL-3.0 isolation via case directory generator; VTK output adapter |
| 2026-11-14 | `su2` | `github.com/su2code/SU2` | `v8.0.1` | Passed Gates A–D | Tier B LGPL-2.1; automated config generator + CLI solver invocation |
| 2026-11-14 | `gmsh` | `gitlab.onelab.info/gmsh/gmsh` | `4.13.1` | Passed Gates A–D | Automated `.geo` script generator with Delaunay tetrahedral meshing |
| 2026-11-14 | `ngspice` | `github.com/ngspice/ngspice` | `ngspice-43` | Passed Gates A–D | Netlist synthesis from 2D schematic; raw tabular output parser |
| 2026-11-14 | `pandapower` | `github.com/e2nIEE/pandapower` | `v2.14.11` | Passed Gates A–D | JSON model serializer + Python IPC bridge; balanced AC power flow |
| 2026-11-14 | `opendss` | `github.com/tshort/OpenDSS` | `v9.6.1` | Passed Gates A–D | High-speed C ABI `libdss_capi` distribution feeder analysis |
| 2026-11-14 | `ifcopenshell`| `github.com/IfcOpenShell/IfcOpenShell`| `v0.7.0` | Passed Gates A–D | IFC4 building schema extraction, geometric tessellation to OBJ/glTF |
| 2026-11-14 | `mujoco` | `github.com/google-deepmind/mujoco` | `3.1.5` | Passed Gates A–D | C API wrapper; kinematic forward dynamics and contact forces |
| 2026-11-14 | `openvsp` | `github.com/OpenVSP/OpenVSP` | `3.38.0` | Passed Gates A–D | VSP3 script generator for swept wings and fuselages |
| 2026-11-14 | `epanet` | `github.com/USEPA/EPANET` | `v2.2.0` | Passed Gates A–D | C ABI `libepanet2` pressurized pipe hydraulic and water age solver |
