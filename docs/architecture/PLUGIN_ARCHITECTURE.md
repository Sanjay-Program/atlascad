# AtlasCAD Plugin & Extensibility Architecture

## Overview
AtlasCAD provides a secure, typed plugin architecture allowing third-party engineering developers to introduce new geometry generators, analysis solvers, import/export formats, and custom ribbon workflows.

---

## 1. Plugin Execution Models

AtlasCAD supports three distinct plugin interfaces:

1. **Native Rust Plugins**: Crates linking directly against `atlas-plugin-sdk` / `cad-contract`. Compiled as dynamic libraries (`.so`/`.dylib`/`.dll`) or built into custom deployment binaries.
2. **Process-Isolated RPC / CLI Plugins**: Standalone external executables communicating with AtlasCAD over standard JSON-RPC 2.0 via standard I/O or Unix domain sockets.
3. **WebAssembly (WASM) Sandboxed Plugins**: High-performance, memory-safe in-browser / in-engine plugins executing in isolated WebAssembly runtimes with zero access to the host filesystem.

---

## 2. Plugin Manifest (`atlas-plugin.json`)

```json
{
  "name": "solar-pv-analysis",
  "version": "1.2.0",
  "author": "AtlasCAD Energy Group",
  "license": "Apache-2.0",
  "description": "Photovoltaic irradiance and shading simulation for BIM buildings",
  "discipline": "architecture",
  "entrypoint": "bin/solar_pv_worker",
  "capabilities": [
    "solar-irradiance",
    "shading-calculation",
    "pv-yield"
  ],
  "ribbon_contributions": [
    {
      "workspace": "architecture",
      "tab": "Energy",
      "button_id": "rbSolarPvBtn",
      "label": "Solar PV Yield",
      "tooltip": "Calculate annual photovoltaic energy yield",
      "command": "SOLARPV"
    }
  ]
}
```

---

## 3. Security & Sandboxing Rules
- **Permission Manifest**: Plugins must declare required permissions (e.g. `network:outbound`, `fs:read`, `fs:write`).
- **Cryptographic Signatures**: Enterprise deployments verify developer certificates before loading plugins.
- **Resource Ceilings**: Out-of-memory and runaway infinite loops are terminated cleanly by the AtlasCAD process supervisor without crashing the workstation.
