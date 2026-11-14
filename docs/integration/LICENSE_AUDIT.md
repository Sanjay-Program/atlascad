# AtlasCAD — Open-Source License Audit & Compliance Review
## IP Boundary Isolation, Permissive vs. Copyleft Governance & SBOM Certification

**Document Version:** 2.0.0  
**Classification:** Legal & Licensing Governance  
**Author:** AtlasCAD Open-Source Compliance & Intellectual Property Group  

---

## 1. Compliance Architecture & Isolation Strategy

AtlasCAD maintains strict separation between permissive proprietary/commercial binaries and copyleft open-source tools:

```
┌────────────────────────────────────────────────────────────────────────┐
│                   COMMERCIAL / ENTERPRISE CORE BINARIES                │
│    All Native Crates: MIT / Apache 2.0 / BSD Permissive Only           │
│    Zero GPL, Zero AGPL statically linked in shipping binary            │
└───────────────────────────────────┬────────────────────────────────────┘
                                    │ (Inter-Process Communication / JSON)
                                    ▼
┌────────────────────────────────────────────────────────────────────────┐
│                   ISOLATED EXTERNAL PROCESS SIDECARS                   │
│    CalculiX (GPL), Gmsh (GPL), OpenFOAM (GPL), PrusaSlicer (AGPL)      │
│    Executed as standalone subprocesses managed by atlas-job-queue      │
└────────────────────────────────────────────────────────────────────────┘
```

---

## 2. License Tiers & Boundary Rules

### Tier 1: Permissive Licenses (Direct In-Memory Linking Permitted)
- **Licenses:** MIT, Apache 2.0, BSD-2-Clause, BSD-3-Clause.
- **Rules:** Fully permitted for static compilation and direct inclusion inside all 27 core AtlasCAD crates.
- **Packages:** `truck` (Apache 2.0), `wgpu` (MIT / Apache 2.0), `serde` (MIT / Apache 2.0), `pandapower` (BSD), `Cantera` (BSD), `MuJoCo` (Apache 2.0).

### Tier 2: Weak Copyleft Licenses (Dynamic Linking / File Boundary)
- **Licenses:** LGPL 2.1, LGPL 3.0, MPL 2.0.
- **Rules:** Permitted when linked dynamically as shared libraries (`.so` / `.dll`) or compiled into isolated WebAssembly modules (e.g. `web-ifc` under MPL 2.0). Source modifications to the library itself must be contributed back, without encumbering calling code.
- **Packages:** `web-ifc` (MPL 2.0), `OpenCASCADE` (LGPL 2.1 w/ exception), `SU2` (LGPL 2.1), `preCICE` (LGPL 3.0).

### Tier 3: Strong Copyleft Licenses (Strict Process Boundary Required)
- **Licenses:** GPL 2.0, GPL 3.0, AGPL 3.0.
- **Rules:** **STRICTLY FORBIDDEN** from being statically or dynamically linked into the shipping AtlasCAD executable (`cad-studio`). Must only be invoked as separate, standalone executables communicating across IPC pipes, command-line interfaces, or temporary filesystem artifacts.
- **Packages:** `CalculiX` (`ccx`), `Gmsh`, `OpenFOAM`, `PrusaSlicer`.

---

## 3. Automated License Verification & Audit Trail

Every release build executes an automated license scanner (`cargo deny` / custom script) verifying that no GPL/AGPL crates exist in the dependency tree of `cad-studio`. Attribution notices and third-party licenses are bundled automatically into `docs/legal/THIRD_PARTY_NOTICES.md`.
