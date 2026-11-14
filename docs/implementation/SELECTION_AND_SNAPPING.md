# AtlasCAD Intelligent Selection & Object Snapping (OSNAP) Architecture

## Overview
Precision interaction in engineering CAD demands sub-pixel picking, unambiguous snapping when multiple entities cluster together, and strict standard selection behaviors. AtlasCAD implements AutoCAD-compliant Window and Crossing selection mechanics coupled with an intelligent multi-mode Object Snap (OSNAP) engine.

---

## 1. Selection Mechanics

### 1.1 Single-Entity Picking
- Direct hover and click selection with SVG `[data-entity-id]` hit-testing.
- Visual feedback: Selected entities render with bright accent highlights (`#00e5ff` / `#2196f3`) and high-contrast bounding grips.

### 1.2 AutoCAD Window vs Crossing Box Selection
AtlasCAD implements the industry-standard drag-direction dependent selection box:

| Drag Direction | Mode | Visual Representation | Selection Rule |
| :--- | :--- | :--- | :--- |
| **Left $\to$ Right** | **Window Selection** | Solid blue fill (`rgba(33, 150, 243, 0.22)`) with solid blue border | **Strict Enclosure**: Only entities completely inside the selection rectangle are selected. |
| **Right $\to$ Left** | **Crossing Selection**| Dashed green fill (`rgba(76, 175, 80, 0.22)`) with dashed green border | **Crossing & Enclosure**: Any entity touching, intersecting, or enclosed by the rectangle is selected. |

### 1.3 Mathematical Evaluation Algorithm
Let the selection rectangle in model space be defined by $[X_{\min}, Y_{\min}, X_{\max}, Y_{\max}]$.
For each entity $E$ with bounding box $[e_{\min}^x, e_{\min}^y, e_{\max}^x, e_{\max}^y]$:
- **Window Test**:
  $$\text{Selected} \iff (e_{\min}^x \ge X_{\min}) \land (e_{\max}^x \le X_{\max}) \land (e_{\min}^y \ge Y_{\min}) \land (e_{\max}^y \le Y_{\max})$$
- **Crossing Test**:
  $$\text{Selected} \iff \neg(e_{\max}^x < X_{\min} \lor e_{\min}^x > X_{\max} \lor e_{\max}^y < Y_{\min} \lor e_{\min}^y > Y_{\max})$$

---

## 2. Intelligent Object Snap (OSNAP) Engine

### 2.1 Snap Modes & Distinct Glyphs
Each geometric relationship displays a distinct visual marker and tooltip label:

| Snap Mode | Glyph Shape | Standard Color | Geometric Definition |
| :--- | :---: | :--- | :--- |
| **Endpoint** | Square ($\Box$) | Green (`#4caf50`) | Vertices of lines, polylines, arcs, splines, rectangles |
| **Midpoint** | Triangle ($\triangle$) | Cyan (`#00bcd4`) | Exact midpoint of straight segments or circular arc lengths |
| **Center** | Circle ($\bigcirc$) | Yellow (`#ffeb3b`) | True center coordinates of circles, arcs, and ellipses |
| **Quadrant** | Diamond ($\Diamond$) | Orange (`#ff9800`) | Four cardinal points ($0^\circ, 90^\circ, 180^\circ, 270^\circ$) on circles/arcs/ellipses |
| **Intersection**| Cross ($\times$) | Red (`#f44336`) | Analytical intersection point between two curve segments |
| **Perpendicular**| Right Angle ($\llcorner$) | Blue (`#2196f3`) | Point on target segment forming $90^\circ$ normal to base point |
| **Tangent** | Tangent Circle ($\bigcirc\hspace{-4pt}\underline{\phantom{a}}$) | Pink (`#e91e63`) | Tangent contact point on circle/arc from base point |
| **Nearest** | Hourglass ($\bowtie$) | Purple (`#9c27b0`) | Nearest point on segment under cursor aperture |
| **Node** | Point Cross ($\oplus$) | Green (`#4caf50`) | Exact point primitive coordinates |
| **Parallel** | Parallel Slash ($/\!/$) | Amber (`#ff9800`) | Line alignment parallel to a reference segment |

### 2.2 Aperture & Snap Priority
When multiple snaps fall within the cursor search aperture (default 12px screen space):
1. Endpoint
2. Midpoint
3. Center
4. Intersection
5. Quadrant
6. Perpendicular
7. Tangent
8. Nearest

### 2.3 Temporary Snap Overrides
Users can enforce a specific snap mode during any placement command via the snap bar pills or shortcut keys (e.g. `END`, `MID`, `CEN`, `INT`, `PER`, `TAN`). The override is consumed immediately on the next point click.

### 2.4 Polar Tracking
- Real-time polar guide ray extending to the drawing sheet boundary.
- Snaps to standard increments ($15^\circ, 30^\circ, 45^\circ, 90^\circ$).
- Dynamic HUD displays tracking angle and current length relative to base point.
