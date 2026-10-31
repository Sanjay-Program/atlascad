#!/usr/bin/env bash
# Build the Atlas CAD Studio runtime image, and optionally prove it serves before you ship it.
#
#   deploy/build-image.sh                 # release binary + image
#   deploy/build-image.sh --smoke         # ... and start it, check it, tear it down
#   deploy/build-image.sh --tag 1.2.3     # a tag other than the crate version
#
# The image is deliberately not built by `docker build` alone: the workspace build needs the host's
# Rust toolchain and crate cache (see deploy/Dockerfile for why), so the binary is compiled here and
# copied in.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

version="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
tag="atlas-studio:${version}"
smoke=0
port="${SMOKE_PORT:-7599}"

while [ $# -gt 0 ]; do
    case "$1" in
        --smoke) smoke=1 ;;
        --tag)
            shift
            [ $# -gt 0 ] || { echo "build-image.sh: --tag needs a value" >&2; exit 2; }
            tag="atlas-studio:$1"
            ;;
        -h | --help)
            sed -n '2,10p' "$0" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        *)
            echo "build-image.sh: unknown argument $1" >&2
            exit 2
            ;;
    esac
    shift
done

echo "==> building the release binary"
cargo build --release --locked -p cad-studio

# The docker context is these two files and nothing else. Handing docker the repository root would
# send it `target/`, `.git/` and every saved document on this machine.
context="$(mktemp -d)"
smoke_cleanup() { rm -rf "$context"; }
trap smoke_cleanup EXIT
cp target/release/cad-studio "$context/cad-studio"
cp deploy/docker-entrypoint.sh "$context/docker-entrypoint.sh"

echo "==> building $tag"
docker build -f deploy/Dockerfile -t "$tag" "$context"

if [ "$smoke" -eq 0 ]; then
    echo "ok: $tag"
    echo "    run: docker run --rm -p 7519:7519 -e ATLAS_STUDIO_TOKEN=\"\$(openssl rand -hex 16)\" $tag"
    exit 0
fi

name="atlas-smoke-$$"
token="$(head -c 32 /dev/urandom | od -An -tx1 | tr -d ' \n')"
container_cleanup() {
    docker rm -f "$name" >/dev/null 2>&1 || true
}
trap 'container_cleanup; smoke_cleanup' EXIT

echo "==> starting $name on :$port"
docker run -d --name "$name" \
    -p "127.0.0.1:$port:7519" \
    -e ATLAS_STUDIO_TOKEN="$token" \
    -e ATLAS_NAME=smoke \
    "$tag" >/dev/null

# Wait for the container's own healthcheck rather than sleeping a fixed time: that is the same
# signal an orchestrator would use, so a smoke test that passes here means the deployment contract
# holds.
echo "==> waiting for health"
status="starting"
for _ in $(seq 1 40); do
    status="$(docker inspect --format '{{if .State.Health}}{{.State.Health.Status}}{{else}}none{{end}}' "$name")"
    [ "$status" = "healthy" ] && break
    [ "$status" = "unhealthy" ] && break
    sleep 2
done
if [ "$status" != "healthy" ]; then
    echo "smoke failed: container health is '$status'" >&2
    docker logs "$name" >&2 || true
    exit 1
fi

base="http://127.0.0.1:$port"
echo "==> GET $base/api/health (no token)"
curl -fsS "$base/api/health"
echo

echo "==> GET $base/api/state (no token, must be 401)"
code="$(curl -s -o /dev/null -w '%{http_code}' "$base/api/state")"
[ "$code" = "401" ] || { echo "smoke failed: /api/state returned $code without a token" >&2; exit 1; }

echo "==> GET $base/api/state?token=... (must be 200)"
curl -fsS "$base/api/state?token=$token" | head -c 200
echo

echo "==> GET $base/api/export/pdf?token=..."
curl -fsS "$base/api/export/pdf?token=$token" -o /tmp/atlas-smoke.pdf
head -c 8 /tmp/atlas-smoke.pdf | grep -q '%PDF-1.4' || { echo "smoke failed: PDF not produced" >&2; exit 1; }
echo "ok: $(wc -c </tmp/atlas-smoke.pdf) byte PDF"

echo "==> document survived the request (file inside the volume)"
docker exec "$name" ls -l /data >&2

echo "ok: $tag served the document, refused an anonymous read, and plotted a sheet"
