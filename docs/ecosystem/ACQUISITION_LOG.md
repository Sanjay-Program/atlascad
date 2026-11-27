# AtlasCAD Engine Acquisition & Verification Log

Acquisition here means **provenance and licence research**: identifying the canonical upstream,
pinning a release and confirming the licence. It is not integration.

Earlier versions of this page recorded "Gate A–E passed" and an `ADAPTED` status for every engine.
No build, adapter or numerical gate had been run for those engines, so those statements were
removed.

## Executed and verified on this host

| Engine | Version | How it was obtained | What was actually verified |
| --- | --- | --- | --- |
| `truck` | 0.6.0 (vendored) | Workspace dependency (`crates/cad-kernel-truck`) | Parametric extrude/cut/revolve and export paths covered by workspace tests |
| CalculiX `ccx` | 2.23 | `pip install cad-ccx` (conda-forge build), rootless | Real axial-tension run parsed and compared with `F L/(A E)`: 0.404% error. See [`ENGINE_BENCHMARKS.md`](../validation/ENGINE_BENCHMARKS.md) |
| ngspice | 41 | conda-forge via micromamba, rootless | Real RC step transient parsed and compared with `1 - exp(-t/RC)`: 0.0101% max error |

Both external adapters are benchmark-scoped: `/api/engines/health` reports the detected binary and
`/api/engines/benchmark` returns the measured comparison. General job execution is not implemented,
so `/api/engines/run` refuses with `501`.

## Researched, not integrated

The remaining entries in
[`UPSTREAM_REPOSITORY_REGISTRY.md`](UPSTREAM_REPOSITORY_REGISTRY.md) are provenance records: the
canonical repository, pinned release and licence for engines that AtlasCAD may support later. No
binary is bundled, downloaded at build time or invoked, and no result is produced for them.

## Provenance and licence policy

- Every candidate is recorded with its canonical URL, pinned version and SPDX licence.
- Copyleft engines (GPL/LGPL) are only considered for process isolation, never static linking.
- No engine's source is vendored without an entry in `third_party/MANIFEST.md` and
  `third_party/licenses/ATTRIBUTION.md`.
- A licence record is not a compatibility statement; compatibility is only claimed after an
  adapter runs in CI or on a recorded host.
