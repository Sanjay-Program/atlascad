# AtlasCAD — Repository Structure & Architecture Blueprint

## Architectural Overview

AtlasCAD utilizes a hybrid modular architecture:
1. **Existing Verified Workspace:** High-performance production crates (`cad-*`) delivering core CAD, document, drafting, CAE, CAM, constraints, and web studio capabilities.
2. **Standardized Target Architecture:** Unifying platform crates (`atlas-*`) providing cross-disciplinary core abstractions, dependency DAG invalidation, engine orchestration, and multi-physics solver interfaces.

```text
atlas-cad/
├── Cargo.toml                      # Workspace root manifest (Resolver v3, Edition 2024)
├── Cargo.lock                      # Deterministic dependency lockfile
├── rust-toolchain.toml             # Pinned Rust toolchain (1.90 / stable)
├── LICENSE                         # Project license
├── README.md                       # Product documentation
│
├── apps/
│   ├── cadcraft/                   # Classical CAD desktop and CLI utilities
│   └── solidcraft/                 # High-integrity solid modeling kernel and vendored patches
│
├── crates/
│   ├── atlas-core/                 # Global contracts, UUIDs, capability tokens, error bus
│   ├── atlas-dependency-graph/     # Directed acyclic graph, invalidation scheduler, provenance
│   ├── atlas-engine-manager/       # 24-engine solver registry, process isolation, health checks
│   ├── atlas-simulation-core/      # Simulation problem, boundary conditions, convergence metrics
│   │
│   ├── cad-contract/               # Sandbox security contracts & grant algebra
│   ├── cad-kernel-truck/           # Truck B-Rep kernel bridge
│   ├── cad-engineering/            # Engine integration registry & supply chain compliance
│   ├── cad-cli/                    # Headless CLI entry points
│   ├── cad-agent-api/              # Autonomous agent interface contracts
│   ├── cad-agent-runtime/          # Sandboxed agent runtime
│   ├── cad-agent-geometry/         # Parametric agent geometry routines
│   ├── cad-document/               # ACID document transactions, crash journal, .zcad I/O
│   ├── cad-entitlements/           # Feature flags and licensing gates
│   ├── cad-engineering-model/      # Physical units, materials, mass properties, BOM, clashes
│   ├── cad-io/                     # STEP, IGES, STL, OBJ, glTF, DXF, PDF importers/exporters
│   ├── cad-studio/                 # Web server, WebGL viewport, HUDs, AutoCAD ribbon & prompt
│   ├── cad-drafting/               # 2D geometry, OSNAP engine, layers, blocks, annotations
│   ├── cad-cae/                    # Built-in linear FEA, modal dynamics, thermal solver
│   ├── cad-cam/                    # 3-axis CNC toolpaths, feeds/speeds, G-code postprocessing
│   └── cad-constraints/            # 2D constraint solver and DOF analyzer
│
├── kernels/
│   ├── truck/                      # Upstream pure-Rust B-Rep geometry kernel
│   ├── opencascade-rs/             # Open CASCADE Technology Rust bridge
│   └── monstertruck/               # Experimental high-concurrency solid kernel
│
├── engines/                        # Specialist engine adapters and configurations
│   ├── cad/                        # OCCT, FreeCAD, SolveSpace, Manifold
│   ├── meshing/                    # Gmsh, Netgen
│   ├── fea/                        # CalculiX, Code_Aster, OpenSees
│   ├── cfd/                        # OpenFOAM, SU2
│   ├── bim/                        # IfcOpenShell, EnergyPlus
│   ├── ecad/                       # KiCad, ngspice, Xyce
│   ├── electrical/                 # pandapower, OpenDSS
│   ├── chemical/                   # Cantera, DWSIM
│   ├── robotics/                   # MuJoCo, Pinocchio, ROS2
│   ├── aerospace/                  # OpenVSP, JSBSim
│   └── environmental/              # EPANET, SWMM, MODFLOW
│
├── third_party/                    # Provenance, license audits, notices, and SBOM
│   ├── README.md                   # Third-party policy and acquisition protocols
│   ├── MANIFEST.md                 # Upstream release registry and commit hashes
│   └── licenses/
│       ├── SPDX_LICENSE_INDEX.md   # Complete SPDX identifier database
│       ├── ATTRIBUTION.md          # Upstream authors and legal acknowledgments
│       └── THIRD_PARTY_NOTICES.md  # Combined legal notices
│
├── docs/
│   ├── architecture/               # System architecture, platform audits, DAG specs
│   ├── integration/                # Upstream repository registry, matrices, acquisition logs
│   ├── legal/                      # License classification, isolation policies, SBOM
│   └── validation/                 # Numerical validation benchmarks, test suites
│
├── deploy/                         # Production containerization, TLS proxy, local CA
├── scripts/                        # Automated CI validation, simulation, packaging
└── target/                         # Compiled Rust artifacts
```
