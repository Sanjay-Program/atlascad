# AtlasCAD — License and Attribution Audit

## 1. Compliance Architecture & Tier Policies

AtlasCAD maintains strict license separation to protect proprietary work, honor open-source community terms, and ensure uncompromised legal compliance across enterprise and cloud deployments.

```text
┌────────────────────────────────────────────────────────────────────────┐
│                        AtlasCAD License Tiers                          │
├─────────┬───────────────────────────┬──────────────────────────────────┤
│ Tier    │ License Classifications   │ Integration & Distribution Model │
├─────────┼───────────────────────────┼──────────────────────────────────┤
│ Tier A  │ Apache-2.0, MIT, BSD, CC0 │ In-Process Static/Dynamic Link   │
│ Tier B  │ LGPL-2.1, LGPL-3.0        │ Dynamic Link / C ABI Only        │
│ Tier C  │ GPL-2.0, GPL-3.0          │ Standalone Process Isolated      │
│ Tier D  │ AGPL-3.0                  │ Network Service / Remote Worker  │
│ Tier E  │ Specialized / Custom      │ Commercial / Dual License Gate   │
└─────────┴───────────────────────────┴──────────────────────────────────┘
```

---

## 2. Third-Party Engine Licensing Inventory

| Engine | Upstream License | SPDX Identifier | Tier | Isolation Model |
|:---|:---|:---|:---:|:---|
| **truck** | Apache 2.0 | `Apache-2.0` | **Tier A** | In-Process Rust Dependency |
| **cad-cae** | Apache 2.0 / MIT | `Apache-2.0 OR MIT` | **Tier A** | Built-in native workspace crate |
| **cad-cam** | Apache 2.0 / MIT | `Apache-2.0 OR MIT` | **Tier A** | Built-in native workspace crate |
| **Open CASCADE (OCCT)** | LGPL 2.1 w/ Exception | `LGPL-2.1-only WITH OCCT-exception-1.0` | **Tier B** | Dynamic Library C ABI |
| **SU2** | LGPL 2.1 | `LGPL-2.1-only` | **Tier B** | Subprocess CLI Execution |
| **IfcOpenShell** | LGPL 3.0 | `LGPL-3.0-only` | **Tier B** | Subprocess CLI / Python bridge |
| **JSBSim** | LGPL 2.1 | `LGPL-2.1-only` | **Tier B** | Subprocess CLI Execution |
| **CalculiX (`ccx`)** | GPL 2.0 | `GPL-2.0-only` | **Tier C** | Process-Isolated Subprocess (`ccx`) |
| **Gmsh** | GPL 2.0+ | `GPL-2.0-or-later` | **Tier C** | Process-Isolated Subprocess (`gmsh`) |
| **OpenFOAM** | GPL 3.0 | `GPL-3.0-only` | **Tier C** | Process-Isolated Subprocess |
| **KiCad** | GPL 3.0+ | `GPL-3.0-or-later` | **Tier C** | Process-Isolated Subprocess (`kicad-cli`) |
| **DWSIM** | GPL 3.0 | `GPL-3.0-only` | **Tier C** | Process-Isolated Subprocess |
| **OpenSees** | BSD-style OpenSees | `BSD-3-Clause` | **Tier A** | Process-Isolated CLI bridge |
| **ngspice** | BSD-3-Clause / LGPL | `BSD-3-Clause` | **Tier A** | Dynamic library / Subprocess |
| **pandapower** | BSD 3-Clause | `BSD-3-Clause` | **Tier A** | Python Subprocess IPC bridge |
| **OpenDSS** | BSD 3-Clause | `BSD-3-Clause` | **Tier A** | C ABI Dynamic Link (`libdss_capi`) |
| **Cantera** | BSD 3-Clause | `BSD-3-Clause` | **Tier A** | C++ / Python bridge |
| **RDKit** | BSD 3-Clause | `BSD-3-Clause` | **Tier A** | C++ Dynamic / Python bridge |
| **MuJoCo** | Apache 2.0 | `Apache-2.0` | **Tier A** | C ABI Dynamic Link |
| **OpenVSP** | NASA Open Source 1.3 | `NOSA-1.3` | **Tier A** | Process-Isolated Subprocess |
| **EPANET** | Public Domain / CC0 | `CC0-1.0` | **Tier A** | C ABI Dynamic Link (`libepanet2`) |
| **EPA SWMM** | Public Domain / CC0 | `CC0-1.0` | **Tier A** | Subprocess CLI Execution |
| **MODFLOW** | Public Domain (US) | `CC0-1.0` | **Tier A** | Subprocess CLI Execution |
| **EnergyPlus** | BSD 3-Clause | `BSD-3-Clause` | **Tier A** | Subprocess CLI Execution |

---

## 3. SBOM Generation & Governance

Every AtlasCAD release executes automated Software Bill of Materials (SBOM) generation via CycloneDX 1.5 standard in `./scripts/package.sh`.
Attribution notices and copyright strings are cataloged in `third_party/licenses/THIRD_PARTY_NOTICES.md`.
