# Entity census -- big-assembly STEP corpus

> The files themselves live OUTSIDE this repository; run `./fetch.sh` to obtain
> them. This census is tracked so the coverage argument survives without the
> gigabyte.

Measured 2026-07-28 with `grep -aco <ENTITY>` over each file.

**Use `-a`.** `Ai-14R.stp` is ISO-8859, so plain `grep -c` treats it as binary
and silently reports **0 for every entity**. That very nearly recorded the one
file that closes our two empty entity classes as containing nothing.

## Counts

| file | size | EXTRUDE | REVOLVE | SPHERE | TORUS | CONE | CYLINDER | BSPLINE | FACES |
|---|---|---|---|---|---|---|---|---|---|
| `Ai-14R.stp` | 100M | **3341** | **232** | 103 | 757 | 1865 | 5630 | 1612 | 18066 |
| `Cruise_Assembly.stp` | 40M | 0 | 0 | 4 | 204 | 116 | 4092 | 706 | 22604 |
| `NissanGT-R.STEP` | 167M | 0 | 0 | 88 | 2047 | 1252 | 8953 | 2966 | 41123 |
| `Rocky_House.stp` | 18M | 0 | 0 | 25 | 286 | 101 | 1251 | 737 | 7721 |
| `ROTOR-201NAL-Z7.STEP` | 6.7M | 0 | 0 | 32 | 57 | 440 | 1029 | 62 | 2319 |
| `Scania-8x4.stp` | 282M | 0 | 0 | 842 | 7620 | 2262 | 20986 | 8939 | 56899 |
| `Scania-Engine-V8-XT-Turbo.step` | 358M | 0 | 0 | 544 | 5259 | 2676 | 11550 | 8071 | 39159 |
| `UMC-500_SS_...r1.stp` | 59M | 0 | 0 | 45 | 292 | 1283 | 5268 | 778 | 17960 |
| **total** | **~1030M** | **3341** | **232** | **1683** | **16522** | **9995** | **58759** | **23871** | **205851** |

(EXTRUDE = `SURFACE_OF_LINEAR_EXTRUSION`, REVOLVE = `SURFACE_OF_REVOLUTION`,
BSPLINE = `B_SPLINE_SURFACE_WITH_KNOTS`, FACES = `ADVANCED_FACE`.)

## What changes versus the previous corpus

The prior corpus was 10 small fixtures totalling **45 cylinders, 14 cones, 14
tori, 8 spheres, 2 B-splines, and ZERO extrusions or revolutions**.

| entity | before | now | note |
|---|---|---|---|
| `SURFACE_OF_LINEAR_EXTRUSION` | **0** | **3341** | was closed as "unreachable, a fix would be unverifiable" |
| `SURFACE_OF_REVOLUTION` | **0** | **232** | no representative existed anywhere |
| `SPHERICAL_SURFACE` | 8, none on a boolean row | **1683** | was closed as unreachable for the same reason |
| `TOROIDAL_SURFACE` | 14 | **16522** | the only class with a KNOWN silent-wrong (7cc) |
| `ADVANCED_FACE` | ~1200 | **205851** | boxy, the reference fixture, has 80 |

**Two conclusions from this table were invalidated the moment these landed.**
`ExtrusionSurface` (`geom_impls.rs:421-436`) carries the same unit-stub
truncation pattern T22 fixed for cylinders and was left alone only because
nothing exercised it. Spheres were closed the same way. Both are now testable.

## Schemas and encodings

| file | encoding | schema |
|---|---|---|
| `Ai-14R.stp` | **ISO-8859, CRLF** | `CONFIG_CONTROL_DESIGN` (AP203) |
| `Cruise_Assembly.stp` | ASCII, CRLF | `AUTOMOTIVE_DESIGN {1 0 10303 214 3 1 1}` |
| `NissanGT-R.STEP` | ASCII, CRLF | `AUTOMOTIVE_DESIGN` |
| `Rocky_House.stp` | ASCII, CRLF | `AUTOMOTIVE_DESIGN {1 0 10303 214 3 1 1}` |
| `ROTOR-201NAL-Z7.STEP` | ASCII, **very long lines** | `AUTOMOTIVE_DESIGN` |
| `Scania-8x4.stp` | ASCII, CRLF | `CONFIG_CONTROL_DESIGN` |
| `Scania-Engine-V8-XT-Turbo.step` | ASCII, CRLF | `CONFIG_CONTROL_DESIGN` |
| `UMC-500_SS_...r1.stp` | ASCII, CRLF | `AUTOMOTIVE_DESIGN` |

Both AP203 (`CONFIG_CONTROL_DESIGN`) and AP214 (`AUTOMOTIVE_DESIGN`) are
represented, in two spellings of the AP214 schema string.

## Known loader gaps, found before any test ran

1. **`Ai-14R.stp` will be REJECTED.** It is ISO-8859 and the fixture path
   decodes with `String::from_utf8`. Kept un-transcoded on purpose: a customer
   can send exactly this, so "the loader cannot read it" is the finding. Needs a
   decision -- decode properly, or refuse in a typed way -- not a quiet fix.
2. **`ROTOR-201NAL-Z7.STEP` has very long lines**, which is a different parser
   stress than the rest and worth its own row.

## Discipline

~1 GB. **Do NOT put these in the default `cargo test` gate.** Reach them via
explicitly `#[ignore]`d rows or a dedicated corpus runner. The per-commit gate
must stay fast; a slow gate stops being run, and an unrun gate is worse than no
gate.

Expect these to be SLOW even on demand: the reference fixture boxy has 80 faces
and its union took 8 s after this month's fixes. `Scania-8x4.stp` has 56,899.
Start with load-and-heal coverage before attempting booleans on the largest.
