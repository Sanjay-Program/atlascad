# AtlasCAD Unified Engine Integration Architecture

## Overview
AtlasCAD orchestrates external and native engineering engines through a unified Rust runtime contract. The architecture decouples the primary CAD document model and interactive user interface from specialized external computational solvers.

---

## 1. Engine Adapter Trait (`EngineAdapter`)

Every integrated engineering engine implements the standard `EngineAdapter` trait:

```rust
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineInfo {
    pub id: String,
    pub name: String,
    pub discipline: String,
    pub version: String,
    pub license: String,
    pub license_tier: String, // "Tier A", "Tier B", "Tier C"
    pub execution_model: String, // "In-Process", "C ABI", "Subprocess"
    pub available: bool,
    pub binary_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineJobRequest {
    pub job_id: String,
    pub engine_id: String,
    pub model_revision: u64,
    pub input_data: serde_json::Value,
    pub timeout_seconds: u64,
    pub max_memory_mb: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineJobResult {
    pub job_id: String,
    pub engine_id: String,
    pub success: bool,
    pub elapsed_ms: u64,
    pub output_data: serde_json::Value,
    pub error_message: Option<String>,
    pub warnings: Vec<String>,
    pub provenance_hash: String,
}

pub trait EngineAdapter: Send + Sync {
    fn info(&self) -> EngineInfo;
    fn check_environment(&self) -> Result<(), String>;
    fn prepare_job(&self, req: &EngineJobRequest, work_dir: &Path) -> Result<PathBuf, String>;
    fn execute_job(&self, req: &EngineJobRequest, work_dir: &Path) -> Result<EngineJobResult, String>;
    fn cancel_job(&self, job_id: &str) -> Result<(), String>;
}
```

---

## 2. Process Isolation & Execution Sandbox

For Tier C (GPL) and Tier D (AGPL) engines:
1. **Subprocess Isolation**: Solvers are invoked as external child processes via `std::process::Command` with standard IO redirection.
2. **Resource Boundaries**: Solvers run with strict timeout deadlines and maximum memory ceilings.
3. **No In-Process Linkage**: AtlasCAD core Rust binary communicates with external tools exclusively via JSON IPC, standard input/output, or temporary disk files, safeguarding AtlasCAD's intellectual property and license boundaries.

---

## 3. Runtime Engine Registry (`EngineRegistry`)

The `EngineRegistry` maintains an inventory of available solvers:
- Discovers system installations (e.g. `/usr/bin/gmsh`, `/usr/bin/ccx`, `/usr/bin/ngspice`, `/usr/bin/openfoam`).
- Reports real-time engine availability and health status to the UI.
- Directs simulation requests to either the fast built-in Rust solver (e.g. `cad-cae`) or external high-fidelity engines (e.g. CalculiX / OpenFOAM).
