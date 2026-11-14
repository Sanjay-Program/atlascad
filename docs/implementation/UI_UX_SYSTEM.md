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
