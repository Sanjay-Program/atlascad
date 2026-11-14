# AtlasCAD — Legal License & Attribution Audit
## Commercial Compliance, Distribution Rights & Open-Source Governance

**Document Version:** 2.0.0  
**Classification:** Legal & Intellectual Property Audit  
**Author:** AtlasCAD Legal & Open-Source Governance Group  

---

## 1. Commercial IP Architecture

AtlasCAD is architected to allow frictionless commercial distribution, SaaS deployment, and air-gapped enterprise installation without risk of copyleft license contamination:

1. **Permissive Core:** The primary workstation binary (`cad-studio`) and all 27 crates in the workspace are authored under permissive licenses (MIT or Apache 2.0).
2. **Copyleft Isolation:** Strongly copyleft components (GPL 2.0, GPL 3.0, AGPL 3.0) such as CalculiX, Gmsh, OpenFOAM, and PrusaSlicer are never linked into the core application process. They are invoked as independent subprocesses across well-defined file and IPC boundaries.
3. **Patent & Trademark Preservation:** AtlasCAD uses original UI naming conventions, original vector iconography, and independent mathematical derivations. No proprietary CAD/CAM/CAE source code or protected trademark assets are incorporated.

---

## 2. Comprehensive Third-Party License Inventory

| Component / Crate | Upstream Project | License | Distribution Permission | Compliance Obligation |
|---|---|---|---|---|
| `cad-kernel-truck` | Truck (ricosjp) | Apache 2.0 | Permitted in commercial binary | Include Apache 2.0 notice & copyright |
| `serde` | Serde Project | MIT / Apache 2.0 | Permitted in commercial binary | Include MIT / Apache notice |
| `wgpu` | gfx-rs / wgpu | MIT / Apache 2.0 | Permitted in commercial binary | Include notice |
| `web-ifc` | ThatOpen | MPL 2.0 | Dynamic / WebAssembly | Distribute source modifications if modified |
| `CalculiX` | Guido Dhondt | GPL 2.0 | External subprocess sidecar | Execute as standalone `ccx` binary |
| `Gmsh` | Christophe Geuzaine | GPL 2.0 w/ exception | External subprocess sidecar | Execute as standalone `gmsh` binary |
| `OpenFOAM` | OpenCFD / ESI | GPL 3.0 | External subprocess sidecar | Execute as standalone container/process |
| `PrusaSlicer` | Prusa Research | AGPL 3.0 | External CLI sidecar | Execute as standalone CLI tool |
