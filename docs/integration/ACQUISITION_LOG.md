# AtlasCAD — Upstream Acquisition Log

## What was actually done

Earlier versions of this log recorded "Passed Gates A–D" for a dozen engines. Those gates were not
run: no engine binary was built, wrapped or validated at the time. What was done is provenance
research — canonical repository, pinned release, SPDX licence — and, for two engines, a real
execution adapter.

| Upstream ID | Canonical URL | Pinned reference | Licence | State |
| --- | --- | --- | --- | --- |
| `truck` | `github.com/ricosjp/truck` | `0.6.0` (`78d699b`) | Apache-2.0 | **Vendored and built**; powers `cad-kernel-truck` |
| `calculix` | `github.com/Dhondtguido/CalculiX` | 2.23 (conda-forge build) | GPL-2.0 | **Benchmark adapter**; real tension-bar run, 0.404% vs theory |
| `ngspice` | `github.com/ngspice/ngspice` | 41 (conda-forge build) | BSD-3-Clause / LGPL | **Benchmark adapter**; real RC transient, 0.0101% vs theory |
| `openfoam`, `su2`, `gmsh`, `pandapower`, `opendss`, `ifcopenshell`, `mujoco`, `openvsp`, `epanet` | see [`UPSTREAM_REPOSITORY_REGISTRY.md`](UPSTREAM_REPOSITORY_REGISTRY.md) | as pinned there | as recorded there | **Provenance only**; no adapter, `/api/engines/*` refuses with `501` |

## Gate definitions (not yet applied to the provenance-only engines)

- **Gate A (Provenance):** canonical repository, authors and pinned release confirmed.
- **Gate B (Licensing):** SPDX identification and attribution obligations recorded.
- **Gate C (Buildability):** reproducible build on Linux x86_64 — *only `truck` passes this today*.
- **Gate D (Interface):** a working process/ABI adapter that parses real output — *only CalculiX and
  ngspice pass this today*.
- **Gate E (Numerical validity):** a recorded comparison against an analytic or published reference
  — *only CalculiX and ngspice have one*.

A gate is only marked passed when the command and its output are recorded in the repository.
