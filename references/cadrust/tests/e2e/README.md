# End-to-end tests

Browser tests drive the web app with Playwright (Python) in headless Chromium and check the
kernel's answers through the page's `window.cadrust` debug hooks. Every suite prints one
`PASS`/`FAIL` line per check and `FAILS [...]` at the end.

```bash
pip install playwright && playwright install chromium
cargo build --release
./target/release/cadrust serve examples/bracket.cadrust.json --port 8080 &      # server mode
python3 -m http.server 8090 --directory web &                                  # serverless mode (after scripts/build-web.sh)

python3 tests/e2e/p4.py                              # against http://127.0.0.1:8080/
python3 tests/e2e/p4.py http://127.0.0.1:8090/index.html
```

| suite | covers |
|---|---|
| e2e, p3con, p3face, p4, p5 | viewer, sketch constraints, sketch on a face, dialogs and navigation, add-ins |
| p6 | Fusion-style chrome: tabs, group menus, browser, timeline marker, data panel, ViewCube |
| p7 | extrude options, primitives with booleans, scale, split, offset face / press pull, delete, STEP import, projection |
| p8 | dependency graph and incremental evaluation (cache reuse, partial recompute, undo) |
| p9, p10 | themed scrollbars and splitters; responsive ribbon folding |
| p11 | Save As / Export dialog, server folder browsing, Data Panel folders |
| cyl | the pneumatic cylinder scenario: constrained sketches, STEP after every stage, undo / redo, constraint edits |
| cyl_video | the same scenario recorded at human pace with captions (writes doc/cylinder_modeling.mp4, needs ffmpeg) |

Screenshots used by the README are written to `doc/` by the suites; `shot.py` takes ad-hoc screenshots.
