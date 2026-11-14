# AtlasCAD — Software Supply Chain Security Architecture
## Dependency Governance, SBOM Generation, Vendor Patching & Reproducible Builds

**Document Version:** 2.0.0  
**Classification:** Supply Chain Security & Build Integrity  
**Author:** AtlasCAD Release Engineering & Product Security Group  

---

## 1. Supply Chain Architecture

AtlasCAD adheres to federal and enterprise cybersecurity standards for critical engineering software (including Executive Order 14028 and NIST SP 800-218 SSDF):

```
[ Cargo.lock Deterministic Pinning ]
                 │
                 ▼
    [ cargo audit & cargo deny ]
                 │
                 ▼
[ Automated License & Vulnerability Scanning ]
                 │
                 ▼
    [ Verified Vendor Patching ]
                 │
                 ▼
[ Software Bill of Materials (SBOM) Export ]
                 │
                 ▼
[ Cryptographically Signed Release Binaries ]
```

---

## 2. Key Supply Chain Safeguards

### 2.1 Deterministic Dependency Pinning
- All direct and transitive dependencies are locked in `Cargo.lock`.
- Builds are fully reproducible across development, continuous integration, and air-gapped production environments.

### 2.2 Automated Vulnerability Auditing
- Continuous scanning via `cargo audit` against the RustSec Advisory Database.
- Strict rejection of dependencies with known CVEs, unmaintained status, or security advisories.

### 2.3 Vendor Patching Governance
- Upstream patches (such as `truck-shapeops` Boolean enhancements) are maintained within the `vendor/` tree with explicit diff logs, provenance records, and author attribution.
- Zero untracked or binary-only third-party blobs.

### 2.4 Software Bill of Materials (SBOM) Export
- Automated generation of SPDX and CycloneDX compliant SBOM metadata for every production release.
- Includes component names, exact version tags, SHA-256 package digests, license classifications, and supplier provenance.
