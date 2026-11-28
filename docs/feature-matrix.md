# Feature Matrix

> **Current (2026-11-27).** This replaces the historical 2026-10-30 review, which described an older
> tree (`apps/`, `kernels/`) that is not part of the deployable product. Status is claimed only where
> a test, benchmark or script exercises the capability on this host; the evidence column names it.
> The authoritative readiness assessment is
> [`quality/ATLASCAD_PRODUCTION_READINESS_AUDIT.md`](quality/ATLASCAD_PRODUCTION_READINESS_AUDIT.md).

**Status vocabulary — used strictly:**

| Status | Meaning |
|---|---|
| **Verified** | Exercised by a test, benchmark or script in this repository; the evidence path is named. |
| **Partial** | Works for a subset, or works with a measured limitation stated in the note. |
| **Absent** | Not implemented. |
| **Refused** | The endpoint exists but returns `Unavailable` instead of inventing a result. |

No row claims parity with AutoCAD, SolidWorks, Inventor or Onshape. No row claims a certification.

---

## Modelling and exchange

| Capability | Status | Note and evidence |
|---|---|---|
| Parametric solid history | **Verified** | Sketch-based extrude/cut/revolve, per-feature rebuild cache, rollback of a failed edit, undo/redo; workspace tests and `scripts/simulate.sh` assert analytic volumes |
| STEP import | **Verified** | 10,092-triangle STEP imports in 638 ms (`scripts/benchmark-large-model.py`) |
| Export: STEP, STL, 3MF, DXF, PDF | **Verified** | `scripts/live-demo.py` DEMO 2/4 generates and checks each file; release-gate validation re-reads them |
| 2D drafting editor | **Verified** | Entity, layer, constraint, dimension and paper-space routes; DXF/PDF output checked in the live demo |
| Sheet metal, fasteners, gears, springs | **Verified** | Analytical generators behind `/api/cad/*`; covered by workspace tests |
| Assemblies, mates, large-part hierarchy | **Absent** | One document, one body; `PR-014`/audit scope notes |

## Analysis, CAM and simulation

| Capability | Status | Note and evidence |
|---|---|---|
| Linear static, modal, steady thermal | **Verified** | `/api/cae/*`; the live demo reports 384 elements, 5.57 MPa, 8,198 Hz and a thermal solution |
| CAM toolpaths, G-code, multi-axis | **Verified** | Toolpath totals, G-code line count and 4/5-axis moves checked in `scripts/live-demo.py` |
| Analytical simulation library | **Verified** | 30 domains behind `/api/sim/multiphysics_suite`; status is derived from the responses and lists any failed domain |
| Analysis standards | **Partial** | Each formula names its basis (e.g. ASME B31, ICRP-103) but this is a formula reference, not an audit or certification |
| CalculiX (`ccx`) FEA | **Partial / verified** | Real process adapter: writes a tension-bar deck, runs `ccx`, parses displacements and compares with `F L/(A E)` (0.40% error on 2.23). `/api/engines/health` and `/api/engines/benchmark` use it; absent `ccx` returns 501 |
| ngspice circuit transient | **Partial / verified** | Real process adapter: writes an RC step netlist, runs `ngspice -b`, parses the waveform and compares it with `1 - exp(-t/RC)` (0.0101% max deviation on 41, 526 timepoints). `/api/engines/health` and `/api/engines/benchmark` use it; absent `ngspice` returns 501 |
| Gmsh meshing | **Partial / verified** | Real process adapter: writes a cube `.geo`, runs `gmsh -nt 1 -3`, parses MSH 2.2 and sums tetrahedron volumes (999.9999999999983 of 1000 mm³ on 4.15.2, 726 tetrahedra). Meshing only; no mesh-quality study |
| Document → volume mesh | **Verified** | `/api/engines/mesh-document` exports the active model's STL, subdivides it and meshes it with Gmsh: a 40×30×10 model gives 6,369 tetrahedra whose volume matches 12,000 mm³ to 3 × 10⁻¹⁴ % in 502 ms |
| Document → mesh → solve | **Verified** | `/api/engines/solve-document` runs the same Gmsh mesh through CalculiX: the 40×30×10 block under 2 MPa shows 2,400.0 N applied over the 1,200 mm² top face and 2,400.000018 N of clamped reaction (7.5 × 10⁻⁷ % residual), 1.0793 × 10⁻⁴ mm maximum axial displacement on 6,369 tetrahedra |
| pandapower power flow | **Partial / verified** | Python-library adapter: solves a two-bus network (1 pu slack, 0.1 + j0.1 pu line, 1 pu constant-power load) and matches the exact quadratic root, 0.8798668869081245 vs 0.8798668869081243 pu (2.5 × 10⁻¹⁴ %), 5 iterations on 3.2.1 |
| Cantera thermochemistry | **Partial / verified** | Python-library adapter: computes the methane lower heating value from GRI-Mech 3.0 species enthalpies, 802.557 kJ/mol against the standard 802.3 kJ/mol (0.032 %), Mayer and ideal-gas residuals zero to machine precision, and the adiabatic flame temperature 2,224.6 K on 3.2.0 |
| OpenSees cantilever statics | **Partial / verified** | Python-library adapter: 10 elasticBeamColumn elements, 1 N tip load on a 1,000 × 10 × 10 mm steel cantilever, tip 1.9047619047612312 mm against `P L³/(3 E I)` = 1.9047619047619047 mm (3.5 × 10⁻¹³ relative) on OpenSeesPy 3.8.0.0 |
| RDKit molecular properties | **Partial / verified** | Python-library adapter: benzene from SMILES gives C₆H₆, 78.046950192 Da against 78.0469501914 Da from exact atomic masses (7.7 × 10⁻¹⁰ %) and 78.114 g/mol against the standard atomic weights (1.8 × 10⁻¹⁴ %) on 2026.9.2 |
| MuJoCo free fall | **Partial / verified** | Python-library adapter: a 1 kg mass dropped from 10 m for 100,000 steps at 10 µs reaches 5.094950949999747 m against `z₀ − ½ g t²` = 5.095 m (9.6 × 10⁻⁴ %) on 4.0.1 |
| EPANET pipe hydraulics | **Partial / verified** | Python-library adapter through WNTR: a 1 km × 300 mm pipe at C = 100 gives a 97.10618591308594 m junction head against Hazen-Williams 97.10614270189 m (4.4 × 10⁻⁵ %) on 1.5.0 |
| IfcOpenShell wall geometry | **Partial / verified** | Python-library adapter: an IFC4 wall 2 × 3 × 0.2 m is built through the API, re-read through `ifcopenshell.geom` and its 12 triangles enclose 1.2000000000000002 m³ against the analytic 1.2 m³ on 0.9.0.post1 |
| OpenFOAM, SU2, EnergyPlus, other engines | **Refused** | 11 registry entries have no adapter; `/api/engines/*` returns 501 with "no solver was run". See `PR-008` |
| Regulatory certification / sign-off | **Absent** | `/api/report` returns a `DRAFT_NOT_RELEASED` report with measured volume and unassessed mass; no sign-offs are created |

## Agents and automation

| Capability | Status | Note and evidence |
|---|---|---|
| Task graphs, workflow templates, approvals | **Verified** | 16 templates; approval is opt-in and gated commands raise an approval request (`http_api` tests) |
| Honest refusal of unimplemented steps | **Verified** | CAD/CAM/export steps without adapters return `Unavailable`, mark the task failed and derive the workflow status; no fabricated toolpath, media or certificate payloads |
| Workflow provenance | **Verified** | Every task records `document_revision`; inspection reports measured volume, bounds and characteristic length. Solver inputs remain a fixed 38 m reference case (PR-010) |
| Iterative optimisation | **Partial** | Deterministic analytical mass/stress loop; `evidence_verified` is false and no compliance score is claimed |
| Geometry-to-solver binding | **Absent** | Requires the adapters in PR-008; the response states that inputs are not model-derived |

## Platform, security and delivery

| Capability | Status | Note and evidence |
|---|---|---|
| Identity and sessions | **Partial / demo only** | Login switches a local demo identity; `credential_verified` is false, SSO is refused, and shared instances refuse login (PR-004) |
| Token-gated sharing | **Verified** | Share mode binds `0.0.0.0` only with a token; 4 share assertions in `simulate.sh` |
| Audit trail | **Verified** | Double-round FNV-1a tamper-evident chain with integrity verification; exposed as traceability, not compliance |
| Collaboration presence, locks, comments | **Partial** | In-memory optimistic leases and comments; no CRDT merge or durable multi-user history |
| Persistence and recovery | **Partial** | Save/reopen, autosave recovery snapshot; named revision history is absent |
| Accessibility and contrast | **Verified** | 191/191 controls named; rendered-DOM contrast 0 failures in light and dark (`simulate.sh`) |
| Large-model performance | **Verified** | 10,092 triangles: import 638 ms, `/api/state` 9.3 ms, STL 3.3 ms, RSS 59 MB; headless frame sample 59.4 fps (SwiftShader) |
| Deterministic packaging, SBOM, CI | **Verified** | `scripts/package.sh`, licence policy, SBOM, advisories; CI green on `b4e342d` |
| Licensing, billing, entitlement provider | **Absent** | `cad-entitlements` is local policy only |
| Installer, code signing, update channel | **Absent** | Checksummed tarball only |

---

## What would change a status

1. PR-008: an engine adapter that executes and validates real output (or removal of the run UI).
2. PR-010: a document-geometry adapter that feeds the solvers, with the bound inputs recorded.
3. PR-004: a real identity provider before any hosted multi-user deployment.
4. Assembly support and durable collaboration history.
