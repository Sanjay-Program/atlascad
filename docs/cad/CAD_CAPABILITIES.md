# CAD Capabilities

**As of:** 2026-10-31 · **Status source:** [`docs/roadmap/FEATURE_IMPLEMENTATION_STATUS.md`](../roadmap/FEATURE_IMPLEMENTATION_STATUS.md)

This page lists what the CAD side of AtlasCAD can actually do in this workspace. It is not a
capability wish list, and it is not a comparison to commercial products. A feature appears here only
when code, a test and a recorded limitation exist. The detailed research matrix remains
[`docs/research/CAD_CAPABILITY_MATRIX.md`](../research/CAD_CAPABILITY_MATRIX.md).

## Implemented and tested

| Area | Capability | Evidence |
|---|---|---|
| Document | `.zcad` schema 8: atomic save, schema-1…7 migration, unknown/newer-schema refusal, named revisions, crash recovery, last-good backup | 78 `cad-document` tests |
| Sketch | Rectangle, circle, regular polygon, drawn closed polyline, arc sector; each on a horizontal plane at any height | analytic area/volume tests over HTTP |
| Modelling | Extrude as new body / join / cut, upwards or downwards; full-turn revolve about a sketch axis | analytic volumes through the Truck kernel; watertight STL exports |
| History | Ordered feature graph with stable IDs, dependency refusal, per-feature rebuild status, per-prefix rebuild cache, undo/redo | kernel-call-count tests; 0.01 ms unchanged / 7.9 ms last-of-six |
| Measurement | Mesh volume, surface area, triangle/vertex counts, bounding box; face picking in the viewport | `GET /api/state`, browser checks |
| Exchange | Binary STL export; DXF R12 read and write (2D drafting), PDF sheet plot | `cad-io`, `cad-drafting`, HTTP and CLI tests |
| Drawing | Plan of the model, editable drawing document with layers, text, linear dimensions, DXF, A4–A0/Letter titled PDF | 60 `cad-drafting` tests, CLI and browser workflows |
| Display | WebGL viewport with orbit/pan/zoom/fit/standard views, wireframe, grid, picking | headless-browser assertions |
| Identity | Document id, revision, units, tolerance, stable entity ids | `cad-contract`, `.zcad` schema |
| Derived-result tracking | A drawing is pinned to the model revision it came from; a saved drawing becomes visibly stale after a model edit | `cad-engineering-model`, `GET /api/engineering`, `/api/draft` |

## Known limits that define the boundary

* **One body.** A project has one solid body. Multi-body design and assemblies are absent.
* **No fillet, chamfer, shell, sweep, loft, pattern or mirror.** The contract may declare some of
  them; no adapter implements them, so they are not offered.
* **No sketch constraints.** The `cad-constraints` directory contains only a manifest; there is no
  solver.
* **No STEP/IGES/3MF/OBJ/glTF/IFC interchange.** DXF (R12) and binary STL are the only geometry
  formats written; DXF is the only one read.
* **Kernel limits are recorded, not hidden.** A boss fully inside a plate, tangent contact,
  three-operand chains and coplanar difference remain known failures in
  [`docs/kernel-comparison.md`](../kernel-comparison.md). The application works around the coplanar
  cases it can (cut overshoot, join overlap) and refuses the rest with a reason.
* **A drawing is one plan view on one page.** No projected, section, detail or auxiliary views, no
  GD&T, no BOM, no sheet set, no batch plot.
* **No freeform/NURBS authoring.** Surfaces and splines are not part of the document model. Do not
  read the presence of truck surface crates as a user-facing surface-modelling feature.

## Licensing and kernel

The shipping geometry path uses Truck crates through `cad-kernel-truck`, with the patched
`truck-shapeops`/`truck-geometry`/`truck-meshalgo` vendors under `apps/solidcraft/vendor`. The
licence audit and the LGPL/OCCT exclusion decision are in
[`docs/integration/license-audit.md`](../integration/license-audit.md) and ADR-0003. No OpenCASCADE
code is part of the shipping graph.
