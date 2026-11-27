# AtlasCAD Engineering Engine Benchmarks

This page records **only** benchmarks that were actually run on the development host against an
installed solver. Every row names the case, the measured value, the closed-form reference and the
command that produced it. The previous version of this page listed timings, memory figures and
"verified" results for kernels, CAM and engines that were never executed; those rows were removed
rather than reworded, because no measurement backed them.

## How a benchmark is produced

Both external engines are exercised through the same HTTP endpoint:

```bash
# point at the installed binaries (or put them on PATH)
export ATLAS_CCX=/path/to/ccx
export ATLAS_NGSPICE=/path/to/ngspice
cargo run -p cad-studio -- --new /tmp/bench.zcad --listen 127.0.0.1:7781

curl -s -X POST http://127.0.0.1:7781/api/engines/benchmark \
  -H 'content-type: application/json' -H 'Origin: http://127.0.0.1:7781' \
  -d '{"engine_id":"ngspice"}'
```

The adapter writes the deck/netlist into a temporary directory, runs the real binary with a
timeout, parses the solver's own output file and compares it with the closed-form answer. When the
binary is absent the endpoint returns `501` with "no solver was run"; it never synthesises a score.

## Measured 2026-11-27

| Engine | Version | Case | Measured | Closed-form reference | Deviation | Wall time | Provenance |
| --- | --- | --- | --- | --- | --- | --- | --- |
| CalculiX `ccx` | 2.23 | Uniaxial tension bar, 100 × 10 × 10 mm, E = 210 GPa, ν = 0.3, 100 N; 45 nodes, 16 C3D8 | Axial extension 0.0004742666 mm | `F L / (A E)` = 0.0004761905 mm | 0.404 % | 40–60 ms | `fnv1a64-6221fbfc274608d5` |
| ngspice | 41 | RC low-pass step, R = 1 kΩ, C = 1 µF, 1 V step; 526 timepoints, probes at 0.1/0.5/1/2/3/5 ms | 0.095153 V at 0.1 ms … 0.993262 V at 5 ms | `1 − exp(−t/RC)`, τ = 1 ms | 0.0101 % max | 20 ms | `fnv1a64-0e0692edd2cc5a45` |

Notes:

- The CalculiX case is a patch test: a uniform axial stress state is captured exactly, so the
  deviation measures the process, units and parser rather than mesh convergence. The deck is small
  on purpose; it is not a structural certification case.
- The ngspice case is linear, so the deviation is transient-integrator error at the default
  trapezoidal settings with a 10 µs maximum step. It is not an AC or distortion analysis.
- Wall times are single runs on a shared 8-core host and are indicative only.
- `provenance_hash` is FNV-1a over the deck/netlist plus the solver's output file. It is a content
  digest for reproducing a run, not a signature or a certification.

## Not measured

No benchmark exists for OpenFOAM, pandapower, Elmer, Gmsh, SU2, MuJoCo, Cantera, EPANET or the
other registered engines: they have no execution adapter under `/api/engines/*`, so the endpoint
refuses with `501`. Memory ceilings and hardware-GPU frame times are also unmeasured; the headless
frame-time figure on SwiftShader is recorded in PR-009 of the remediation backlog instead.
