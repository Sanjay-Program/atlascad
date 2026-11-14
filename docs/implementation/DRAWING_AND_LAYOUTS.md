# AtlasCAD Drawing & Layouts (Paper Space) Engine

## Overview
AtlasCAD separates 3D engineering models from 2D production documentation using a dual Model Space / Paper Space architecture identical to AutoCAD and ISO drawing standards.

---

## 1. Paper Space & Sheet Management

### 1.1 Sheet Formats & Scaling
- Standard Paper Sizes:
  - **ISO A-Series**: A4 ($210 \times 297\text{ mm}$), A3 ($420 \times 297\text{ mm}$), A2 ($594 \times 420\text{ mm}$), A1 ($841 \times 594\text{ mm}$), A0 ($1189 \times 841\text{ mm}$).
  - **ANSI Formats**: ANSI A (Letter), ANSI B (Tabloid), ANSI C, ANSI D, ANSI E.
- Standard Drawing Scales: `1:1`, `1:2`, `1:5`, `1:10`, `2:1`, `5:1`, `10:1`.
- Viewport boundary clipping preventing drawing elements from bleeding outside paper borders.

### 1.2 Title Block Automation
Every layout sheet embeds an automated, standard-compliant title block displaying:
- **Project Name**: Pulled directly from project metadata (`app.project.name`).
- **Revision Identifier**: Auto-incremented revision tag (e.g. `REV 1`, `REV 2`).
- **Scale**: Active sheet ratio and unit indicator (e.g. `1:1 (mm)`).
- **Date & Drafter**: Timestamp and author credentials.

---

## 2. Multi-View Orthographic Projection Engine (`MULTIVIEW` / `MV`)

### 2.1 Standard 4-View Projections
The `MULTIVIEW` command automatically generates a complete 4-view drawing arrangement:
1. **Front Elevation**: Primary silhouette projected onto the $XZ$ plane.
2. **Top Plan**: Orthogonal projection onto the $XY$ plane.
3. **Right Profile**: Orthogonal projection onto the $YZ$ plane.
4. **Isometric 3D Projection**: Scaled trimetric/isometric pictorial view in the upper-right quadrant.

### 2.2 Section Views & Cross-Hatching
- Automatic internal cavity revelation using cutting planes.
- Section cutting lines conforming to ISO 128-40 (thick ends, directional arrows, identification letters `A-A`).
- Cross-hatching automatically applied to solid material slices using ANSI31 pattern ($45^\circ$ continuous lines at uniform pitch).

---

## 3. Vector Output & Export Pipeline

### 3.1 AutoCAD R12 DXF Export
- High-fidelity ASCII DXF generation adhering strictly to Autodesk R12/LT2 specifications for maximum CAM/CNC machine compatibility.
- Layers, colors, and line segments mapped to native DXF tables.

### 3.2 ISO Standard PDF Plotting
- Direct client-side and server-side vector PDF plotting.
- High-resolution linework rendering with exact millimeter lineweight preservation ($0.18\text{ mm}$ to $0.70\text{ mm}$).
