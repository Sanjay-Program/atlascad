# CAD Capabilities

**As of:** 2026-10-31 · **Status source:** [`docs/roadmap/FEATURE_IMPLEMENTATION_STATUS.md`](../roadmap/FEATURE_IMPLEMENTATION_STATUS.md)

This page lists what the CAD side of AtlasCAD can actually do in this workspace. It is not a
capability wish list, and it is not a comparison to commercial products. A feature appears here only
when code, a test and a recorded limitation exist. The detailed research matrix remains
[`docs/research/CAD_CAPABILITY_MATRIX.md`](../research/CAD_CAPABILITY_MATRIX.md).

## Implemented and tested

| Area | Capability | Evidence |
|---|---|---|
| Document | `.zcad` schema 11: atomic save, schema-1…10 migration, unknown/newer-schema refusal, named revisions, crash recovery, last-good backup, embedded derived-artifact registry, and imported mesh bodies | 91 `cad-document` tests |
| Sketch | Rectangle, circle, regular polygon, drawn closed polyline, arc sector; each on a horizontal plane at any height | analytic area/volume tests over HTTP |
| Modelling | Extrude as new body / join / cut, upwards or downwards; revolve about a sketch axis at any angle from >0° to 360°, with partial turns capped at both ends | analytic volumes through the Truck kernel; watertight STL exports |
| History | Ordered feature graph with stable IDs, dependency refusal, per-feature rebuild status, per-prefix rebuild cache, undo/redo | kernel-call-count tests; 0.01 ms unchanged / 7.9 ms last-of-six |
| Measurement | Mesh volume, surface area, triangle/vertex counts, bounding box; face picking in the viewport | `GET /api/state`, browser checks |
| Exchange | Binary STL export; STEP AP214 faceted export **and import** (`atlas step-import`, studio `/api/import/step`); DXF R12 read and write (2D drafting), PDF sheet plot | `cad-io`, `cad-drafting`, HTTP and CLI tests |
| Imported bodies | A STEP part becomes a named mesh body in `.zcad` (schema 11), merged into the built geometry, measured, exported and undoable; it has no sketch or feature and is not parametric | `cad-document` imported-body tests; studio HTTP import/undo/remove test |
| Drawing | Plan of the model, editable drawing document with layers, text, linear dimensions, radial/diameter dimensions, DXF, A4–A0/Letter titled PDF | 64 `cad-drafting` tests, CLI and browser workflows |
| AutoCAD Drafting | Fillet, Chamfer, Offset, Trim, Extend, Join, Array, Polygon, Mirror, Rotate, Scale, Move, Copy, Explode, Blocks, Inserts, Linetypes, Purge, Overkill, Divide, Measure, MatchProp, Crosshatch ANSI31/NET, 4-view orthographic generator, Section properties | 64 `cad-drafting` tests, 44 `cad-studio` tests |
| 2D Constraints | Pure Rust 2D parametric geometric & dimensional constraint engine with Levenberg-Marquardt solver, QR rank analysis, and DOF diagnostic reports | `cad-constraints`, studio `/api/draft/constraints/solve` |
| Multi-Body | Named independent solid bodies (`body_id`), targeted feature operations, inter-body clearance distance measurement, and pairwise interference / collision clash detection (`INTERFERE`) | `cad-document`, tests in `multi_body_tests`, `/api/interference` |
| Mass Properties | Analytical divergence-theorem mass properties, centroid, inertia tensor, Jacobi 3x3 principal moments/axes, radii of gyration, physical mass scaled with material density (`MASSPROP`) | `cad-contract`, `cad-kernel-truck`, tests in `mesh.rs`, `/api/mass-properties` |
| Bill of Materials | Assembly Bill of Materials (BOM) itemizing components, bounding box dimensions, materials, volume, and physical mass rollups (`BOM`) | `cad-document`, `cad-studio`, `/api/bom`, `atlas bom` |
| Patterns & Mirrors | 3D feature arrays (linear cut array, circular radial cut array, planar mirror cut) with rebuild cache invalidation | `cad-document`, tests in `pattern_tests` |
| Display | WebGL viewport with orbit/pan/zoom/fit/standard views, interactive orientation gizmo (Top/Right/Front/Iso), status bar controls (GRID, WIRE, OSNAP, POLAR), wireframe, picking | 146 headless CDP browser assertions, zero console errors |
| Identity | Document id, revision, units, tolerance, stable entity ids, entity-level diff (`diff_revision`) | `cad-contract`, `.zcad` schema |
| Derived-result tracking | A drawing or simulation is pinned to the model revision it came from; stale detection propagates transitively; per-kind release policy enforcement | `cad-engineering-model`, `GET /api/engineering`, `POST /api/engineering/review`, `atlas artifacts` |

## Known limits that define the boundary

* **Assemblies & joints.** Multi-body parts are supported with clearance measurements, pairwise clash detection (`INTERFERE`), and automated Bill of Materials (`BOM`) generation; 3D kinematics/assembly mate joints are not yet implemented.
* **No 3D B-Rep fillet, chamfer, shell, sweep or loft.** Fillets, chamfers and offsets are fully supported on 2D profiles and sketches; 3D B-Rep edge filleting is absent.
* **Interchange is STL, DXF and faceted STEP.** Binary STL is written and read; DXF R12 is written and read (2D drafting); STEP AP214 `CONFIG_CONTROL_DESIGN` is written by the studio and read by `atlas step-import` as a **faceted** boundary representation (one planar face per triangle, `MANIFOLD_SOLID_BREP` only when the mesh is watertight). Analytic STEP surfaces (`B_SPLINE_SURFACE`, cylindrical, toroidal), `EDGE_LOOP` B-reps, assemblies, IGES, 3MF read, OBJ, glTF and IFC are absent. No STEP file has been validated against a commercial reader on this machine.
* **Kernel limits are recorded, not hidden.** Booleans beyond ±1e5 mm are refused with a re-centre
  message, and the raw coplanar cut remains a known failure in
  [`docs/kernel-comparison.md`](../kernel-comparison.md).
  Boolean chains complete through a documented 3e-2 mm tolerance escalation (`boolean_escalations()`,
  0.003 % volume cost), and face-to-face or contained unions fuse through a 1e-3 mm inward retry. The
  application works around the coplanar cases it can (cut overshoot, join overlap) and refuses the
  rest with a reason.
* **No freeform/NURBS solid authoring.** Surfaces and splines are part of the drafting document, but not 3D freeform NURBS solid modeling.

## Licensing and kernel

The shipping geometry path uses Truck crates through `cad-kernel-truck`, with the patched
`truck-shapeops`/`truck-geometry`/`truck-meshalgo` vendors under `apps/solidcraft/vendor`. The
licence audit and the LGPL/OCCT exclusion decision are in
[`docs/integration/license-audit.md`](../integration/license-audit.md) and ADR-0003. No OpenCASCADE
code is part of the shipping graph.
