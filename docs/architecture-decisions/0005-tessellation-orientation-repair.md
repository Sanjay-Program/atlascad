# ADR-0005: Weld and orient tessellations in the adapter, not in every consumer

- **Status:** Accepted
- **Date:** 2026-10-22
- **Deciders:** engineering (this cycle)
- **Evidence:** `crates/cad-kernel-truck/src/lib.rs` (`mesh_of`, `weld_by_position`,
  `orient_outward`), test `tessellation_is_a_closed_outward_manifold`,
  `engineering/quality/tests.json#t-cad-cli`, `engineering/quality/benchmarks.json#bm-cli-box`

## Context

Building the first real consumer of the contract (`crates/cad-cli`, which writes binary STL) exposed
a defect that the adapter's own tests had not caught, because they asserted volume and validity but
not topology:

* a 10×10×5 mm box tessellated to **24 positions / 12 triangles** — per-face vertices, so every edge
  looked unshared: `open_edges()` returned **48**;
* after welding coincident positions, `open_edges()` still returned **8**: the cap at z = 0 was wound
  **inward** while every other face was outward;
* the signed volume nevertheless read exactly **500.000 mm³**. That is why the defect survived: a face
  lying in the z = 0 plane contributes nothing to the tetrahedral sum, so volume alone could not see
  it.

Triangles wound inconsistently are not a cosmetic issue: STL consumers, slicers and any downstream
kernel receive a non-manifold surface with wrong normals for that face.

## Options considered

| Option | Assessment |
|---|---|
| Reverse the profile before sweeping the base face | **Tried and rejected.** It inverted the whole solid (volume −500) and broke five boolean tests, because truck's boolean path consumes the solid's orientation. A solid-level change to fix a mesh-level symptom is the wrong layer. |
| Require callers to weld and orient | Every consumer (CLI, renderer, exporter, future web client) would need the same non-trivial repair; the first one written would be the only correct one. |
| Patch the vendored `truck-meshalgo` | The patch set is already a maintenance liability; adding geometry logic there deepens the fork without testing at the contract level. |
| **Repair in the adapter's tessellation output** | Chosen: it is the component that promises "a mesh crossing the boundary is valid", it is tested, and solids (and therefore booleans) are untouched. |

## Decision

`mesh_of` now, before returning:

1. **Welds** vertices by position with a 1 nm quantum (four orders of magnitude below truck's own
   1e-4 mm tolerance), remapping triangles and dropping triangles whose vertices collapse (zero area,
   hence no surface).
2. **Makes the winding consistent** by BFS across shared edges: two neighbours must traverse a shared
   edge in opposite directions; if they agree, the neighbour is flipped.
3. **Orients the whole surface outward** by flipping everything if the signed volume is negative.

The result for the box fixture is 8 vertices / 12 triangles / 0 open edges / +500 mm³, verified by an
independent parser reading the CLI's STL output (12 triangles, volume 500.0000, 0 open edges). The
union fixture produces 82 triangles, volume 1120.0037 mm³, 0 open edges.

## Consequences

* `Mesh::is_closed()` and `open_edges()` are now meaningful for adapter output, and the new test
  `tessellation_is_a_closed_outward_manifold` fails if either the welding or the orientation regresses.
* Volume measurements are unchanged in magnitude; the repair removes a latent wrongness rather than
  changing arithmetic.
* Cost is O(triangles) with two ordered maps; the box path remains below the 10 ms resolution of
  `/usr/bin/time` end-to-end including process start.
* The repair cannot invent topology. A surface with genuine holes stays open, and a non-manifold edge
  with more than two triangles is repaired locally but not guaranteed globally; the test suite
  covers closed solids only.
* This is a *tessellation* fix, not a kernel fix. The underlying solid may still carry an orientation
  flag that does not match its geometry; booleans behave as measured, and the adapter continues to
  refuse boolean difference rather than returning a wrong solid.

## Follow-up

`task.corpus.booleans` remains the right next step: a fixture set with analytic volumes and topology
checks would catch the next defect of this class deliberately instead of through a new consumer.
