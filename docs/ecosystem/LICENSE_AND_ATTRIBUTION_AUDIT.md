# AtlasCAD License & Attribution Audit

## Executive Summary
This document classifies every evaluated upstream engineering engine by license tier and specifies the legally compliant architectural isolation boundary required by AtlasCAD.

---

## 1. License Classification Categories

| Category | Description | Permitted Integration Model in AtlasCAD | Examples |
| :--- | :--- | :--- | :--- |
| **Tier A: Permissive** | MIT, Apache-2.0, BSD-2-Clause, BSD-3-Clause, ISC, Public Domain/CC0 | Direct Rust crate compilation, static linking, embedding in core binary | truck, MuJoCo, GDAL, EnergyPlus, pandapower, OpenDSS, Cantera, RDKit, EPANET, SWMM, MODFLOW |
| **Tier B: Weak Copyleft** | LGPL-2.1, LGPL-2.1 with exception, LGPL-3.0 | Dynamic linking (`dlopen`), standalone shared libraries (`.so`/`.dylib`), C ABI boundary with clean relinking | Open CASCADE (OCCT), Netgen, SU2, IfcOpenShell, JSBSim, FEniCSx |
| **Tier C: Strong Copyleft**| GPL-2.0, GPL-3.0 | **Mandatory Process Isolation**: Subprocess execution via typed JSON/IPC, CLI adapters, containerized background workers. **Never statically linked or in-process compiled into core AtlasCAD binary.** | CalculiX (`ccx`), OpenFOAM, Gmsh, SolveSpace, KiCad, OpenSees, DWSIM, OpenRocket, LAMMPS |
| **Tier D: Network Copyleft**| AGPL-3.0 | Isolated network service with explicit user opt-in | Palabos |
| **Tier E: Specialized** | NASA Open Source Agreement (NOSA 1.3) | Process isolation via command-line interface | OpenVSP |

---

## 2. Upstream Compliance & Isolation Matrix

```mermaid
flowchart TD
    subgraph AtlasCAD_Core["AtlasCAD Core Platform (Rust / WebGL)"]
        UI["Unified Workstation UI"]
        Graph["Unified Engineering Graph"]
        Registry["Engine Registry & Orchestrator"]
        Truck["truck B-Rep Kernel (Apache-2.0)"]
        CAE["Built-in CAE (Rust)"]
        CAM["Built-in CAM (Rust)"]
    end

    subgraph Tier_A["Tier A: Permissive Engines (In-Process / Native)"]
        MuJoCo["MuJoCo (Apache-2.0)"]
        GDAL["GDAL (MIT)"]
        EPANET["EPANET (Public Domain)"]
        Cantera["Cantera (BSD-3)"]
    end

    subgraph Tier_B["Tier B: Weak Copyleft (Dynamic C ABI Boundary)"]
        OCCT["Open CASCADE (LGPL-2.1)"]
        IfcOpenShell["IfcOpenShell (LGPL-3.0)"]
        SU2["SU2 (LGPL-2.1)"]
    end

    subgraph Tier_C["Tier C: Strong Copyleft (Strict Subprocess / IPC Isolation)"]
        CalculiX["CalculiX ccx (GPL-2.0)"]
        OpenFOAM["OpenFOAM (GPL-3.0)"]
        Gmsh["Gmsh CLI (GPL-2.0+)"]
        KiCad["KiCad CLI (GPL-3.0)"]
        OpenSees["OpenSees CLI (Custom)"]
    end

    Registry -->|Direct Function Calls| Tier_A
    Registry -->|Dynamic Shared Library| Tier_B
    Registry -->|Standard IO / JSON IPC| Tier_C
```

---

## 3. Software Bill of Materials (SBOM) & Attribution Notice

AtlasCAD generates and maintains a CycloneDX 1.5 JSON SBOM (`engineering/sbom/atlascad.cdx.json`). All upstream projects are credited with copyright notices preserved in `dist/NOTICE.txt` and accessible in the client via `HELP` $\to$ `About AtlasCAD & Open Source Licenses`.
