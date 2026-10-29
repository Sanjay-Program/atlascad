#!/usr/bin/env bash
# Live simulation of everything that ships.
#
#   ./scripts/simulate.sh          # HTTP workflow + headless-browser smoke check
#   ./scripts/simulate.sh --http   # HTTP only (no Node, no Chrome)
#
# Builds the release binaries if they are missing, then:
#   1. drives the real server through a cut, a join, undo/redo, a parameter edit, save, export,
#      restart and a kill -9 recovery (31 analytic assertions);
#   2. drives the shipped `atlas` and `atlas-eng` binaries on real files — box/union STL volumes,
#      a headless project edit/undo, manifest validation, the licence policy and a deterministic
#      SBOM (21 assertions);
#   3. starts a shared studio and checks the token boundary over HTTP, including a token-bearing
#      edit (4 assertions);
#   4. loads the live page in headless Chrome, loopback and shared, and checks the tree, the volume,
#      the badges, the share panel and the console (scripts/browser-check.mjs), skipping cleanly
#      when no browser exists.
set -euo pipefail
cd "$(dirname "$0")/.."

HTTP_ONLY=0
if [ "${1:-}" = "--http" ]; then
  HTTP_ONLY=1
fi

if [ ! -x target/release/cad-studio ] || [ ! -x target/release/atlas ] || \
   [ ! -x target/release/atlas-eng ]; then
  printf '==> building release binaries\n'
  cargo build --offline --release -p cad-studio -p cad-cli -p cad-engineering -j "${JOBS:-2}"
fi

python3 scripts/simulate.py

if [ "$HTTP_ONLY" -eq 1 ]; then
  printf '\n==> browser check skipped (--http)\n'
  exit 0
fi

if ! command -v node >/dev/null 2>&1; then
  printf '\n==> browser check skipped (node is not installed)\n'
  exit 0
fi

PORT="$(python3 - <<'PY'
import socket
with socket.socket() as sock:
    sock.bind(("127.0.0.1", 0))
    print(sock.getsockname()[1])
PY
)"
WORK="$(mktemp -d)"
SERVER_PID=""
SHARED_PID=""

cleanup() {
  for pid in "$SERVER_PID" "$SHARED_PID"; do
    if [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null; then
      kill "$pid" 2>/dev/null || true
      wait "$pid" 2>/dev/null || true
    fi
  done
  rm -rf "$WORK"
}
trap cleanup EXIT

target/release/cad-studio --new "$WORK/browser.zcad" --name browser --size 10,10,5 --port "$PORT" \
  >"$WORK/server.log" 2>&1 &
SERVER_PID=$!

for _ in $(seq 1 200); do
  if curl -s -o /dev/null "http://127.0.0.1:$PORT/api/health"; then
    break
  fi
  sleep 0.05
done
curl -s -o /dev/null "http://127.0.0.1:$PORT/api/health" || {
  echo "the studio did not start:" >&2
  cat "$WORK/server.log" >&2
  exit 1
}

node scripts/browser-check.mjs "http://127.0.0.1:$PORT"

# The shared page is a different security path: it loads without a token in the URL, then presents
# one on every API call. It gets its own browser run rather than being assumed to work.
TOKEN="browsersharetoken0123456789abcdef"
SHARED_PORT="$(python3 - <<'PY'
import socket
with socket.socket() as sock:
    sock.bind(("127.0.0.1", 0))
    print(sock.getsockname()[1])
PY
)"
target/release/cad-studio --new "$WORK/shared.zcad" --name browser --size 10,10,5 --share \
  --advertise 127.0.0.1 --token "$TOKEN" --port "$SHARED_PORT" >"$WORK/shared.log" 2>&1 &
SHARED_PID=$!

for _ in $(seq 1 200); do
  if curl -s -o /dev/null -H "x-atlas-token: $TOKEN" \
    "http://127.0.0.1:$SHARED_PORT/api/health"; then
    break
  fi
  sleep 0.05
done
curl -s -o /dev/null -H "x-atlas-token: $TOKEN" \
  "http://127.0.0.1:$SHARED_PORT/api/health" || {
  echo "the shared studio did not start:" >&2
  cat "$WORK/shared.log" >&2
  exit 1
}

node scripts/browser-check.mjs "http://127.0.0.1:$SHARED_PORT/?token=$TOKEN"
printf '\n==> live simulation passed\n'
