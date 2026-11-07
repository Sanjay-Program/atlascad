#!/usr/bin/env bash
# Start (or restart) the hosted Atlas CAD Studio stack, after checking what a deployment needs.
#
#   cd deploy
#   STUDIO_PUBLIC_HOST=cad.example.com ./up.sh     # start, wait for health, print the URL
#   ./up.sh                                        # same, reading deploy/.env
#
# It refuses to start rather than starting something broken. Before touching compose it checks:
#   - docker and the compose plugin are installed,
#   - compose can interpolate the stack (a missing STUDIO_PUBLIC_HOST is the usual cause),
#   - the token file exists, is a plausible secret (16–128 characters, no whitespace) and is 0600,
#   - the studio image is present (it is built on the host by deploy/build-image.sh).
#
# The token is a compose secret mounted at /run/secrets/atlas-studio-token; it is never exported
# into the container's environment and never appears in a command line.
#
# Exit status: 0 when the stack is healthy, 1 when a check or the start failed, 2 on a usage error.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"

case "${1:-}" in
    -h | --help)
        sed -n '2,18p' "$0" | sed 's/^# \{0,1\}//'
        exit 0
        ;;
    "") ;;
    *)
        echo "up.sh: unknown argument $1" >&2
        exit 2
        ;;
esac

compose_args=()
if [ -f .env ]; then
    compose_args=(--env-file .env)
fi
compose() { docker compose "${compose_args[@]}" -f compose.yaml "$@"; }

if ! command -v docker >/dev/null 2>&1; then
    echo "up.sh: docker is not installed" >&2
    exit 2
fi
if ! docker compose version >/dev/null 2>&1; then
    echo "up.sh: the docker compose plugin is not installed" >&2
    exit 2
fi

errors="$(mktemp)"
trap 'rm -f "$errors"' EXIT

if ! resolved="$(compose config --format json 2>"$errors")"; then
    cat "$errors" >&2
    echo >&2
    echo "up.sh: compose could not resolve the stack. Set the variables it names, for example:" >&2
    echo "         STUDIO_PUBLIC_HOST=cad.example.com ./up.sh" >&2
    echo "       (or copy hosted-env.example to deploy/.env and edit it)" >&2
    exit 1
fi

if ! values="$(printf '%s' "$resolved" | python3 -c '
import json, sys
config = json.load(sys.stdin)
volumes = config["services"]["atlas-token"]["volumes"]
source = next((v["source"] for v in volumes if v.get("target") == "/source/token"), "")
print(
    config["services"]["atlas-proxy"]["environment"]["STUDIO_PUBLIC_HOST"],
    config["services"]["atlas-studio"]["image"],
    source,
)
')"; then
    echo "up.sh: could not read the resolved compose configuration" >&2
    exit 1
fi
read -r public_host image token_file <<<"$values"
# Compose resolves the bind source to an absolute path; fall back to the variable/default if the
# layout changes so the token checks still run against what compose will actually mount.
token_file="${token_file:-${STUDIO_TOKEN_FILE:-./studio-token.txt}}"

if [ ! -f "$token_file" ]; then
    echo "up.sh: the token file $token_file does not exist; run ./gen-token.sh first" >&2
    exit 1
fi
case "$(stat -c %a "$token_file" 2>/dev/null || echo "unknown")" in
    600 | 400) ;;
    *) echo "up.sh: warning: $token_file is not 0600; chmod 600 it" >&2 ;;
esac
length="$(wc -c <"$token_file" | tr -d ' ')"
token="$(tr -d '\n' <"$token_file")"
case "$length" in
    '' | *[!0-9]*) echo "up.sh: could not read the length of $token_file" >&2; exit 1 ;;
esac
if [ "$length" -lt 16 ] || [ "$length" -gt 128 ]; then
    echo "up.sh: the token in $token_file must be 16–128 characters (it is $length); run ./gen-token.sh" >&2
    exit 1
fi
if [ "$(printf '%s' "$token" | wc -l)" -ne 0 ] || printf '%s' "$token" | grep -q '[[:space:][:cntrl:]]'; then
    echo "up.sh: the token in $token_file contains whitespace or control characters" >&2
    exit 1
fi

if ! docker image inspect "$image" >/dev/null 2>&1; then
    echo "up.sh: the studio image $image is not present; build it with ./build-image.sh" >&2
    exit 1
fi

echo "==> starting the hosted stack for https://$public_host"
# --force-recreate so a rotated token file is staged again: the one-shot `atlas-token` service
# only runs on (re)create, and the studio reads its token once at startup.
compose up -d --wait --force-recreate

echo
echo "ok: the stack is healthy"
printf '    url         https://%s\n' "$public_host"
printf '    token file  %s (mounted as /run/secrets/atlas-studio-token)\n' "$token_file"
printf '    document    /data/part.zcad in volume %s_atlas-studio-data\n' "${COMPOSE_PROJECT_NAME:-atlas}"
echo "    logs        docker compose -f compose.yaml logs -f"
echo "    backup      ./backup.sh"
echo "    stop        docker compose -f compose.yaml down"
