# CAE Capabilities

**As of:** 2026-10-31 · **Status:** not implemented.

There is **no solver and no simulation workspace** in this repository. The material library and the
result-provenance registry are foundations, not analysis.

## What exists

| Thing | What it is | What it is not |
|---|---|---|
| `cad-engineering-model::MaterialLibrary` | seven validated materials with typical handbook properties and an explicit "not certified" source | a qualified material database; no temperature-dependent curves, plasticity, fatigue or creep data |
| `ArtifactKind::Simulation` | a record that pins a study's summary to a source revision and upstream artifacts | a study definition, mesh, boundary condition, solver run or result field |
| `atlas artifacts check` | prevents a stale study from being released silently | verification that a study was ever physically valid |
| `GET /api/engineering` | exposes the library and the drawing's staleness, not a study | a results visualiser |

No linear static, modal, thermal, buckling, nonlinear, dynamic, injection-moulding or electronics-
cooling analysis exists. No mesh generator for analysis exists. No solver is integrated, and no
reference-solution suite exists.

## Required shape of the first real analysis

The smallest honest Phase D deliverable is a **linear static study on the existing tessellated
solid**, because the platform already produces a watertight mesh and stable entity ids:

1. A persisted study definition: geometry reference, material assignment, constraints, loads, mesh
   parameters, solver identity and version.
2. A mesh built from the solid with a stated element type and quality report; tet meshing of the
   tessellation is acceptable only with its geometric error recorded.
3. A linear elastic solver with documented assumptions (small strain, isotropic, homogeneous,
   linear boundary conditions).
4. Results that identify geometry revision, material, mesh, solver/version, load cases, convergence
   status, warnings and numerical assumptions — exactly the fields the unified model already has
   slots for.
5. A reference suite: a cantilever tip deflection against `PL³/(3EI)`, a plate with a known
   analytical stress concentration, patch tests, and a mesh-convergence series. A solver without
   these is a demo.
6. A quality label on every result: *educational estimate*, *preliminary engineering study*, or
   *validated workflow*. No result may be labelled certified.

Modal, thermal and buckling follow only after linear static is trustworthy. Nonlinear, dynamic and
injection moulding require an established external solver and licensing review; writing a homemade
nonlinear solver is not acceptable. Electronics cooling requires a validated thermal/CFD engine and
the 3D PCB assembly that does not exist yet.

## Explicit non-claims

* Material values are typical, not certified.
* No safety factor or failure prediction may be reported until a solver exists and is benchmarked.
* No result is a certified engineering conclusion, and this page does not imply otherwise.

Acceptance and evidence rules are in
[`docs/quality/ENGINEERING_VALIDATION.md`](../quality/ENGINEERING_VALIDATION.md).
