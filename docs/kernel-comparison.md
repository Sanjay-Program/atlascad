# Kernel Comparison and Selection

**Date:** 2026-10-21 · **Host:** i3-1125G4 (4C/8T), 7.4 GB RAM, Intel UHD G4, rustc 1.94.0
**Decision:** **truck 0.6 is the primary 3D kernel — specifically SolidCraft's patched copy of it, not
the published crates alone.** Everything else stays isolated as a reference.

---

## 1. Method, and what "verified" means here

Claims are graded, and only one grade is a claim about behaviour:

| Grade | Meaning |
|---|---|
| **VERIFIED** | Built and executed **on this machine**, with the command and result recorded |
| **READ** | Read from the repository's source/manifests/README, not executed |
| **ABSENT** | Looked for and not found |

Nothing below says a feature works because a README says so. Where a candidate was not built, the
reason is stated. The full evidence for truck's behaviour is reproducible with:

```sh
cargo run --release -p cad-kernel-truck --example probe     # boolean matrix, coplanar vs non-coplanar
cargo run --release -p cad-kernel-truck --example probe5    # coplanar + nudge experiment
cargo run --release -p cad-kernel-truck --example probe6    # per-shell volume localisation
cargo run --release -p cad-kernel-truck --example chain     # union chain limit
cargo run --release -p cad-kernel-truck --example bench     # timings + correctness
cargo test  --release -p cad-kernel-truck                   # 13 tests
```

## 2. Candidates at a glance (facts verified from the checkouts)

| Kernel | Language | Licence (verified) | Native dependency | Build status here |
|---|---|---|---|---|
| **truck** | pure Rust | **Apache-2.0** (`LICENSE`) | none | **VERIFIED**: `solidcraft-kernel` (which uses it) builds; adapter builds and runs |
| monstertruck | pure Rust | Apache-2.0 | none | **not built** — a fork of truck; nothing to gain |
| cascade-rs | pure Rust | Apache-2.0 (`Cargo.toml`) | none (`opencascade-sys` commented out) | **not built** — see §6 |
| hagane | pure Rust | MIT OR Apache-2.0 | none (no `build.rs`) | **not built** — see §6 |
| cadrum | Rust + 5 C/C++ files | wrapper MIT, **statically links OCCT (LGPL-2.1)** | `build.rs`, cmake or prebuilt tarball | **not built** — licence + build cost |
| opencascade-rs | Rust bindings | **LGPL-2.1** | cmake → OCCT | **not built** — licence decision required |

## 3. Feature matrix

Legend: **V** verified by execution · **R** read from source · **—** not present · **?** not tested

| Capability | truck (+SolidCraft patch) | monstertruck | cascade-rs | hagane | cadrum | opencascade-rs |
|---|---|---|---|---|---|---|
| Points/vectors/transforms | **V** (used to build test geometry) | R | R | R | R | R |
| Analytic curves/surfaces | **V** (planar faces, lines) | R | R | R | R | R |
| NURBS / B-spline | R (`truck-geometry`) | R | R | R | R | R |
| B-rep topology | **V** (`Solid`, shells, faces, edges) | R | R | R | R | R |
| Solid primitives | **V** (extruded prisms) | R | R | R | R | R |
| Boolean union | **V** — 1120 mm³ for a 1000+160−40 fixture, 0.0037 mm³ error; corpus: partial overlap passes | R | R | R | R | R |
| Boolean intersection | **V** — 40 mm³ for a 2×2×10 overlap; corpus: thin wall and near-degenerate sliver pass | R | R | R | R | R |
| Boolean difference | **V** — exact for a tool that overshoots the body (§4.2); coincident faces remain a recorded limitation | ? | ? | ? | ? | ? |
| Boolean chains (3+ operands) | **V — fails at the third union** (§4.3) | ? | ? | ? | ? | ? |
| Coplanar-face booleans | **V — fails without a nudge** (§4.1) | R | R | R | R | R |
| Extrude | **V** — 148 753 boxes/s, exact analytic volume | R | R | R | R | R |
| Revolution (full turn) | **V** — truck's `rsweep`; a rectangle on the axis measures 150.796 mm³ against π·4²·3 within 0.05, an offset rectangle 113.097 mm³ against π·(4²−2²)·3, both watertight; a partial turn and a profile crossing the axis are refused | R | R | R | R | R |
| Sweep / loft | — in the adapter; R in SolidCraft's kernel | R | R | R | R | R |
| Fillet / chamfer | — in the adapter; **R + shipped** in SolidCraft's kernel (own implementation) | R | R | R | R | R |
| Shell / offset | — in the adapter; R in SolidCraft | R | R | R | R | R |
| Tessellation | **V** — valid, watertight (0 open edges), outward-oriented, correct volume, 12 tris for a box at 0.01 mm. Requires the adapter's weld + orientation repair; the raw output was per-face and had an inward cap (ADR-0005) | R | R | R | R | R |
| Tolerance handling | **V** — kernel minimum 1e-4 enforced; boolean sensitivity measured | R | R | R | R | R |
| Cancellation / deadline | **V** — every operation takes a `Budget` (deadline + cancellation token), checks it at internal checkpoints, and leaves the instance usable after an interruption (ADR-0006) | ? | ? | ? | ? | ? |
| STEP | R in SolidCraft (`truck-stepio`, patched) | R | R | R | R | R |
| STL | R (SolidCraft `crates/io`) | R | R | R | R | R |
| WASM | **?** no target installed; nothing claimed | R | R | R (demo GIF claims a WASM build) | R (claims WASM) | R |
| Determinism of boolean result | **V** — volume stable across runs (spread 0.000000000 mm³); face order is *not* stable upstream (see §4.4) | ? | ? | ? | ? | ? |

### 3.1 The correctness corpus

`crates/cad-kernel-truck/examples/corpus.rs` is the regression gate that the "?" rows above need.
It runs ten analytic fixtures against the adapter, each with a stated volume and area tolerance, and
prints a pass/fail table. The exit code is the gate: a fixture that previously passed and now fails
makes the binary exit non-zero.

| Fixture | Expected | Result |
|---|---|---|
| `box_control` | 1000 mm³ | **ok** |
| `non_axis_aligned_box` | analytic | **ok** |
| `union_partial_overlap` | 1120 mm³ | **ok** |
| `intersection_thin_wall` | analytic | **ok** |
| `intersection_near_degenerate_sliver` | analytic | **ok** |
| `union_tangent_contact` | analytic | **known failure** — the tangent case does not produce a solid |
| `intersection_large_coordinates` | analytic (coords near 1e6 mm) | **known failure** — the kernel loses precision at that scale |
| `difference_piercing_cut` | 84 000 mm³ / 22 400 mm² | **ok** — delta 0 |
| `difference_contained_void` | 184 000 mm³ / 32 800 mm² | **ok** — delta 0 |
| `difference_coplanar_cut` | 84 000 mm³ / 22 400 mm² | **known failure** — volume 84 000.18, area 25 600, not watertight (§4.2) |

Measured: **8 ok, 3 known failures, 0 regressions, 0 unexpected passes**, exit code 0. The two cut
fixtures that pass are exact rather than close: their reported delta is 0.000e0.

Two things are worth stating plainly. First, the two known failures were **not known before the
corpus existed**: tangent contact and large-coordinate intersection were invisible to the
hand-written tests, which is precisely the argument for a corpus over ad-hoc cases. Second, a known
failure is recorded as a **baseline**, not a skip — the corpus prints it, the gate stays green, and a
future fix appears as an unexpected pass that has to be promoted deliberately. Silently skipping the
two would have made the corpus agree with itself rather than with the kernel.

The real kernel (`solidcraft-kernel`) has **not** been run against this corpus: it needs the ≥16 GB
RAM host. Until it is, the "two kernels, one corpus" comparison this document is meant to deliver
does not exist, and the columns above stay "?" for the other candidates.

## 4. truck: the measured reality

This is the most important section in the document. truck is a genuine B-rep kernel, but the
**published crates are not sufficient for a CAD product's boolean operations**, and the measurements
below are why SolidCraft carries four patched vendored crates.

### 4.1 Coplanar, overlapping faces fail

Two 10 mm cubes, the second offset (5, 5, 0), so both span z 0…10:

```
A coplanar z 0..10      diff  and(a,¬b)      FAILED
A coplanar z 0..10      inter and(a,b)       FAILED
A coplanar z 0..10      union or(a,b)        FAILED
B piercing z -1..11     diff  and(a,¬b)      OK      shells 1
B piercing z -1..11     inter and(a,b)       OK      shells 1
B piercing z -1..11     union or(a,b)        OK      shells 1
C contained z 2..8      diff  and(a,¬b)      OK      shells 2
C contained z 2..8      inter and(a,b)       OK      shells 1
D partial z 8..12       diff  and(a,¬b)      OK      shells 1
D partial z 8..12       union or(a,b)        OK      shells 1
```

The failure is **not** scale, tolerance, or how the solid was built. It was reproduced with:

* cubes at 1 mm, 10 mm and 100 mm;
* tolerances from 1e-4 to 0.5 mm (and 5 % of the model size);
* solids built by `vertex → tsweep` (truck's documented cube) and by
  `wire → try_attach_plane → tsweep` (this adapter's and SolidCraft's construction);
* every combination of the two styles.

Only one thing separated success from failure: **whether the operands have coplanar, overlapping
faces.** Every CAD user cuts a hole exactly as deep as the plate; this is the common case, not an
edge case.

**Workaround implemented:** move *only the tool* by ~1e-4 mm in all three axes before the operation
and move the result back. Measured threshold: 1e-6 mm still fails, 1e-5 mm and above succeed. A z-only
nudge is not enough when the operands also share a y extent — which is why the offsets are
incommensurate per axis. Cost: the result's volume moves by ~0.004 mm³ on a 1120 mm³ part (benchmark),
and **100 of 100** unions needed this retry.

### 4.2 Difference: the defect was winding, and it is repaired

This section previously said difference "returns a solid that is not the difference" and was the reason the
adapter refused the capability. That conclusion was drawn from a volume measurement of **truck's raw
output**, and it was half right: the volume really was wrong. What the measurement did not establish is
*why*, and the reason turns out to be orientation, not geometry.

A 10×10×10 mm plate with a 4×4×10 mm tool removed. Analytic answer: **840 mm³**.

| Case | Raw truck output | After this adapter's repair |
|---|---|---|
| Piercing tool (no coincident faces) | 1053.333 mm³, 10 face patches | **840.000 mm³**, 22 400 mm² for a 100 mm plate |
| Coplanar tool (with nudge) | 1053.326 mm³ | 84 000.18 mm³ (membrane, §4.2.2) |
| Contained void (expected 904) | outer 1000 + inner **+**117.333 | **184 000.000 mm³** for the 100 mm fixture |

The piercing case makes the diagnosis visible: the result has exactly the **10 face patches a through-cut
should have**, and its volume is `840 + 2 × 106.667`. That 106.667 is precisely the contribution of the four
hole walls — counted with the wrong sign. The cut's *topology* was right all along; the walls of the cavity
were wound inward.

Two repairs are needed, and both are in the adapter:

1. **Local consistency.** Neighbouring triangles must traverse a shared edge in opposite directions. A
   breadth-first pass over shared edges already existed and already fixes the through-cut.
2. **Shell sign by nesting depth.** A cavity must be wound *inward*, so the enclosed volume is
   `outer − cavity`, not `outer + cavity`. A single whole-mesh flip cannot express that, because the outer
   shell is already correct. Each connected shell's nesting depth is measured by ray parity — a point taken
   from the shell, tested against every other shell — and shells at odd depth are flipped. Without this,
   the contained-void fixture measured 216 000 mm³ (200 000 + 16 000) and the conservation guard refused it.

With both repairs, both coplanar-free fixtures report a delta of **0.000e0** against analytic values, and
`BooleanDifference` is advertised.

#### 4.2.1 How the application builds a cut

`extrude` always starts at the z = 0 sketch plane, so a tool built for a body spanning z 0…h always shares a
plane with that body — every cut would be a coplanar cut. The contract therefore gained
`Capability::Transform` and `Kernel::translate`, and `cad-document` builds each cut tool with
`CUT_OVERSHOOT_MM` (0.01 mm) of overshoot on whichever faces would coincide, inside a *bounding box* check
so a pocket that ends inside the body is not extended at all. The extra material lies outside the body, so
it changes nothing, and the operation is ordinary: the end-to-end HTTP test measures 255.000 mm³ for a
through-cut, exactly.

#### 4.2.2 What is still not exact

A caller that hands `Kernel::boolean` two operands with a coincident face still gets the coplanar retry,
because truck returns no result without it. The retry translates the tool by ~1e-4 mm and translates the
result back, which is why a membrane one nudge thick remains on the shared face:

* volume: 84 000.18 mm³ against 84 000 analytic (2 ppm);
* area: 25 600 mm² against 22 400, because the hole is blind by 1.1e-4 mm and the membrane is real surface;
* watertightness: **8 hairline sliver edges** at one hole corner, so that mesh is not a closed manifold.

The fixture `difference_coplanar_cut` is recorded as a **known failure** with these numbers rather than
promoted, and the application avoids the case entirely by overshooting. Also guarded: a difference that
removes everything is refused with a message, because there is no mesh for "nothing" and the retry would
otherwise return a shell as thick as its own nudge (measured 0.008 mm³ for a plate cut by its own profile).

### 4.3 Chains fail

A union of boxes offset by half their width: the first union succeeds (1500.010 mm³ vs 1500), the
second **panics inside truck's geometry code**:

```
union 1 at x=5  -> ok, volume 1500.010 (expected 1500), retries so far 1
union 2 at x=10 -> FAILED: kernel internal error: union: non-string panic payload
                   (panic: tolerance must be no less than 1e-6, intersection_curve.rs:172)
```

The panic is caught and reported as `KernelError::Internal`, so the document is not corrupted — but
the operation is unavailable. Real modelling is a sequence of booleans, so this alone disqualifies a
naive "just use truck" integration.

### 4.4 Upstream non-determinism (from the vendored patch notes)

SolidCraft's `vendor/README.md` documents upstream defects its patches fix, including:

> *"the result's shells and faces keep the order of the shell they come from (truck's connected
> components followed hash order of face addresses, so a boolean's faces came out in a different
> order on every run)"*

and robustness fixes for empty/single-point polylines and collapsed/non-finite segments — i.e. the
published crate can panic or mis-normalise on degenerate intersections. This adapter uses the patched
crates (see the `[patch.crates-io]` block in the root `Cargo.toml`), which is why its union works at
all. **"We use truck" therefore means "we use truck plus SolidCraft's four patched crates."**

### 4.5 What this implies for the product

SolidCraft's `crates/kernel` is not incidental complexity around truck; it is the layer that makes
truck usable: retries with nudges and reseams, a mesh-based boolean path, history tracking,
orientation and volume guards, healing, and 76 reference parts checked against analytic/FreeCAD
oracle values (README claim: 75/76 match; **READ**, not re-run here).

Any plan to "integrate truck" from scratch would have to rediscover all of it. **Reuse the kernel
crate; do not rebuild it.**

## 5. Why truck is still the right choice

1. **Apache-2.0** — the only candidate whose licence is unconditionally safe for a closed-source
   product (see `docs/license-inventory.md`).
2. **Pure Rust, no native toolchain** — builds on this 7.4 GB machine in 8 min for one crate; no
   cmake, no OCCT download, no C++ ABI.
3. **Already integrated and hardened** — 94 803 LOC of adapter, 4 patched crates, an oracle, and 5 884
   tests behind it.
4. **Verified working subset in this milestone** — extrusion, union, intersection, tessellation, all
   with correct analytic volumes.
5. **WASM-plausible** — truck's own `truck-js` crates use `wasm-bindgen`; SolidCraft ships a web app
   (**READ**; no WASM build was performed here).

## 6. Why the others are not integrated

| Candidate | Why not | What would change it |
|---|---|---|
| **monstertruck** | A **fork of truck** (its README expands to "Multifarious Omnificence, Nomenclature Standardized, Terminology Enhanced & Refactored Truck"), 456 files vs truck's 274, same Apache-2.0. Two copies of one kernel is duplication with divergence risk, and truck is the one with a production integration and an oracle. | Demonstrated fixes for the defects in §4 that upstream/vendor lacks, with tests. |
| **cascade-rs** | Pure Rust, Apache-2.0, pre-1.0 ("targeting 80/20 feature parity with OpenCASCADE"); its own feature checklist was last validated 2026-02-19 and is auto-generated by its own code, so it is a self-report. Not built here: no measured advantage over an already-integrated kernel, and building it would cost hours on a machine where one crate takes 8 minutes. | A head-to-head on the same fixtures (booleans of coplanar/piercing/contained solids, failure behaviour) showing better robustness or features the product needs. |
| **hagane** | MIT/Apache, pure Rust, but self-described as *"an early, working foundation"*. Not built: same cost/benefit as above. | Maturity evidence: a boolean/feature test corpus, not a demo GIF. |
| **cadrum** | MIT wrapper over **statically linked OpenCASCADE (LGPL-2.1)**. The MIT licence covers cadrum's Rust source only; it cannot relicense OCCT. Also needs a native OCCT build/download. **Legal review required before any distribution.** | An explicit licence decision plus a dynamic-linking/packaging plan; or verification that its `pure` backend is not OCCT-dependent and is mature. |
| **opencascade-rs** | **LGPL-2.1** bindings. Static linking into a closed-source binary does not satisfy LGPL-2.1 §6 without relinkable form. Also cmake + full OCCT build. | An owner decision to ship LGPL with a compliant dynamic-linking strategy, and a build environment that can compile OCCT. |
| **references/** | cadrs, cadrust, bevy_pmetra are **read-only references** for ideas. `opencadstudio` arrived as `OpenCADStudio-main.zip` and its licence is **verified GPL-3.0** (1 797 files, 892 Rust); it stays unextracted and no code may enter the product. It is the most substantial reference application in the set and is worth reading for its command/UI architecture, not for code. | Product relicensed to GPL-3.0 (not planned). |

## 7. Selection

**Primary kernel for the integrated product: truck 0.6 through SolidCraft's `crates/kernel`, with
SolidCraft's vendored patches.** The shared contract talks to it through an adapter; the adapter in
this workspace proves the interface and documents the limits, and is not a replacement for it.

**Second kernel: none.** No candidate has demonstrated a capability the product needs that truck
lacks, and every second kernel multiplies the test surface.

**Revisit triggers** (any one of these reopens the comparison):

1. SolidCraft's kernel cannot represent a feature the product must ship (e.g. a class of blends).
2. A boolean/robustness corpus shows a competing pure-Rust kernel is materially more reliable.
3. Licensing changes (OCCT moving to a permissive licence is the only realistic path for OpenCASCADE).
4. WASM becomes a hard requirement and the truck-based kernel cannot meet the size/startup budget.

## 8. Honest summary of what is not known

* No candidate other than truck was built or measured on this machine. The comparison is therefore
  **evidence-based for truck and repository-based for the alternatives**, and says so.
* The 75/76 oracle figure is SolidCraft's claim, not a re-run.
* WASM viability is unverified for every candidate.
* Boolean chains are **unavailable** through the adapter (the third union panics inside truck). Difference
  is available and exact for a tool that overshoots the body; operands with coincident faces remain a
  recorded, measured limitation (§4.2.2), and the root cause is in the kernel layer, not the adapter.
