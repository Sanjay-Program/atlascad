# Big-assembly STEP corpus

Real-world CAD exports from <https://www.steptools.com/docs/stpfiles/bigassy/index.html>
(originals on GrabCAD; see that index for per-model attribution and licences).

**Why these exist.** Until 2026-07-28 the boolean corpus was ten small STEP
fixtures, and whole surface-type paths had *zero* coverage -- which reads as a
clean result but is really an absence of evidence. Two defect classes were closed
this month as "unreachable" purely because no fixture exercised them
(`SURFACE_OF_LINEAR_EXTRUSION`, and `SPHERICAL_SURFACE` on any boolean row). These
assemblies close that gap and are the corpus the kernel has to survive before any
production claim about arbitrary imported B-Reps is defensible.

**The files are NOT in this repository.** They total ~1 GB, and storing them
here -- even via git-lfs -- would put a permanent 1 GB in the remote's LFS store
that no branch deletion reclaims, against an org quota shared with everything
else. Only this README, `census.md` and `fetch.sh` are tracked.

Get them with:

```sh
./fixtures/step-bigassy/fetch.sh              # -> ~/code/step-corpus/bigassy
MONSTERTRUCK_STEP_CORPUS=/somewhere ./fixtures/step-bigassy/fetch.sh
```

The script is idempotent, skips files already present, and verifies every one
opens with the part-21 magic. Point tests at the directory via
`MONSTERTRUCK_STEP_CORPUS`; a corpus row must SKIP (not fail) when it is unset or
the file is absent, so a fresh clone stays green.

## What they cover

See `census.md` for the measured per-file entity counts. The headline is that
`Ai-14R.stp` alone carries 3,341 `SURFACE_OF_LINEAR_EXTRUSION` and 232
`SURFACE_OF_REVOLUTION`, both of which had no representative anywhere in the
previous corpus.

## Encoding warning -- read before adding a loader test

`Ai-14R.stp` is **ISO-8859 with CRLF**, not UTF-8: its `FILE_NAME` holds Cyrillic
text. The existing fixture path decodes with `String::from_utf8` and will reject
it. That is itself a real-world case worth having -- a STEP file a customer can
plausibly send that the loader cannot currently read -- so it is kept
deliberately rather than transcoded.

## Discipline

These are LARGE. Do not add them to the default `cargo test` gate. Reach them
through explicitly `#[ignore]`d rows or a dedicated corpus runner, so the
per-commit gate stays fast and these run on demand.
