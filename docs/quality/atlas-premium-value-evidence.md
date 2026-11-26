# AtlasCAD Premium Value Evidence

**Date:** 2026-11-26
**Question:** does AtlasCAD create defensible professional value today, and what evidence would a
buyer require? This document records verified workflows and measured numbers only; it contains no
invented ROI and no claim that a commercial product has been replaced.

---

## 1. Value evidence matrix

| Customer job | Current alternative | AtlasCAD capability | Verified workflow | Measured benefit | Evidence | Gap |
| --- | --- | --- | --- | --- | --- | --- |
| Model a parametric part | Inventor / SolidWorks / Onshape | profiles, extrude/join/cut/revolve, parameters, history, undo | create box → sketch → extrude → cut → save → reopen | 0.97 ms state read; 9 MB RSS; exact analytic volumes | 48 HTTP tests, 1,206 assertions | no constraints, fillets, shells, patterns, assemblies |
| Produce a drawing and PDF | AutoCAD / DraftSight | 2D editor, layers, DXF R12, titled PDF sheet | draw → dimension → DXF out → PDF plot | PDF text extracted; DXF re-read by the same reader | drafting tests, `pdftotext` | one plan view; no projected/section views, GD&T, sheet sets |
| Exchange geometry | STEP-centric PLM/CAM | STEP AP214 and STL write, STEP read as mesh body | export STEP → re-read → volume check | cube round-trip 500 mm³ | `cad-io` tests | faceted BREP; no analytic surfaces/assemblies; no IGES |
| Check a static design | ANSYS / Abaqus | linear-static FEA, modal, thermal | cantilever solve | analytic cantilever comparison | 213 simulation tests | no general mesher, contact, nonlinear, solver choice |
| Estimate a machining job | Mastercam / Fusion CAM | toolpaths, feeds, 3/4/5-axis tests, G-code | toolpath → postprocess | byte-level G-code regression | `cad-cam` tests | no machine/controller execution |
| Prepare a print | Cura / PrusaSlicer | FDM/SLA/SLM profiles, slice, supports, 3MF | slice → 3MF | profile + slice tests | `atlas-additive` tests | no printer link, limited preview evidence |
| Review with a colleague | Email + PDF | token-gated live session, 3D pins, leases | share → token → edit | 401/200 boundary, same-origin enforcement | 21 share assertions, browser | no accounts, no merge, no revocation without restart |
| Trace decisions | PLM / QMS | revisions, audit chain, command history, reports | edit → revision → report → audit | hash-chained audit tests; real command timestamps now | audit + report tests | no signatures, no IdP, no retention policy |
| Run an external solver | CalculiX / OpenFOAM install | engine registry and guides | — | — | 24 entries, 24 guides | **no adapter; jobs now return 501** |
| Hand off to Blender/Unity/Resolve | manual export | glTF/USD/EDL/FCPXML/LUT writers | export bytes | format tests | `atlas-interchange` | no host-application round trip |

---

## 2. What is genuinely differentiated

1. **Honesty as a product property.** The kernel corpus records known failures; engine jobs now
   refuse instead of inventing convergence; saves are atomic; errors are typed. This is rare and is
   the strongest foundation in the repository.
2. **Small operational footprint.** A 16.8 MB server, 9 MB RSS for a simple part, 99 ms start, and a
   single document file with no database.
3. **Breadth of analytical engineering models.** 140 analytical solvers with 213 unit tests, exposed
   through a single local API.
4. **Test density in critical logic.** 878 workspace tests, an 11-fixture analytic corpus, and a
   1,206-assertion product harness.

None of these is a substitute for assemblies, real authentication, or external solver adapters.

---

## 3. What a professional buyer would still require

- A real identity boundary (SSO/MFA, revocation, audit of sign-in).
- A published advisory process and SBOM from CI.
- A benchmark on a representative 10k-face assembly, not a box.
- Signed installers, an update channel, and a support SLA.
- A data-handling statement and backup/restore evidence.
- Independent verification of at least one solver result against a reference case.

---

## 4. Pricing position (recommendation, separate from technical results)

| Tier | Recommendation | Precondition |
| --- | --- | --- |
| Community / evaluation | free, source-available or binary | current state; label demo identity and absent engines clearly |
| Professional (individual) | low-cost, one-time or small subscription | PR-005 advisory scan; identity is demo-only but not needed for local use; 10k-model numbers now measured (PR-009) |
| Team / enterprise | only after hosted identity, audit retention, support and SLA exist | PR-004, PR-005, PR-011, signed artifacts |

**Do not price at $10,000/seat today.** The verified workflows support a credible low-cost local
tool, not an enterprise engineering subscription. Charging enterprise prices before the gaps in §3
are closed would convert an honest prototype into a trust problem.
