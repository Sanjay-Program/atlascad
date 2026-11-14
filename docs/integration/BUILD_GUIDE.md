# AtlasCAD — Compilation, Build & Quality Assurance Guide
## Step-by-Step Developer Setup, Toolchain Configuration & Test Suite Execution

**Document Version:** 2.0.0  
**Classification:** Developer & DevOps Operations Manual  
**Author:** AtlasCAD Release Engineering & QA Group  

---

## 1. Prerequisites & Toolchain Requirements

- **Rust:** Stable Rust toolchain (1.80 or higher). Managed via `rustup`.
- **Operating System:** Linux (x86_64 or aarch64), macOS (Apple Silicon or Intel), or Windows (MSVC).
- **Python:** Python 3.8+ for automated live demonstration and regression verification scripts.
- **Node.js (Optional):** Required only if headless Chromium end-to-end browser tests are executed.

---

## 2. Compilation Commands

### 2.1 Debug Build
```bash
cargo build --workspace
```

### 2.2 Optimized Release Build
To generate the production-ready high-performance CAD/CAM/CAE workstation binary:
```bash
cargo build --release -p cad-studio
```
The compiled server executable is placed at `target/release/cad-studio`.

---

## 3. Comprehensive Verification & Testing Battery

### 3.1 Unit & Integration Test Battery
Execute all unit tests, integration tests, and doc-tests across all 27 workspace crates:
```bash
cargo test --workspace
```

### 3.2 Strict Linter & Static Analysis
Enforce `#![forbid(unsafe_code)]` and zero clippy warnings across all features and test targets:
```bash
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

### 3.3 Formatting Compliance
Verify standard Rust style conventions:
```bash
cargo fmt --check
```

### 3.4 Automated Live Demo Verification
Launches a transient release server and verifies 85 analytical physics and multi-disciplinary assertions:
```bash
python3 scripts/live-demo.py
```

### 3.5 Full Exhaustive Check
Runs 312 assertions verifying protocol limits, sketches, drafting, export formats, and recovery:
```bash
python3 scripts/full-check.py
```

### 3.6 End-to-End Simulation Suite
Runs full HTTP, CLI control-plane, and WebGL browser simulations:
```bash
./scripts/simulate.sh
```

---

## 4. Running the AtlasCAD Studio Server

To start the local workstation server on `127.0.0.1:8080`:
```bash
./target/release/cad-studio --bind 127.0.0.1:8080
```
Open a modern web browser to `http://127.0.0.1:8080` to access the full 3D CAD modeling, 2D drafting, CAE simulation, CAM machining, and additive slicing workbenches.
