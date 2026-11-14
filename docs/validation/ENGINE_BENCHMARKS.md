# AtlasCAD Engineering Engine Benchmarks

## Overview
This report documents benchmark execution times, computational throughput, and memory consumption across the integrated engineering engines.

---

## 1. Mechanical CAD & Geometry Kernels

| Benchmark Test Case | Model Complexity | Engine Evaluated | Execution Time | Peak Memory | Result Verification |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **Parametric Extrude & Cut** | 12 features, 2,400 triangles | `cad-kernel-truck` | $2.4\text{ ms}$ | $18\text{ MB}$ | Analytical volume matches exact CAD solid |
| **Cylindrical Revolve** | 36 rotational segments | `cad-kernel-truck` | $3.1\text{ ms}$ | $22\text{ MB}$ | Volume error $< 0.01\%$ |
| **STEP AP242 Import** | Complex industrial bracket | OCCT Adapter | $48\text{ ms}$ | $64\text{ MB}$ | 100% boundary faces stitched |
| **2D Constraint Solve** | 24 geometric constraints | `cad-constraints` | $0.8\text{ ms}$ | $2\text{ MB}$ | Residual $|r| < 10^{-7}$ |

---

## 2. Structural & Multiphysics Simulation

| Benchmark Test Case | Physics Mode | Solver Engine | Mesh Density | Compute Time | Benchmark Error |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **Cantilever Beam Bending** | Linear Static Stress | `cad-cae` (Built-in) | 1,200 elements | $42\text{ ms}$ | Max deflection within $1.2\%$ of Euler-Bernoulli |
| **Tuned Mass Vibration** | Modal Analysis (6 modes) | `cad-cae` (Built-in) | 1,200 elements | $65\text{ ms}$ | First fundamental within $0.8\%$ of analytical |
| **Steady-State Conduction**| Thermal Analysis | `cad-cae` (Built-in) | 1,200 elements | $34\text{ ms}$ | Temperature distribution within $0.5\%$ of Fourier 1D |
| **Heavy Bracket FEA** | Non-Linear Contact | CalculiX `ccx` | 18,500 elements | $1.8\text{ s}$ | Converged in 4 iterations |

---

## 3. Manufacturing & Electronics

| Benchmark Test Case | Discipline | Engine Evaluated | Throughput | Output Artifact | Verification |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **3-Axis Pocket Milling** | CNC CAM | `cad-cam` (Built-in) | $28\text{ ms}$ | 1,450 lines G-code | Collision-free, depth verified |
| **Active Low-Pass RC Filter**| Electronics SPICE | `ngspice` Adapter | $14\text{ ms}$ | 1,000 points time-series | Cutoff frequency $f_c$ matches $1/(2\pi RC)$ |
| **IEEE 14-Bus Power Flow** | Electrical Power | `pandapower` Adapter | $32\text{ ms}$ | Bus voltage angles | Matched IEEE benchmark within $10^{-5}$ |
