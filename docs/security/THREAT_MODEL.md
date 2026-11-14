# AtlasCAD — Platform Security Architecture & STRIDE Threat Model
## Enterprise CAD/CAE/CAM Attack Surface Analysis & Defense-in-Depth Engineering

**Document Version:** 2.0.0  
**Classification:** Security Architecture & Threat Analysis  
**Security Mandate:** `#![forbid(unsafe_code)]`, memory safety, cryptographic tamper-evidence.  
**Author:** AtlasCAD Security Engineering & Product Integrity Group  

---

## 1. System Threat Model Overview

AtlasCAD processes untrusted user inputs (CAD models, STEP files, DXF drawings, simulation parameter sets, and G-code programs) and operates within enterprise defense, aerospace, and medical engineering environments. Security is treated as an intrinsic structural requirement.

```
[ Untrusted External Input ] (STEP / DXF / JSON / STL)
             │
             ▼
[ Input Sanitization & Bounds Checking ]
             │
             ▼
[ Memory-Safe Parsing (#![forbid(unsafe_code)]) ]
             │
             ▼
[ Immutable Audit Trail (128-Bit FNV-1a Hash Chain) ]
             │
             ▼
[ Sandboxed External Sidecar Execution (Subprocess) ]
```

---

## 2. STRIDE Threat Analysis Matrix

| Threat Category | Potential Attack Vector | AtlasCAD Architectural Mitigation |
|---|---|---|
| **Spoofing** | Forged user identities approving Engineering Change Orders (ECOs) | 3-tier cryptographic digital sign-offs (Lead Designer, Stress Engineer, Chief Architect) with cryptographic user fingerprints. |
| **Tampering** | Silent alteration of CAD dimensions, simulation stress results, or G-code toolpaths | Continuous 128-bit FNV-1a hash chain linking all engineering actions, geometry modifications, simulation runs, and approvals. Tamper verification fails closed if any byte deviates. |
| **Repudiation** | An engineer denies releasing an unvalidated or failing design revision | Immutable audit journal recording non-repudiable timestamps, user IDs, Git commit SHAs, and content hashes. |
| **Information Disclosure** | Intellectual property leakage over unauthorized cloud networks | 100% local-first, air-gapped architecture. Zero outbound telemetry or mandatory cloud connections. Strict CORS and HTTP origin validation on local studio server. |
| **Denial of Service** | Malformed geometry, infinite mesh refinement loops, or runaway simulation solvers | Pre-allocation bounds checking, input size limits, iteration caps (`max_iterations`), and execution time budgets with cancellation channels. |
| **Elevation of Privilege**| Buffer overflow or arbitrary code execution via malformed 3D files | `#![forbid(unsafe_code)]` declared across all 27 crates. External copyleft engines execute in unprivileged subprocesses without host process memory sharing. |

---

## 3. Machine Tool & Manufacturing Safety Safeguards

1. **Heightmap Collision Verification:** Before emitting G-code, subtractive toolpaths are simulated against remaining stock. Rapid gouges and tool holder collisions fail with explicit aborts.
2. **Mandatory Human-in-the-Loop Review:** G-code is never dispatched to physical machine tools automatically. Explicit operator review of cycle times, clearance planes, and spindle speeds is required.
3. **Emergency Stop Interlocks:** Industrial interfaces support immediate pause, retract, and feed-hold signals.
