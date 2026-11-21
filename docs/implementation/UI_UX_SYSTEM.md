# AtlasCAD UI/UX System & Design System Specification

## Overview
AtlasCAD delivers a unified, high-performance professional engineering workstation experience. The interface synthesizes the command ergonomics of AutoCAD, the parametric feature modeling intelligence of Autodesk Inventor and Fusion 360, and the clean multi-sheet layout workflows of modern production CAD environments.

---

## 1. Top Ribbon & Workflow Tabs

The ribbon organizes all drafting, modeling, and simulation tools into standard engineering categories:

1. **Home**: Quick-access geometry creation (Line, Circle, Rectangle, Arc, Polyline, Spline), modifications (Trim, Extend, Offset, Fillet, Chamfer), Layer Manager (`#rbLayersBtn`), Blocks (`#rbBlockLibBtn`), Named Views (`#rbNamedViewsBtn`), and AI Copilot (`#rbAiDraftBtn`).
2. **Solid**: 3D parametric feature operations (Extrude Join, Cut/Subtract, Revolve, Fillet 3D, Chamfer 3D, Shell, Hole, Pattern).
3. **Draw**: Detailed 2D geometric entity placement tools, construction lines, and hatching.
4. **Modify**: Advanced transform tools (Move, Copy, Rotate, Scale, Mirror, Array, Explode, Join, Align).
5. **Annotate**: Linear, Aligned, Angular, Radial, Diameter dimensions, Multileaders, Center Marks, GD&T feature control frames, Surface Texture symbols, and Title Block management.
6. **Simulate**: Engineering FEA (Static stress, von Mises deflection), Modal vibration frequency analysis, Steady-state thermal conduction, and 3-axis CNC CAM toolpath generation.
7. **Automate & AI**: AI CAD drafting copilot, verified typed CAD transactions, custom parametric macros, and batch automation.
8. **Output**: Multi-sheet vector PDF plotting, AutoCAD R12 DXF export, binary STL generation, and revision control management.

---

## 2. Floating Viewport Navigation & ViewCube

### 2.1 3D Floating ViewCube
- Persistent, interactive orientational cube in the upper-right corner of the WebGL viewport.
- 6 Cardinal Faces: `TOP`, `BOTTOM`, `FRONT`, `BACK`, `LEFT`, `RIGHT`.
- Isometric Corner Hotspots: Clickable corners instantly orienting the camera to SE, SW, NE, and NW isometric projections.
- Compass Ring: North, South, East, West bearing alignment with smooth camera interpolation.

### 2.2 AutoCAD Navigation Bar
Floating vertical toolbar containing precision viewport navigation controls:
- **Home View ($\mathtt{\unicode{x2302}}$)**: Restores default SE Isometric framing.
- **Zoom Extents ($\mathtt{\unicode{x26F6}}$)**: Frames entire model/drawing geometry (`FIT` / `ZE`).
- **Pan ($\mathtt{\unicode{x270B}}$)**: Middle-mouse-drag or Shift-drag pan mode.
- **Orbit ($\mathtt{\unicode{x21BB}}$)**: Spherical yaw/pitch camera orbit around model center.
- **Visual Styles ($\mathtt{\unicode{x1F3A8}}$)**: Cycles through the 12 professional 3D visual styles.
- **Projection Toggle ($\mathtt{\unicode{x29C9}}$)**: Switches between Perspective (True FOV) and Orthographic (Parallel Projection).
- **Inspection HUD ($\mathtt{\unicode{x1F4CF}}$)**: Toggles face-area, edge-length, and center-to-center measurement HUD.
- **Clean Screen ($\mathtt{\unicode{x2922}}$)**: Toggles distraction-free full-canvas focus mode (`Ctrl+0` / `F11`).

---

## 3. Dynamic Multi-Sheet Layout System (Paper Space)

- Tabbed workspace switcher supporting Model Space (`tabModel`) and an arbitrary number of Paper Space drawing layouts (`tabLayout1`, `tabLayout2`, `+ New Layout`).
- Full ISO title blocks displaying Project Name, Revision, Scale, and Unit standards.
- Independent pan and zoom transforms per layout sheet.
- Synchronized rendering between model 3D changes and 2D drawing sheet views.

---

## 4. Theme System & Visual Accessibility
- Dark Mode (Studio Slate `#090d13`) and Light Mode (Paper Engineering `#f4f6f8`).
- Contrast ratios compliant with WCAG 2.1 AA for technical drawing annotations.
- Hardware-accelerated CSS transforms and zero UI layout shifts during viewport re-renders.

---

## 5. Unified Direct-Manipulation (Drag & Drop) System

Every drag surface in the studio is driven by one pointer-event layer, so the same gestures work with a mouse, pen or touch input and no action is drag-only.

### 5.1 Viewport and sheet navigation
| Gesture | Result |
| :--- | :--- |
| Left-drag in the 3D viewport | Orbit the camera |
| Shift-drag, middle-drag, right-drag or Space+drag | Pan |
| Scroll / pinch | Zoom |
| Drag with Alt (or middle button) on a drawing sheet | Pan the paper-space sheet |
| Left-drag on empty sheet with the Select tool | Window selection (left→right) or crossing selection (right→left) |
| Drag a file onto the workspace | Routed to the CAD import drop surface (STEP, STL, 3MF, OBJ, JSON, DXF) |

### 5.2 Drawing geometry
| Gesture | Result |
| :--- | :--- |
| Drag a selected entity | Live translate preview, committed as one `/api/draft/transform` translate with undo |
| Shift while dragging | Orthogonal lock to the dominant axis |
| Ctrl/Cmd while dragging | Copy instead of move |
| Ctrl/Shift+click an entity | Add or remove it from the multi-selection; dragging moves the whole set |
| Esc during a drag | Restore the original transforms and issue no request |
| Drag a component card from the Block Library | Sheet opens, a ghost follows the pointer, and releasing places the component at the drop coordinates |

Locked layers refuse entity drags with an explicit message; a drag never silently rewrites locked geometry. Every committed drag is a single server transaction, so a single Ctrl+Z restores it.

### 5.3 Reorderable interface
| Surface | Behaviour | Persistence |
| :--- | :--- | :--- |
| Layout tabs (Model / Layout 1…) | Horizontal drag reorder with a live insertion preview | `localStorage` key `atlas-layout-order` |
| Engineering workbench tabs (3D, Drafting, CAE, CAM, BIM, ECAD, Create, RealTime, Media…) | Horizontal drag reorder | `localStorage` key `atlas-workbench-order` |
| Project-browser sections (Project, Create, Parameters, Model tree…) | Drag by the section heading | `localStorage` key `atlas-left-panel-order` |

`Alt+Arrow` moves the focused tab or section with the keyboard, and every move is announced through a polite live region (`#atlasDragAnnouncer`).

### 5.4 Numeric scrubbing
Dragging the label of any `number` input scrubs the value continuously across every department panel: model parameters, sketch placement, CAE loads, CAM feeds and speeds, BIM storeys, ECAD values and creative controls. Shift multiplies the step by 10, Alt divides it by 10, Esc restores the starting value, and the committed value fires the normal `change` event so existing validation and rebuild paths run unchanged.

### 5.5 Feedback and cancellation
A floating drag HUD (`#dragHud`) reports the operation, the live delta/coordinates and modifier state. Sortable items show an accent outline while dragged, the sheet shows a dashed drop zone during component placement, and Esc cancels entity drags, component placement, scrubbing and tab reordering without mutating the document.
