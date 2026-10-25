# ADR-0001: Share a contract, do not merge the applications

- **Status:** Accepted
- **Date:** 2026-10-21
- **Deciders:** engineering (this audit)

## Context

The workspace contains two mature Rust CAD applications with different domains:

* **CADCraft** — 2D drafting, 12 crates, DXF/DWG, constraints, plotting; no 3D kernel.
* **SolidCraft** — parametric 3D, 13 crates, truck-backed B-rep kernel, feature timeline, STEP/IGES.

They share no code: two vector types (`cadcraft-geom::Vec3`, `solidcraft-geom`), two document models
(`Handle` + copy-on-write `EntityStore` vs a parametric timeline), two undo strategies, two unit
conventions (`INSUNITS` vs unit-aware expressions), and two `ui-egui`/`engine` crates with the same
roles and incompatible APIs.

The brief proposes a `crates/cad-*` tree. Read literally, that means moving both applications into one
workspace and unifying their document models.

## Decision

**Do not merge the applications or their document models.** Introduce a third, small crate
(`cad-contract`) that both can depend on, containing vocabulary only: units, stable IDs, validated
tolerances, capability discovery, a kernel error taxonomy, a plain mesh interchange type, a minimal
kernel trait, and a versioned document header with migrations.

Each application keeps its own document model, renderer, and release cadence. Adoption happens one
crate at a time.

## Reasons

1. **The document models encode domain knowledge.** SolidCraft's timeline has incremental evaluation,
   feature references, face anchors and lineage; CADCraft's `EntityStore` is a copy-on-write structure
   tuned for cheap undo snapshots. A shared document model would have to be one of them (destroying
   the other) or a union of both (unmaintainable).
2. **Merging multiplies risk with no measured benefit.** A workspace-wide build already peaks at
   2.56 GB for one crate here; a merged tree would be unbuildable on this machine, and neither app's
   test suite (2 212 + 5 884 tests) has been run in full.
3. **A contract is cheaper to change than an implementation.** If the interface is wrong, it is a few
   hundred lines to revise; if a merged document model is wrong, it is years.
4. **Kernel swap stays possible.** With the contract, a second kernel is an adapter, not a migration.

## Consequences

* Some concepts remain duplicated (two vector types, two undo implementations) until a measured reason
  to unify appears. This is accepted.
* A shared shell (one window hosting both) is *possible* — both use egui 0.36 and wgpu 30 — but is not
  proposed, because no product requirement justifies it yet.
* Cross-application features (e.g. "insert a 3D part into a 2D drawing") require explicit interchange
  through the contract, not shared internals. That is the intended cost.
