#!/usr/bin/env bash
# One-command end-to-end smoke of the hosted stack.
#
#   deploy/smoke.sh                 # build if needed, start, verify, drive a browser, tear down
#   deploy/smoke.sh --advanced      # ... and run the full interaction driver (drags, shortcuts,
#                                   #     every panel, parsed downloads; ~1 minute more)
#   deploy/smoke.sh --keep          # leave the stack running and print its URL
#
# What it proves, in order:
#   1. the image exists (built with deploy/build-image.sh when missing),
#   2. a *brand-new* document is created by the container on a fresh volume — this is the
#      "create a new CAD" path, not a re-open of an existing file,
#   3. the proxy answers liveness over plain HTTP,
#   4. the proxy completes a TLS handshake and answers liveness over HTTPS (local-CA cert),
#   5. the document is refused without a token and served with one, through the proxy,
#   6. headless Chrome loads the real page through the proxy, builds and edits a model, saves,
#      and downloads STL, DXF and PDF (scripts/browser-check.mjs, 31 assertions),
#   7. teardown removes the containers and the smoke volume.
#
# It uses host ports 8080/8443/7599 and a compose project name of its own, so it can run while a
# real deployment keeps 80/443. Every step is a real request against the deployed shape; nothing
# here inspects expectations from the source.
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")"

KEEP=0
ADVANCED=0
case "${1:-}" in
    --keep) KEEP=1 ;;
    --advanced) ADVANCED=1 ;;
    -h | --help)
        sed -n '2,22p' "$0" | sed 's/^# \{0,1\}//'
        exit 0
        ;;
    "") ;;
    *)
        echo "smoke.sh: unknown argument $1" >&2
        exit 2
        ;;
esac

PROJECT="${SMOKE_PROJECT:-atlas-smoke}"
HTTP_PORT="${STUDIO_HTTP_PORT:-8080}"
HTTPS_PORT="${STUDIO_HTTPS_PORT:-8443}"
BIND_PORT="${STUDIO_BIND_PORT:-7599}"
IMAGE_TAG="${STUDIO_IMAGE_TAG:-0.1.0}"
# The smoke always uses the file the production stack mounts as its secret. The leading ./ matters:
# compose treats a bare name as a named volume, not a bind mount.
TOKEN_FILE="./studio-token.txt"
IMAGE="atlas-studio:${IMAGE_TAG}"

step() { printf '\n==> %s\n' "$1"; }
# Every compose call goes through this: the smoke file is interpolated from the same variables the
# production stack needs, so a missing one fails the parse rather than being silently omitted.
smoke_compose() {
    STUDIO_PUBLIC_HOST=localhost \
        STUDIO_TOKEN_FILE="$TOKEN_FILE" \
        STUDIO_IMAGE_TAG="$IMAGE_TAG" \
        docker compose -p "$PROJECT" -f compose.smoke.yaml "$@"
}
fail() {
    echo "SMOKE FAILED: $1" >&2
    smoke_compose logs --tail 40 >&2 || true
    exit 1
}

if ! command -v docker >/dev/null 2>&1; then
    echo "smoke.sh: docker is not installed" >&2
    exit 2
fi

if ! docker image inspect "$IMAGE" >/dev/null 2>&1; then
    step "image $IMAGE is missing; building it (cargo build --release -p cad-studio)"
    ./build-image.sh || fail "could not build $IMAGE"
fi

step "generating a throwaway token"
./gen-token.sh >/dev/null
chmod 0600 "$TOKEN_FILE"
TOKEN="$(cat "$TOKEN_FILE")"
[ "${#TOKEN}" -ge 16 ] || fail "the generated token is too short"

cleanup() {
    if [ "$KEEP" -eq 1 ]; then
        printf '\n==> left running: http://localhost:%s/?token=<in %s>\n' "$HTTP_PORT" "$TOKEN_FILE"
        return
    fi
    step "tearing down"
    smoke_compose down -v --remove-orphans >/dev/null 2>&1 || true
}
trap cleanup EXIT

step "removing any previous smoke stack and its volume"
smoke_compose down -v --remove-orphans >/dev/null 2>&1 || true

step "starting a fresh stack (new document, 10x10x5) on :$HTTP_PORT/:$HTTPS_PORT"
STUDIO_HTTP_PORT="$HTTP_PORT" \
    STUDIO_HTTPS_PORT="$HTTPS_PORT" \
    STUDIO_BIND_PORT="$BIND_PORT" \
    STUDIO_SIZE="10,10,5" \
    STUDIO_NAME="smoke" \
    smoke_compose up -d --wait \
    || fail "compose did not reach a healthy state"

BASE="http://localhost:$HTTP_PORT"
TLS="https://localhost:$HTTPS_PORT"

step "liveness over plain HTTP"
curl -fsS --max-time 10 "$BASE/api/health" || fail "$BASE/api/health did not answer"
echo

step "TLS handshake + liveness over HTTPS (local CA)"
curl -fsSk --max-time 10 "$TLS/api/health" || fail "$TLS/api/health did not answer over TLS"
echo

step "the document is refused without a token through the proxy"
code="$(curl -s -o /dev/null -w '%{http_code}' --max-time 10 "$BASE/api/state")"
[ "$code" = "401" ] || fail "/api/state returned $code without a token (expected 401)"

step "the token is a file the studio's uid reads, not a container environment variable"
studio_id="$(smoke_compose ps -q atlas-studio)"
[ -n "$studio_id" ] || fail "the studio container was not found"
if docker inspect --format '{{range .Config.Env}}{{println .}}{{end}}' "$studio_id" | grep -qF "$TOKEN"; then
    fail "the token appears in the container environment (docker inspect shows it)"
fi
docker exec --user 10001:10001 "$studio_id" test -r /run/secrets/atlas-studio-token \
    || fail "uid 10001 cannot read /run/secrets/atlas-studio-token"
echo "    not in Config.Env; readable by uid 10001 at /run/secrets/atlas-studio-token"

step "the proxy sends HSTS and refuses to cache the document over TLS"
headers="$(curl -sk -D - -o /dev/null --max-time 10 -H "Host: localhost" "$TLS/api/state")"
printf '%s' "$headers" | grep -qi '^strict-transport-security:' \
    || fail "no Strict-Transport-Security header over TLS"
printf '%s' "$headers" | grep -qi '^cache-control:.*no-store' \
    || fail "no Cache-Control: no-store header through the proxy"

step "the brand-new document is served with a token through the proxy"
state="$(curl -fsS --max-time 10 "$BASE/api/state?token=$TOKEN")" || fail "/api/state failed with a token"
python3 - "$state" <<'PY' || fail "the served state is not the freshly created document"
import json, sys
state = json.loads(sys.argv[1])
assert state.get("name") == "smoke", state.get("name")
assert state.get("features"), "the new document has no features"
assert state.get("dirty") is False, state.get("dirty")
assert not state.get("failure"), state.get("failure")
volume = state.get("geometry", {}).get("volume_mm3")
assert volume is not None and volume > 0, volume
print(f"    document '{state['name']}' created with {len(state['features'])} feature(s), volume {volume} mm^3")
PY

if command -v node >/dev/null 2>&1; then
    step "headless browser through the proxy: load, edit, save, export STL/DXF/PDF"
    node ../scripts/browser-check.mjs "$BASE/?token=$TOKEN" \
        || fail "the browser check failed against the deployed stack"
else
    if [ "$ADVANCED" -eq 1 ]; then
        fail "--advanced needs node, and node is not installed"
    fi
    step "browser check skipped (node is not installed)"
fi

if [ "$ADVANCED" -eq 1 ]; then
    # The driver asserts analytic volumes from the starting rectangle, so it needs a document of its
    # own; the standard check above has already edited this one.
    step "restarting on a fresh document for the advanced driver"
    smoke_compose down -v --remove-orphans >/dev/null 2>&1 || true
    STUDIO_HTTP_PORT="$HTTP_PORT" \
        STUDIO_HTTPS_PORT="$HTTPS_PORT" \
        STUDIO_BIND_PORT="$BIND_PORT" \
        STUDIO_SIZE="10,10,5" \
        STUDIO_NAME="drive" \
        smoke_compose up -d --wait \
        || fail "the fresh stack for the advanced driver did not become healthy"

    step "advanced interaction driver: real drags, wheel zoom, shortcuts, every panel"
    node ../scripts/browser-drive.mjs "$BASE/?token=$TOKEN" --artifacts ../dist/browser-drive \
        || fail "the advanced browser drive failed against the deployed stack"
fi

step "stack smoke passed"
