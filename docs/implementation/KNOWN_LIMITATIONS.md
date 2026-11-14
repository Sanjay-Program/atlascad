# AtlasCAD Known Limitations & Architectural Roadmap

## Overview
AtlasCAD is engineered for high-precision parametric mechanical modeling and 2D engineering documentation. This document documents the current engineering boundaries, theoretical limitations, and prioritized development roadmap.

---

## 1. Geometric Modeling & Kernel Boundaries

### 1.1 Non-Uniform Rational B-Splines (NURBS) Surfaces
- **Current State**: Exact analytical cylinders, planar faces, fillets, chamfers, and Catmull-Rom splines are fully supported.
- **Limitation**: Freeform Class-A sculpted aerodynamic surfaces (e.g. automotive body styling) requiring multi-patch bi-cubic NURBS stitching are currently tessellated upon import.
- **Roadmap**: OpenNURBS / STEP AP242 boundary representation tessellation engine in Milestone 2.

### 1.2 Multi-Body Complex Sweeps & Lofts
- **Current State**: Linear extrusions, boolean subtractions/unions, cylindrical revolves, and stepped boss operations are fully supported.
- **Limitation**: Guide-curve lofts across non-parallel planar cross-sections with twist angles are currently planned.

---

## 2. Large Assembly Handling

### 2.1 Component Hierarchy
- **Current State**: Multi-feature single-body and multi-body parts with parametric timeline history, interference checking, and BOM generation are fully operational.
- **Limitation**: Sub-assembly nesting beyond 5 levels of deeply referenced external `.zcad` files is serialized into a flattened reference graph for memory optimization.

---

## 3. WebGL Viewport & Client Hardware

### 3.1 Legacy Hardware Fallback
- **Current State**: WebGL 1.0 / 2.0 with standard float textures and hardware antialiasing.
- **Limitation**: Systems lacking WebGL acceleration (e.g. headless VMs without virtual GPU drivers) automatically switch to the server-side vector drawing sheet and headless export modes.
- **Roadmap**: WebGPU rendering pipeline for 1,000,000+ triangle instant rendering on modern browser runtimes.
