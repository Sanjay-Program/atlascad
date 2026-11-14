# AtlasCAD — Third-Party Integration & Software Governance

This directory documents the provenance, licenses, and notices for all third-party open-source components integrated into or orchestrated by AtlasCAD.

## Directory Structure

- `MANIFEST.md`: Complete upstream release catalog, pinned commit hashes, and distribution modes.
- `licenses/SPDX_LICENSE_INDEX.md`: SPDX license inventory for all components.
- `licenses/ATTRIBUTION.md`: Copyright notices, project homepages, and developer attributions.
- `licenses/THIRD_PARTY_NOTICES.md`: Combined legal notices for distribution bundles.
- `patches/`: Vendored patches applied to upstream dependencies for stability and bugfixes.

## Principles

1. **No Mixed Source Code:** Third-party code is never commingled into AtlasCAD's core crate directories.
2. **Process Isolation for Copyleft:** All GPL/AGPL licensed engines run strictly in isolated subprocesses or containerized services.
3. **Immutability & Integrity:** Dependencies are pinned to exact commit SHAs or release tags.
