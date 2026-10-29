#!/usr/bin/env bash
# Live simulation of everything that ships.
#
#   ./scripts/simulate.sh          # HTTP workflow + headless-browser smoke check
#   ./scripts/simulate.sh --http   # HTTP only (no Node, no Chrome)
#
# Builds the release binaries if they are missing, then:
#   1. drives the real server through a cut, a join, undo/redo, a parameter edit, save, export,
#      restart and a kill -9 recovery (scripts/simulate.py, 31 analytic assertions);
#   2. loads the live page in headless Chrome and checks the tree, the volume, the badges and the
#      console (scripts/browser-check.mjs), skipping cleanly when no browser exists.
set -euo pipefail
cd "$(dirname "$0")/.."

HTTP_ONLY=0
if [ "${1:-}" = "--http" ]; then
  HTTP_ONLY=1
fi

if [ ! -x target/release/cad-studio ]; then
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

cleanup() {
  if [ -n "$SERVER_PID" ] && kill -0 "$SERVER_PID" 2>/dev/null; then
    kill "$SERVER_PID" 2>/dev/null || true
    wait "$SERVER_PID" 2>/dev/null || true
  fi
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
printf '\n==> live simulation passed\n'
