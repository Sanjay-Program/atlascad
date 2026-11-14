# AtlasCAD — Digital Manufacturing Architecture
## Subtractive Multi-Axis CNC Machining & Additive 3D Slicing Platform

**Document Version:** 2.0.0  
**Classification:** Digital Manufacturing Architecture & Kinematics  
**Safety Mandate:** Zero unvalidated toolpaths, collision-checked G-code, exact volumetric kinematics.  
**Author:** AtlasCAD Manufacturing & Machine Control Group  

---

## 1. Overview of the Manufacturing System

AtlasCAD features a dual manufacturing platform connecting CAD solid geometry directly to physical production:
1. **Subtractive CAM (`cad-cam`):** 2.5D, 3-axis, 4-axis rotary, and 5-axis simultaneous milling with modal $(I, J, K)$ orientation vectors, heightmap stock carving simulation, and holder collision detection.
2. **Additive Slicing (`atlas-additive`):** FDM/FFF cross-sectional polygon slicing, Dolenc & Mäkelä adaptive layer height, 5 infill algorithms (including TPMS Gyroid), volumetric extrusion kinematics, and multi-dialect G-code emission.

```
                         [ Parametric CAD Model ]
                                    │
           ┌────────────────────────┴────────────────────────┐
           ▼                                                 ▼
[ Subtractive CAM Pipeline ]                      [ Additive Slicing Pipeline ]
- Stock & WCS Setup                               - Build Plate Orientation
- 3/4/5-Axis Toolpath Planning                    - Adaptive Layer Height Calculation
- Tool Orientation (I, J, K)                      - Boundary Contour Insetting
- Heightmap Carve Simulation                      - Mathematical Infill (Gyroid, etc.)
- Holder Collision Verification                   - Volumetric E-axis Kinematics
- Modal CNC G-Code (ISO/LinuxCNC)                 - Multi-Dialect 3D G-Code (Marlin)
- WebGL 3D Move Overlay                           - WebGL Dynamic Layer Scrubber
```

---

## 2. Subtractive CAM & Multi-Axis CNC Engine

### 2.1 Multi-Axis Kinematics & Tool Orientation
- **Tool Center Point (TCP) Parametrization:** Every point on a toolpath is defined by position $\mathbf{p} = [X, Y, Z]$ and an optional unit orientation vector $\mathbf{u} = [I, J, K]$.
- **Rotary 4-Axis Continuous Milling:** Generates cylindrical continuous spiral and indexed toolpaths around the rotation axis ($X$ or $A$), maintaining the tool perpendicular to the rotary centerline.
- **5-Axis Simultaneous Contouring:** Emits surface normal tool vectors $[I, J, K]$ computed from the analytical B-Rep face geometry, eliminating tool heel gouging on freeform sculptured surfaces.

### 2.2 Heightmap Stock Model & Holder Collision Checking
- **2.5D Heightmap Simulation (`MaterialRemoval`):** Discretizes stock into an analytical grid. Every cutting move lowers affected cells to the tool cutting depth, computing exact volumetric material removal in $\text{mm}^3$ and removal fraction percentage.
- **Holder Clearance Verification (`check_holder_clearance`):** Models tool holder geometry and verifies clearance against remaining uncarved material along every move interpolation step. Emits `HolderCollision` errors if holder overlaps standing stock.

### 2.3 WebGL Viewport CAM Overlay
- Rapid traverse moves: High-visibility yellow line vectors.
- Plunge feed moves: Orange-red vertical penetration vectors.
- Cutting feed moves: Cyan high-feed contouring passes.

---

## 3. Additive Manufacturing & 3D Slicing Engine

### 3.1 Adaptive Layer Height Formulation
Under Dolenc & Mäkelä stair-stepping theory, the cusp height (stair-step error) $c$ on an inclined surface with slope angle $\theta$ relative to the horizontal build plate is:
$$c = h \cos\theta$$
Enforcing a maximum allowable cusp threshold $c_{allowable}$ yields the optimal adaptive layer height:
$$h_{adaptive} = \text{clamp}\left( \frac{c_{allowable}}{\max(\cos\theta, 10^{-4})}, h_{min}, h_{max} \right)$$
- Vertical walls ($\theta = \pi/2$): $\cos\theta \to 0 \implies h = h_{max}$ (rapid printing).
- Shallow slopes ($\theta \to 0$): $\cos\theta \to 1 \implies h = h_{min}$ (maximum surface finish).

### 3.2 Advanced Mathematical Infill Patterns
1. **Rectilinear & Grid:** Alternating raster angles ($0^\circ, 90^\circ, \pm 45^\circ$).
2. **Honeycomb / Hexagonal:** Cellular hexagonal lattice providing optimal strength-to-weight ratio.
3. **Gyroid TPMS:** Triply Periodic Minimal Surface approximation:
   $$\sin(x)\cos(y) + \sin(y)\cos(z) + \sin(z)\cos(x) = 0$$
   Provides isotropic mechanical strength without enclosed liquid traps.
4. **Concentric:** Offset perimeter contour rings.

### 3.3 Volumetric Extrusion Kinematics
Calculates exact linear filament feed length $E$ from nozzle line width $w$, layer thickness $h$, segment length $L$, and filament diameter $d_{filament}$:
$$E = \frac{w \cdot h \cdot L}{\pi (d_{filament} / 2)^2}$$
Manages retract distance, unretract, travel moves, and multi-dialect G-code formatting (`M104`, `M109`, `M140`, `M190`, `M106`, `G92 E0`, `G1`).
