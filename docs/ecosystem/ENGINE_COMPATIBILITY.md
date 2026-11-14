# AtlasCAD Engine Platform & Hardware Compatibility Guide

## Overview
AtlasCAD supports multi-tier hardware architectures ranging from modest laptops with integrated graphics to multi-GPU workstations and high-performance computing (HPC) Linux clusters.

---

## 1. Operating System & Platform Support

| Operating System | Architecture | Binary Support | Native Acceleration | Tested Environments |
| :--- | :--- | :--- | :--- | :--- |
| **Linux (Enterprise & Desktop)** | `x86_64` | Native ELF64 | WebGL, OpenMP, POSIX threads | Ubuntu 22.04/24.04, Fedora 39/40, RHEL 9 |
| **Linux (ARM64)** | `aarch64` | Native ELF64 | NEON, WebGL | AWS Graviton 3/4, Raspberry Pi 5 |
| **macOS** | `aarch64` (Apple Silicon) | Universal 2 | Metal / WebGL, Accelerate framework | macOS 14 Sonoma, macOS 15 Sequoia |
| **Windows** | `x86_64` | PE32+ (x64) | DirectX / WebGL, MSVC runtime | Windows 10/11 Pro & Enterprise |
| **Containerized / Cloud** | Docker / OCI | Dockerfile.dist | Headless software rasterizer (LLVMpipe) | Debian Bookworm slim, Alpine |

---

## 2. Hardware Resource Profiles

### Profile 1: Light Mobile / Integrated GPU (8 GB RAM)
- **Workspaces Active**: Drafting, Mechanical Modeling, Electrical Schematics, Water Hydraulics.
- **Mesh Limits**: Up to 50,000 triangles.
- **Solvers**: Built-in fast voxel CAE, ngspice, EPANET.
- **Viewport**: Flat/Shaded styles, 60 FPS capped.

### Profile 2: Standard Engineering Workstation (16–32 GB RAM)
- **Workspaces Active**: All workspaces (FEA, CFD, BIM, Chemical Flowsheet, Robotics).
- **Mesh Limits**: Up to 500,000 triangles.
- **Solvers**: Multi-threaded CalculiX, SU2 aerodynamics, pandapower grid flow.
- **Viewport**: All 12 visual styles with real-time shadow sheen and Gooch shading.

### Profile 3: Heavy Simulation & HPC Cluster (64+ GB RAM, Multi-Core)
- **Workspaces Active**: High-resolution OpenFOAM LES/DES, complex IfcOpenShell BIM federations, non-linear seismic OpenSees.
- **Solvers**: Distributed MPI execution, background batch job queueing with cancellation and resource monitoring.
