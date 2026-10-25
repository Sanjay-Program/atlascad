#!/usr/bin/env bash
# Build the serverless web app: wasm kernel (Docker, wasi-sdk toolchain from cadrum) plus a
# static copy of the add-ins. Serve the web/ folder with any static file server afterwards.
set -euo pipefail
cd "$(dirname "$0")/.."
docker run --rm -v "$PWD":/work -w /work ghcr.io/lzpel/cross-wasm32-unknown-unknown \
  cargo build --release -p cadrust-wasm --target-dir /work/target/wasm
cp target/wasm/wasm32-unknown-unknown/release/cadrust_wasm.wasm web/cadrust_wasm.wasm
rm -rf web/addins web/examples && cp -r addins/dist web/addins && cp -r examples web/examples
# Static add-in index with the same shape as GET /api/addins.
python3 - <<'PY'
import json, pathlib
out = []
for p in sorted(pathlib.Path('web/addins').iterdir()):
    if p.suffix == '.wasm':
        out.append({"kind": "wasm", "url": f"addins/{p.name}", "manifest": None})
    elif (p / 'manifest.json').exists():
        m = json.loads((p / 'manifest.json').read_text())
        out.append({"kind": "js", "url": f"addins/{p.name}/{m.get('entry', 'main.js')}", "manifest": m})
pathlib.Path('web/addins.json').write_text(json.dumps(out, indent=2))
# Static design index with the same shape as GET /api/examples.
ex = [{"name": p.name[:-len('.cadrust.json')], "file": p.name} for p in sorted(pathlib.Path('web/examples').glob('*.cadrust.json'))]
pathlib.Path('web/examples.json').write_text(json.dumps(ex, indent=2))
PY
ls -la web/cadrust_wasm.wasm web/addins.json
