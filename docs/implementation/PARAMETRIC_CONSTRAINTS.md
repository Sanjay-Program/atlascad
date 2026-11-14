# AtlasCAD Parametric Sketch & Geometric Constraints Engine

## Overview
AtlasCAD features a 2D geometric and dimensional constraint solver that governs sketch profiles before 3D feature extrusion or revolution.

---

## 1. Supported Constraints

### 1.1 Geometric Constraints
- **Coincident**: Points locked to identical spatial coordinates $(x_1 = x_2, y_1 = y_2)$.
- **Horizontal**: Line constrained parallel to the $X$-axis $(y_2 - y_1 = 0)$.
- **Vertical**: Line constrained parallel to the $Y$-axis $(x_2 - x_1 = 0)$.
- **Parallel**: Two lines sharing identical slopes $(m_1 = m_2)$.
- **Perpendicular**: Two lines forming a $90^\circ$ angle $(m_1 \cdot m_2 = -1)$.
- **Tangent**: Line or arc touching a circle/arc at exactly one point without crossing.
- **Concentric**: Circles/arcs sharing identical center points $(c_{1x} = c_{2x}, c_{1y} = c_{2y})$.
- **Equal**: Segments constrained to identical lengths or circles to identical radii ($L_1 = L_2$ or $R_1 = R_2$).
- **Collinear**: Two segments lying on the same infinite line.
- **Symmetric**: Two entities positioned symmetrically across a reference mirror line.
- **Midpoint**: Point constrained to bisect a segment.
- **Fixed**: Entity coordinates anchored rigidly in sketch space.

### 1.2 Dimensional Constraints
- **Horizontal Distance**: Constrained $\Delta x = d$.
- **Vertical Distance**: Constrained $\Delta y = d$.
- **Linear Distance**: Point-to-point Euclidean distance $\sqrt{\Delta x^2 + \Delta y^2} = d$.
- **Radius ($R$)**: Arc or circle radius constraint.
- **Diameter ($\varnothing$)**: Circle diameter constraint.
- **Angle ($\theta$)**: Angular opening between two segments.

---

## 2. Degrees of Freedom (DOF) & Solver Status

The solver evaluates the system Jacobian and reports:
- **Underconstrained (Blue)**: Unfixed degrees of freedom remain. Geometry can be dragged interactively.
- **Fully Constrained (Black / Green)**: 0 degrees of freedom remain. Every vertex is deterministically defined.
- **Overconstrained (Red)**: Redundant or conflicting constraints detected. Diagnostic HUD highlights offending constraints for user removal.

---

## 3. Profile Validation & Loop Detection
Before performing an Extrude or Revolve operation:
- **Closed-Loop Verification**: Validates that all edges form a continuous, non-self-intersecting planar polygon.
- **Island / Void Detection**: Detects nested closed loops and treats them as holes/voids during solid extrusion.
- **Self-Intersection Rejection**: Flags overlapping or crossed segments to prevent manifold geometry corruptions.
