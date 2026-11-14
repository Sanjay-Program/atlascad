# AtlasCAD Command Reference Manual

## Overview
AtlasCAD features a professional dual-mode command interpreter:
1. **Interactive CLI Prompt**: Bottom command bar (`#commandPromptInput`) with real-time fuzzy autocomplete suggestions and 2-line command history.
2. **Keyboard Accelerators**: Single-letter and multi-letter aliases conforming to AutoCAD 2024 standards.

---

## Command Alphabetical Index

| Command | Aliases | Category | Description | Shortcut / Key |
| :--- | :--- | :--- | :--- | :--- |
| **AI / ASSIST** | `AI`, `ASSIST`, `COPILOT` | Automate | Natural language CAD assistant with verified plan execution | Ribbon Home / AI |
| **ALIGNED DIM** | `DIM`, `DIMALIGNED` | Annotate | Linear dimension between two arbitrary points | `DIM` + 2 clicks |
| **ARC** | `A`, `ARC` | Draw | 3-point circular arc placement | Ribbon Draw |
| **ARRAY** | `AR`, `ARRAY` | Modify | Rectangular or polar circular geometric pattern | `AR` |
| **BLOCK** | `B`, `BLOCK` | Draw | Define reusable component block from selected entities | `B` |
| **BLOCKLIB** | `LIB`, `BLOCKLIB` | Draw | Component & Block Library palette (ISO fasteners, symbols) | Ribbon Home / Blocks |
| **BOX** | `BOX` | Solid | Parametric 3D solid box primitive | `BOX` |
| **CAM 2D** | `CAM2D` | Simulate | Generate 2D profile CNC toolpaths & G-code | `CAM2D` |
| **CAM 3D** | `CAM`, `GCODE` | Simulate | Generate 3-axis CNC surface machining toolpaths | `CAM` |
| **CHAMFER** | `CHA`, `CHAMFER` | Modify | Bevel corner between two intersecting lines | `CHA` |
| **CIRCLE** | `C`, `CIRCLE` | Draw | Draw circle with center and radius / diameter | `C` |
| **CLEANSCREEN** | `SCR`, `CLEAN`, `FULL` | View | Toggle distraction-free full-canvas focus mode | `Ctrl+0` / `F11` |
| **COPY** | `CO`, `CP`, `COPY` | Modify | Duplicate entities displaced by vector | `CO` |
| **CYLINDER** | `CYL`, `CYLINDER` | Solid | Parametric 3D cylinder primitive | `CYL` |
| **DIMDIAMETER** | `DDI` | Annotate | Create diameter dimension symbol ($\varnothing$) for circle/arc | `DDI` |
| **DIMRADIUS** | `DRA` | Annotate | Create radial dimension symbol ($R$) for circle/arc | `DRA` |
| **DIST** | `DI`, `DIST` | Inspect | Measure distance, coordinate delta, and angle | `DI` |
| **DIVIDE** | `DIV`, `DIVIDE` | Draw | Divide entity into equal intervals by placing points | `DIV` |
| **DXFOUT** | `DXF`, `DXFOUT` | Output | Export active plan drawing to AutoCAD R12 DXF | Ribbon Output |
| **DYNAMIC INPUT**| `DYN`, `DYNMODE` | System | Toggle on-canvas floating dynamic coordinate HUD | `F12` |
| **ELLIPSE** | `EL`, `ELLIPSE` | Draw | Draw ellipse with semi-major and semi-minor radii | `EL` |
| **ERASE** | `E`, `ERASE` | Modify | Delete selected drawing entity | `Delete` |
| **EXPLODE** | `X`, `EXPLODE` | Modify | Decompose rectangle/polyline into separate lines | `X` |
| **EXTEND** | `EX`, `EXTEND` | Modify | Extend line segment to boundary edge | `EX` |
| **EXTRUDE** | `EXT`, `EXTRUDE` | Solid | Interactive 3D parametric extrude HUD | Ribbon Solid |
| **FEA** | `FEA`, `STATIC` | Simulate | Linear static stress & von Mises deflection FEA | Ribbon Simulate |
| **FILLET** | `F`, `FILLET` | Modify | Tangent circular fillet arc between two segments | `F` |
| **GRID** | `G`, `GRID` | View | Toggle 3D and 2D ground alignment grid | `F7` |
| **HATCH** | `H`, `BH`, `HATCH`| Draw | Boundary crosshatching (ANSI31, NET, SOLID) | `H` |
| **HELP** | `HELP`, `?` | System | Keyboard shortcuts & engineering command guide | `F1` |
| **HIDDEN** | `HIDDEN` | View | Switch visual style to Hidden Line Removed | Nav Bar |
| **INSERT** | `I`, `INS`, `INSERT`| Draw | Insert block instance into drawing | `I` |
| **INTERFERE** | `INF`, `CLASH` | Inspect | Multi-body collision and clearance interference check | `INF` |
| **JOIN** | `J`, `JOIN` | Modify | Combine connected lines/segments into single polyline | `J` |
| **LAYER** | `LA`, `LAYER` | Format | Open Layer Properties Manager palette | Ribbon Home / Layers |
| **LINE** | `L`, `LINE` | Draw | Draw continuous 2-point line segments | `L` |
| **LINETYPE** | `LTYPE`, `LINETYPE`| Format | Set layer linetype (CONTINUOUS, HIDDEN, CENTER) | `LTYPE` |
| **MASSPROP** | `MP`, `MASSPROP` | Inspect | Physical mass properties, volume, surface area, moments | `MP` |
| **MATCHPROP** | `MA`, `MATCHPROP`| Format | Match layer, color, and linetype across entities | `MA` |
| **MEASURE** | `ME`, `MEASURE` | Draw | Place point markers at uniform length intervals | `ME` |
| **MEASUREGEOM** | `MEASURE3D`, `INSPECT`| Inspect | 3D Face & Distance Measurement HUD | `M` / `I` |
| **MIRROR** | `MI`, `MIRROR` | Modify | Reflect entities across a 2-point mirror axis | `MI` |
| **MODAL** | `MODAL`, `VIBRATION`| Simulate | Dynamic natural frequency modal vibration FEA | Ribbon Simulate |
| **MOVE** | `M`, `MOVE` | Modify | Translate entities by vector $(\Delta x, \Delta y)$ | `M` |
| **MULTIVIEW** | `MV`, `ORTHO` | Output | Generate 4-view orthographic drawing with cross-sections | `MV` |
| **OFFSET** | `O`, `OFFSET` | Modify | Parallel or concentric offset by distance $d$ | `O` |
| **ORTHOPROJ** | `PROJ` | View | Toggle Camera Orthographic / Perspective projection | Nav Bar / `O` |
| **OSNAP** | `OSNAP` | Precision | Toggle Object Snap precision engine | `F3` |
| **OVERKILL** | `OV`, `OVERKILL` | Modify | Purge duplicate entities and merge collinear segments | `OV` |
| **PAN** | `P`, `PAN` | View | Pan viewport / drawing sheet | MMB Drag / Shift+Drag |
| **PLINE** | `PL`, `PLINE` | Draw | Draw continuous multi-segment polyline | `PL` |
| **PLOT** | `PDF`, `PLOT` | Output | Plot titled ISO engineering drawing sheet to PDF | Ribbon Output |
| **POINT** | `PO`, `POINT` | Draw | Place 2D point entity | `PO` |
| **POLAR** | `POLAR` | Precision | Toggle Polar Angle tracking (15°, 30°, 45°, 90°) | `F10` |
| **POLYGON** | `POL`, `POLYGON` | Draw | Draw regular $n$-sided polygon | `POL` |
| **PURGE** | `PU`, `PURGE` | System | Clean unused layers and unreferenced block definitions | `PU` |
| **REDO** | `REDO` | System | Redo last reverted transaction | `Ctrl+Y` / `Ctrl+Shift+Z` |
| **REGEN** | `RE`, `REGEN` | View | Regenerate 2D vector cache and 3D WebGL buffers | `RE` |
| **REVOLVE** | `REV`, `REVOLVE` | Solid | 3D revolve sketch profile around axis | Ribbon Solid |
| **ROTATE** | `RO`, `ROTATE` | Modify | Rotate entities around base point | `RO` |
| **SAVE** | `QSAVE`, `SAVE` | File | Commit changes to persistent document | `Ctrl+S` |
| **SCALE** | `SC`, `SCALE` | Modify | Scale entities by uniform scale factor | `SC` |
| **SECTION3D** | `SECTION3D` | View | Switch visual style to Sectioned 3D View | Nav Bar |
| **SPLINE** | `SPL`, `SPLINE` | Draw | Draw Catmull-Rom cubic spline curve | `SPL` |
| **SUBTRACT** | `CUT`, `SUBTRACT`| Solid | 3D Cut / subtract parametric feature from body | Ribbon Solid |
| **TECH** | `TECH` | View | Switch visual style to Technical Illustration (Gooch) | Nav Bar |
| **TEXT** | `T`, `TEXT`, `MT` | Annotate | Place single or multiline annotation text | `T` |
| **TILEMODE** | `MODEL`, `LAYOUT`| Workspace | Switch between Model Space (3D) and Paper Space (2D) | Layout Tabs |
| **TRIM** | `TR`, `TRIM` | Modify | Trim segment between boundary cutting edges | `TR` |
| **UNDO** | `U`, `UNDO` | System | Undo last operation | `Ctrl+Z` |
| **UNION** | `UNI`, `UNION` | Solid | 3D Union / join parametric feature to body | Ribbon Solid |
| **VIEW** | `V`, `VIEW` | View | Camera Bookmarks & Named Views palette | Ribbon Home / Views |
| **VISUALSTYLE** | `STYLE`, `VSTYLE` | View | Cycle between 12 professional 3D visual styles | Nav Bar / Ribbon |
| **ZOOMEXTENTS** | `ZE`, `FIT` | View | Zoom viewport and drawing sheet to fit extents | `F` |
