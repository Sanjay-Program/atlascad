# AtlasCAD Integration Test Verification

## Overview
This document specifies the integration test matrix verifying that external engine adapters execute reliably through the AtlasCAD Rust orchestration layer and WebGL/SVG studio interface.

---

## 1. Automated Integration Test Harness

The AtlasCAD test suite executes across three tiers:
1. **Rust Workspace Unit & Integration Tests**: `cargo test --workspace` (514+ assertions).
2. **Server & End-to-End Simulation**: `python3 scripts/full-check.py` and `./scripts/simulate.sh` (368 assertions).
3. **Headless Browser Workstation Drive**: `./deploy/smoke.sh --advanced` (146 assertions).

---

## 2. Tested Engine Workflows

| Test Suite | Engine Tested | Tested Capability | Status |
| :--- | :--- | :--- | :---: |
| `tests/cae_workflow.rs` | `cad-cae` | Linear static stress, modal frequencies, thermal conduction | **PASS** |
| `tests/cam_workflow.rs` | `cad-cam` | Toolpath generation, Taylor tool-life feeds, G-code post | **PASS** |
| `tests/draft_workflow.rs`| `cad-drafting` | DXF export, vector PDF plotting, multi-layer transforms | **PASS** |
| `tests/constraint_tests.rs`| `cad-constraints` | Coincident, horizontal, vertical, distance, equal constraints | **PASS** |
| `tests/artifacts_workflow.rs`| `cad-io` / `cad-contract` | Binary STL serialization, B-Rep mesh volume consistency | **PASS** |
| `scripts/browser-drive.mjs` | `cad-studio` (Full UI) | WebGL styles, OSNAP glyphs, box selection, feature editing | **PASS** |
| `deploy/smoke.sh` | Reverse Proxy / Auth | TLS encryption, bearer token security, container health | **PASS** |
