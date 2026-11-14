# AtlasCAD Advanced 2D Drafting Suite Implementation

## Overview
The AtlasCAD 2D drafting suite provides a professional, precision drafting and engineering annotation experience adhering to ASME Y14.5, ISO 1101, and DIN engineering standards. The system runs both interactively in WebGL/SVG paper space and through high-performance Rust analytical kernels.

---

## 1. Geometric Primitive Engine

### 1.1 Supported Primitives
AtlasCAD supports the complete suite of professional 2D drafting primitives:
- **LINE**: Two-point segments with millimeter precision, Cartesian (`x,y`), polar (`@dist<angle`), and relative (`@dx,dy`) coordinate inputs.
- **POLYLINE (PLINE)**: Multi-vertex continuous chains supporting open profiles and closed loops (`C` / Close option) with vertex chamfering/filleting.
- **CIRCLE**: Centre-radius, 2-point, and 3-point placement modes with diameter/radius dimensioning.
- **ARC**: 3-point, Start-Center-End, and Center-Start-Angle arcs with true arc-length computation.
- **RECTANGLE (REC)**: Diagonal corner and dimension-specified parametric rectangles with automatic corner filleting.
- **ELLIPSE (EL)**: Major and minor semi-axis parametric ellipses and elliptical arcs.
- **SPLINE (SPL)**: Uniform Catmull-Rom cubic splines with interactive control-point manipulation and curvature evaluation.
- **POLYGON (POL)**: Regular $n$-sided polygons (inscribed or circumscribed) around a center circle.
- **POINT (PO)**: 2D node references for measurement, dividing, and geometric constraints.
- **HATCH (H / BH)**: Boundary hatching supporting ANSI31 (standard cast iron / steel 45° crosshatch), ISO patterns, and NET grid hatching with scalable pattern pitch.

### 1.2 Mathematical Representation & Precision
All coordinate calculations, intersection tests, and bounding box evaluations are carried out in 64-bit double-precision floating point (`f64`). Points preserve sub-nanometer analytical accuracy before SVG and WebGL viewport rasterization.

---

## 2. Advanced Modification Tools

AtlasCAD includes standard CAD modification commands operating on single entities, selection sets, or crossing boundaries:

| Command | Alias | Description | Geometric Algorithm |
| :--- | :--- | :--- | :--- |
| **MOVE** | `M` | Translate entities by $(\Delta x, \Delta y)$ | Rigid vector displacement applied to all vertices/centers |
| **COPY** | `CO`, `CP` | Duplicate entities shifted by displacement vector | Deep-clone entities with updated unique IDs and displaced geometry |
| **ROTATE** | `RO` | Rotate entities around base point by angle $\theta$ | $2\times 2$ coordinate rotation matrix around arbitrary pivot |
| **SCALE** | `SC` | Scale entities about base point by scale factor $k$ | Homothetic dilation relative to base point |
| **MIRROR** | `MI` | Mirror entity across arbitrary reflection line | Affine reflection matrix across 2-point axis |
| **OFFSET** | `O` | Parallel or concentric offset by distance $d$ | Normal displacement along segment perpendiculars and radial offset |
| **TRIM** | `TR` | Trim segment between boundary cutting edges | Exact analytical line-line and line-arc intersection calculation |
| **EXTEND** | `EX` | Extend segment to boundary edge | Ray-segment intersection projection to nearest boundary |
| **FILLET** | `F` | Create tangent circular arc between intersecting lines | Inscribed circle tangent to both line directions at radius $R$ |
| **CHAMFER** | `CHA` | Bevel corner between intersecting lines | Distance cutback along both legs connected by straight segment |
| **EXPLODE** | `X` | Decompose rectangles/polylines into individual lines | Vertex-to-vertex edge extraction into first-class `Line` primitives |
| **JOIN** | `J` | Combine collinear/contiguous lines into single polyline | Endpoint proximity graph traversal with $10^{-6}$ tolerance |
| **ARRAY** | `AR` | Rectangular and Polar circular geometric patterns | Matrix translation grid or radial angular distribution |
| **OVERKILL** | `OV` | Purge duplicates and merge overlapping collinear segments | Direction alignment check + 1D interval merging |

---

## 3. Reusable Block & Component System

### 3.1 Block Definition & Instantiation
- Standard ISO/DIN mechanical fasteners pre-loaded in the component library:
  - **ISO 4014 Hex Head Bolts** (M6, M8, M10)
  - **ISO 4032 Hex Nuts** (M6, M8)
  - **ISO 7089 Plain Washers** (Form A, M6, M8)
  - **Datum Feature Symbols** (ASME Y14.5 / ISO 1101 Datum [A], [B])
  - **Surface Finish / Roughness Symbols** (ISO 1302 Ra 3.2, Ra 1.6)
  - **Center Marks & Crosses** (DIN / ISO)
  - **IEC Electrical Schematics** (Resistor, Earth Ground, Motor)
- One-click insertion into the active drawing sheet at cursor coordinates or specified model origin.
- Reusable block definitions stored persistently in document metadata.

---

## 4. AI-Assisted CAD Drafting Pipeline

AtlasCAD features an integrated Natural Language CAD Drafting Assistant:
1. **Intent Parsing**: Natural language parsing engine parses engineering phrases such as:
   - *"Create a 80x50 mm plate with 4x M6 holes at 10mm corner inset"*
   - *"Create a 100x60 rectangle centered at 0,0"*
   - *"Add 12mm center hole"*
   - *"Fillet 5mm all corners"*
   - *"Generate 4-view orthographic drawing"*
2. **Operation Plan Preview**: Displays parsed operation name, target layer, parameters, and affected entities before touching document state.
3. **Verified Execution**: Executes atomic, validated CAD transactions appending real parametric entities into the active drawing model.
4. **Transaction Journal**: Real-time session history displaying executed operations with timestamps.
