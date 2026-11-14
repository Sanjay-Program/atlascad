# AtlasCAD — Universal Engineering Data Model Specification
## Comprehensive Representation of Geometry, Parameters, Physics, Toolpaths & Lineage

**Document Version:** 2.0.0  
**Classification:** Universal Engineering Foundation Architecture  
**Author:** AtlasCAD Core Architecture & Data Modeling Group  

---

## 1. System Overview

The **Universal Engineering Model (UEM)** is the single source of truth across all 8 integrated workbenches in AtlasCAD. It ensures that every mechanical part, architectural wall, fluid simulation domain, 5-axis CNC toolpath, and 3D printing slicing layer shares a common topological and parametric foundation.

```
                         [ AtlasCAD Project ]
                                  │
    ┌─────────────────────────────┼─────────────────────────────┐
    ▼                             ▼                             ▼
[ B-Rep Solids & Sketches ]  [ Physical Properties ]   [ Derived Artifacts ]
- Truck Topological Shells   - Materials Database      - FEA / Modal Results
- Analytical NURBS Curves    - SI / Imperial Units     - 3/4/5-Axis CAM Paths
- CSG Parametric Timeline    - Geometric Constraints   - FDM / SLA Slice Layers
- Boolean History Tree       - Coordinate Systems      - 2D Technical Drawings
                                                       - OpenBIM IFC4 BoQ
                                                       - ECAD SPICE Netlists
```

---

## 2. Core Entities & Schema Architecture

### 2.1 Topological B-Rep Solid Representation
- **Analytical Precision:** Exact mathematical representation of vertices, edges, wires, faces, and shells via `cad-kernel-truck`.
- **Exact NURBS Evaluation:** Direct evaluation of surface normals, tangents, and Gaussian curvature without forced polygonal approximation.
- **Analytic Mass Properties:** Exact volume calculation via divergence theorem:
  $$V = \frac{1}{6} \sum_{k} \left( v_0^{(k)} \cdot (v_1^{(k)} \times v_2^{(k)}) \right)$$
  Derivation of center of mass $\vec{r}_{cm}$, principal moments of inertia $I_{xx}, I_{yy}, I_{zz}$, and total component mass $m = \rho V$.

### 2.2 Parametric Timeline & Feature Tree
- **Directed Acyclic Graph (DAG):** Ordered sequence of parametric features (`Extrude`, `Cut`, `Revolve`, `Join`, `Fillet`, `Hole`).
- **Dynamic Rollback:** Allows rolling the active modeling state backwards to inspect or insert upstream features without corrupting downstream geometry.
- **Non-Destructive Suppression:** Individual features can be suppressed or unsuppressed dynamically with instantaneous topological recalculation.

### 2.3 Universal Material Registry
- **Mechanical:** Young's Modulus ($E$), Poisson's Ratio ($\nu$), Yield Strength ($\sigma_y$), Ultimate Tensile Strength ($\sigma_u$), Mass Density ($\rho$).
- **Thermal:** Thermal Conductivity ($k$), Specific Heat ($c_p$), Coefficient of Thermal Expansion ($\alpha$).
- **Fluid & Electrical:** Dynamic Viscosity ($\mu$), Electrical Conductivity ($\sigma_e$), Relative Permittivity ($\epsilon_r$).

---

## 3. Dependency Tracking & Cascade Invalidation

The Universal Engineering Model enforces strict provenance and lineage tracking:

```
[ Parametric Dimension Edit ]
              │
              ▼
    (Dirty Flag Cascade)
              │
    ┌─────────┼─────────┬─────────┬─────────┐
    ▼         ▼         ▼         ▼         ▼
[ B-Rep ]  [ FEA ]   [ CAM ]   [ Slices ] [ Drawing ]
(Rebuild)  (Dirty)   (Dirty)   (Dirty)    (Dirty)
```

When an upstream CAD dimension changes:
1. `atlas-dependency-graph` recursively traverses the dependency tree.
2. Downstream simulation results, CAM paths, additive slice layers, and 2D drawing projections are flagged as **Stale**.
3. Recomputation is scheduled on-demand or queued via `atlas-job-queue`.
4. Stale manufacturing or simulation results are never presented as valid or release-ready.

---

## 4. Derived Artifact Registry & Release Policies

Each derived engineering output is registered with:
- **`ArtifactId`:** Stable, globally unique 64-bit identifier.
- **`source_revision`:** Cryptographic hash and revision integer of the input CAD model.
- **`release_policy`:** Strict rules governing release readiness:
  - *Simulation Studies, CAM Toolpaths, 3D Slices:* **Recompute-Only**. Cannot be released if the upstream CAD model has mutated.
  - *2D Technical Drawings & Documentation:* **Acceptable After Formal Review**. May be released if the Chief Engineer acknowledges the revision changes.
