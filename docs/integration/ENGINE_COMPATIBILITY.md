# AtlasCAD — Engine Compatibility & Multi-Platform Matrix

## Operating System & Hardware Profile


> **Status correction (2026-11-27).** "Pre-installed" below was wrong: the container image bundles
> no external engine. Only CalculiX and ngspice were installed and executed for benchmark
> verification; every other engine must be supplied by the operator.

| Engine | Linux (x86_64) | macOS (Apple Silicon) | Windows (x64) | Container (Docker) | Min RAM | Recommended RAM |
|:---|:---:|:---:|:---:|:---:|:---:|:---:|
| `truck` | Native | Native | Native | Native | 512 MB | 2 GB |
| `cad-cae` | Native | Native | Native | Native | 1 GB | 4 GB |
| `cad-cam` | Native | Native | Native | Native | 512 MB | 2 GB |
| `occt` | Native (SO) | Native (DYLIB) | Native (DLL) | Native | 2 GB | 8 GB |
| `gmsh` | Binary CLI | Binary CLI | Binary CLI | Not bundled | 1 GB | 8 GB |
| `calculix` | Binary CLI (`ccx`)| Homebrew CLI | Native Win64 | Not bundled | 2 GB | 16 GB |
| `openfoam` | POSIX Native | Docker / UTM | WSL2 / Docker | Production Image | 4 GB | 32 GB |
| `su2` | Binary CLI | Native CLI | Native CLI | Not bundled | 2 GB | 16 GB |
| `ifcopenshell` | Python / C ABI | Python / C ABI | Python / C ABI | Not bundled | 1 GB | 4 GB |
| `energyplus` | Binary CLI | Binary CLI | Binary CLI | Not bundled | 1 GB | 4 GB |
| `opensees` | Binary CLI | Binary CLI | Binary CLI | Not bundled | 1 GB | 8 GB |
| `ngspice` | Native SO | Native DYLIB | Native DLL | Not bundled | 512 MB | 2 GB |
| `kicad` | CLI tool | CLI tool | CLI tool | Not bundled | 2 GB | 8 GB |
| `pandapower` | Python Bridge | Python Bridge | Python Bridge | Not bundled | 1 GB | 4 GB |
| `opendss` | Native SO | Native DYLIB | Native DLL | Not bundled | 1 GB | 4 GB |
| `cantera` | Native / Python | Native / Python | Native / Python | Not bundled | 1 GB | 4 GB |
| `dwsim` | .NET Core CLI | .NET Core CLI | .NET Core CLI | Not bundled | 2 GB | 8 GB |
| `rdkit` | Native / Python | Native / Python | Native / Python | Not bundled | 1 GB | 4 GB |
| `openvsp` | Binary CLI | Binary CLI | Binary CLI | Not bundled | 1 GB | 4 GB |
| `jsbsim` | Binary CLI | Binary CLI | Binary CLI | Not bundled | 512 MB | 2 GB |
| `mujoco` | Native SO | Native DYLIB | Native DLL | Not bundled | 1 GB | 4 GB |
| `epanet` | Native SO | Native DYLIB | Native DLL | Not bundled | 256 MB | 1 GB |
| `swmm` | Binary CLI | Binary CLI | Binary CLI | Not bundled | 512 MB | 2 GB |
| `modflow` | Binary CLI | Binary CLI | Binary CLI | Not bundled | 1 GB | 4 GB |
