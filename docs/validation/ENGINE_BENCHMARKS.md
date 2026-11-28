# AtlasCAD Engineering Engine Benchmarks

This page records **only** benchmarks that were actually run on the development host against an
installed solver. Every row names the case, the measured value, the closed-form reference and the
command that produced it. The previous version of this page listed timings, memory figures and
"verified" results for kernels, CAM and engines that were never executed; those rows were removed
rather than reworded, because no measurement backed them.

## How a benchmark is produced

Every adapted engine is exercised through the same HTTP endpoint:

```bash
# point at the installed binaries and interpreter (or put them on PATH)
export ATLAS_CCX=/path/to/ccx
export ATLAS_NGSPICE=/path/to/ngspice
export ATLAS_GMSH=/path/to/gmsh
export ATLAS_PYTHON=/path/to/python # with pandapower and cantera installed
cargo run -p cad-studio -- --new /tmp/bench.zcad --listen 127.0.0.1:7781

curl -s -X POST http://127.0.0.1:7781/api/engines/benchmark \
  -H 'content-type: application/json' -H 'Origin: http://127.0.0.1:7781' \
  -d '{"engine_id":"ngspice"}'
```

The adapter writes the deck/netlist/script into a temporary directory, runs the real solver with a
timeout, parses the solver's own output and compares it with the closed-form answer. CalculiX,
ngspice and Gmsh are run as processes; pandapower, Cantera, OpenSees, RDKit, MuJoCo and EPANET are
libraries, so their adapters run a fixed script in the separate interpreter named by `ATLAS_PYTHON`
and parse the JSON it prints.
When the solver or module is absent the endpoint returns `501` with "no solver was run"; it never
synthesises a score.

## Measured 2026-11-27

| Engine | Version | Case | Measured | Closed-form reference | Deviation | Wall time | Provenance |
| --- | --- | --- | --- | --- | --- | --- | --- |
| CalculiX `ccx` | 2.23 | Uniaxial tension bar, 100 × 10 × 10 mm, E = 210 GPa, ν = 0.3, 100 N; 45 nodes, 16 C3D8 | Axial extension 0.0004742666 mm | `F L / (A E)` = 0.0004761905 mm | 0.404 % | 40–60 ms | `fnv1a64-6221fbfc274608d5` |
| ngspice | 41 | RC low-pass step, R = 1 kΩ, C = 1 µF, 1 V step; 526 timepoints, probes at 0.1/0.5/1/2/3/5 ms | 0.095153 V at 0.1 ms … 0.993262 V at 5 ms | `1 − exp(−t/RC)`, τ = 1 ms | 0.0101 % max | 20 ms | `fnv1a64-0e0692edd2cc5a45` |
| Gmsh | 4.15.2 | 10 × 10 × 10 mm cube, element size 2–4 mm, one thread, MSH 2.2 | 234 nodes, 1,190 elements, 726 tetrahedra summing to 999.9999999999983 mm³ | Cube volume 1,000 mm³ | 1.7 × 10⁻¹³ % | 100 ms | `fnv1a64-31b1646a103d6bad` |
| Gmsh | 4.15.2 | Active document meshed through `/api/engines/mesh-document`: 40 × 30 × 10 mm block, 3,072 subdivided surface triangles, target 3.33 mm | 1,875 nodes, 9,441 elements, 6,369 tetrahedra summing to 12,000.000000000004 mm³ | Document volume 12,000 mm³ | 3.0 × 10⁻¹⁴ % | 502 ms | `fnv1a64-d6cb0adcaf63b24e` |
| CalculiX `ccx` | 2.23 | Active document solved through `/api/engines/solve-document`: the 6,369-tetrahedron mesh above, 2 MPa on its 1,200 mm² top face, base clamped | 2,400.000018 N clamped reaction for 2,400.0 N applied (7.5 × 10⁻⁷ % residual), maximum axial displacement 1.0793 × 10⁻⁴ mm | Uniaxial `P L / E` for the free bar = 9.5238 × 10⁻⁵ mm | 13.3 % (see note) | 181 ms | `fnv1a64-240e2a2746434c89` |
| pandapower | 3.2.1 | Two-bus network: 1 pu slack, 1 km line 0.1 + j0.1 Ω/km, 1 MW constant-power load on a 1 MVA / 1 kV base | Receiving bus 0.8798668869081245 pu in 5 Newton iterations, 0.1292 MW line loss | High-voltage root of `|V|² = Re(V) − R P`, `Im(V) = −X P`: 0.8798668869081243 pu | 2.5 × 10⁻¹⁴ % | 985 ms | `fnv1a64-6001a60b63b81aa2` |
| Cantera | 3.2.0 | GRI-Mech 3.0: CH₄ + 2 O₂ → CO₂ + 2 H₂O(g) at 298.15 K; stoichiometric CH₄/air equilibrium at 1 atm | LHV 802.557 kJ/mol CH₄; flame temperature 2,224.6 K; `|cp − cv − R| / R` and `|ρ − pM/(RT)| / ρ` both 0.0 | Standard LHV of methane 802.3 kJ/mol | 0.032 % | 261 ms | `fnv1a64-b59124b2db7db41d` |
| OpenSeesPy | 3.8.0.0 | Cantilever, 1,000 mm × 10 × 10 mm, E = 210 GPa, 10 elasticBeamColumn elements, 1 N tip load | Tip displacement 1.9047619047612312 mm | `P L³ / (3 E I)` = 1.9047619047619047 mm | 3.5 × 10⁻¹³ relative | 161 ms | `fnv1a64-02f7c5625f8f5cba` |
| RDKit | 2026.9.2 | Benzene from SMILES `c1ccccc1`: formula, monoisotopic mass and average molecular weight | C₆H₆, 78.046950192 Da, 78.114 g/mol | Exact masses of ¹²C and ¹H (78.0469501914 Da); standard atomic weights (78.114 g/mol) | 7.7 × 10⁻¹⁰ % and 1.8 × 10⁻¹⁴ % | 261 ms | `fnv1a64-362c3084ea8bf05f` |
| MuJoCo | 4.0.1 | 1 kg point mass dropped from 10 m, 100,000 steps at 10 µs | Height after 1 s: 5.094950949999747 m | `z₀ − ½ g t²` = 5.095 m | 9.6 × 10⁻⁴ % (the integrator's truncation error) | 424 ms | `fnv1a64-77cd23226526a177` |
| EPANET via WNTR | 1.5.0 | 1 km × 300 mm pipe at C = 100 between a 100 m reservoir and a 0.05 m³/s junction demand | Junction head 97.10618591308594 m, flow 0.05000000074505806 m³/s | Hazen-Williams head loss with EPANET's SI coefficient (10.667, exponents 1.852/4.871): 97.10614270189 m | 4.4 × 10⁻⁵ % | 1,105 ms | `fnv1a64-ebc794b1ff6fac2f` |

Notes:

- The CalculiX case is a patch test: a uniform axial stress state is captured exactly, so the
  deviation measures the process, units and parser rather than mesh convergence. The deck is small
  on purpose; it is not a structural certification case.
- The ngspice case is linear, so the deviation is transient-integrator error at the default
  trapezoidal settings with a 10 µs maximum step. It is not an AC or distortion analysis.
- The document solve clamps every degree of freedom on the lowest face, which suppresses Poisson
  contraction there. Its maximum axial displacement is therefore 13.3 % above the free-bar
  `P L / E` reference; the exact check for that run is equilibrium, where the clamped reactions
  match the applied force to 7.5 × 10⁻⁷ %. The prismatic patch case in the HTTP test reproduces
  `P L / E` within 5 %.
- The pandapower reference is solved independently in Rust from the same network constants; the
  solver is compared with it, not with its own previous output.
- The Cantera reference is the standard enthalpy of combustion of methane, not a Cantera run. The
  flame temperature is reported as computed and carries no accuracy claim.
- The OpenSees case is a linear-elastic beam under a tip point load, which the element formulation
  represents exactly; the deviation is solver round-off, not a discretisation study.
- Wall times are single runs on a shared 8-core host and are indicative only.
- `provenance_hash` is FNV-1a over the deck/netlist plus the solver's output file. It is a content
  digest for reproducing a run, not a signature or a certification.

## Not measured

No benchmark exists for OpenFOAM, SU2, IfcOpenShell, EnergyPlus, KiCad, OpenDSS, DWSIM, OpenVSP,
JSBSim, SWMM, MODFLOW or OCCT: those 12 registry entries have no execution adapter under
`/api/engines/*`, so the endpoint refuses with `501`. Memory ceilings and hardware-GPU frame times
are also unmeasured; the headless frame-time figure on SwiftShader is recorded in PR-009 of the
remediation backlog instead.
