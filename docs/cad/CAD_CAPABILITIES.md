# CAD Capabilities

**As of:** 2026-10-31 · **Status source:** [`docs/roadmap/FEATURE_IMPLEMENTATION_STATUS.md`](../roadmap/FEATURE_IMPLEMENTATION_STATUS.md)

This page lists what the CAD side of AtlasCAD can actually do in this workspace. It is not a
capability wish list, and it is not a comparison to commercial products. A feature appears here only
when code, a test and a recorded limitation exist. The detailed research matrix remains
[`docs/research/CAD_CAPABILITY_MATRIX.md`](../research/CAD_CAPABILITY_MATRIX.md).

## Implemented and tested

| Area | Capability | Evidence |
|---|---|---|
| Document | `.zcad` schema 9: atomic save, schema-1…8 migration, unknown/newer-schema refusal, named revisions, crash recovery, last-good backup, embedded derived-artifact registry | 81 `cad-document` tests |
| Sketch | Rectangle, circle, regular polygon, drawn closed polyline, arc sector; each on a horizontal plane at any height | analytic area/volume tests over HTTP |
| Modelling | Extrude as new body / join / cut, upwards or downwards; full-turn revolve about a sketch axis | analytic volumes through the Truck kernel; watertight STL exports |
| History | Ordered feature graph with stable IDs, dependency refusal, per-feature rebuild status, per-prefix rebuild cache, undo/redo | kernel-call-count tests; 0.01 ms unchanged / 7.9 ms last-of-six |
| Measurement | Mesh volume, surface area, triangle/vertex counts, bounding box; face picking in the viewport | `GET /api/state`, browser checks |
| Exchange | Binary STL export; DXF R12 read and write (2D drafting), PDF sheet plot | `cad-io`, `cad-drafting`, HTTP and CLI tests |
| Drawing | Plan of the model, editable drawing document with layers, text, linear dimensions, radial/diameter dimensions, DXF, A4–A0/Letter titled PDF | 64 `cad-drafting` tests, CLI and browser workflows |
| AutoCAD Drafting | Fillet, Chamfer, Offset, Trim, Extend, Join, Array, Polygon, Mirror, Rotate, Scale, Move, Copy, Explode, Blocks, Inserts, Linetypes, Purge, Overkill, Divide, Measure, MatchProp, Crosshatch ANSI31/NET, 4-view orthographic generator, Section properties | 64 `cad-drafting` tests, 44 `cad-studio` tests |
| 2D Constraints | Pure Rust 2D parametric geometric & dimensional constraint engine with Levenberg-Marquardt solver, QR rank analysis, and DOF diagnostic reports | `cad-constraints`, studio `/api/draft/constraints/solve` |
| Multi-Body | Named independent solid bodies (`body_id`), targeted feature operations, inter-body clearance distance measurement | `cad-document`, tests in `multi_body_tests` |
| Patterns & Mirrors | 3D feature arrays (linear cut array, circular radial cut array, planar mirror cut) with rebuild cache invalidation | `cad-document`, tests in `pattern_tests` |
| Display | WebGL viewport with orbit/pan/zoom/fit/standard views, interactive orientation gizmo (Top/Right/Front/Iso), status bar controls (GRID, WIRE, OSNAP, POLAR), wireframe, picking | 146 headless CDP browser assertions, zero console errors |
| Identity | Document id, revision, units, tolerance, stable entity ids, entity-level diff (`diff_revision`) | `cad-contract`, `.zcad` schema |
| Derived-result tracking | A drawing or simulation is pinned to the model revision it came from; stale detection propagates transitively; per-kind release policy enforcement | `cad-engineering-model`, `GET /api/engineering`, `POST /api/engineering/review`, `atlas artifacts` |

## Known limits that define the boundary

* **Assemblies & joints.** Multi-body parts are supported with clearance measurements; 3D kinematics/assembly joints and BOM tables are not yet implemented.
* **No 3D B-Rep fillet, chamfer, shell, sweep or loft.** Fillets, chamfers and offsets are fully supported on 2D profiles and sketches; 3D B-Rep edge filleting is absent.
* **No STEP/IGES/3MF/OBJ/glTF/IFC interchange.** DXF (R12) and binary STL are the geometry formats written; DXF is read.
* **Kernel limits are recorded, not hidden.** A boss fully inside a plate, tangent contact,
  three-operand chains and coplanar difference remain known failures in
  [`docs/kernel-comparison.md`](../kernel-comparison.md). The application works around the coplanar
  cases it can (cut overshoot, join overlap) and refuses the rest with a reason.
* **No freeform/NURBS solid authoring.** Surfaces and splines are part of the drafting document, but not 3D freeform NURBS solid modeling.

## Licensing and kernel

The shipping geometry path uses Truck crates through `cad-kernel-truck`, with the patched
`truck-shapeops`/`truck-geometry`/`truck-meshalgo` vendors under `apps/solidcraft/vendor`. The
licence audit and the LGPL/OCCT exclusion decision are in
[`docs/integration/license-audit.md`](../integration/license-audit.md) and ADR-0003. No OpenCASCADE
code is part of the shipping graph.
