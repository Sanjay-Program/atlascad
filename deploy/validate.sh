#!/usr/bin/env bash
# Validate the deployment assets without starting anything.
#
#   deploy/validate.sh        # shell syntax, compose interpolation, Caddyfile syntax
#
# This is the deployment half of the repository gate: it catches a broken script, a compose file
# that cannot be interpolated (the usual cause is a missing STUDIO_PUBLIC_HOST), or a Caddyfile that
# will not load, before any of them reach a host.
#
# Docker is optional. Without it the shell checks still run, compose interpolation is skipped with a
# clear message, and the run still succeeds: the gate must work on a machine that only compiles code.
# Caddyfile validation uses the `caddy` binary when present, else the cached caddy image, else it
# reports that it was skipped.
set -uo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

fail=0
note() { printf '    %s\n' "$1"; }
bad() {
    printf '    FAIL %s\n' "$1"
    fail=1
}

echo "==> shell syntax"
for script in deploy/*.sh scripts/*.sh; do
    if bash -n "$script" 2>/dev/null; then
        note "ok   $script"
    else
        bad "$script (bash -n)"
    fi
done

echo "==> compose interpolation"
if ! command -v docker >/dev/null 2>&1 || ! docker compose version >/dev/null 2>&1; then
    note "skipped: the docker compose plugin is not installed"
else
    token="$(mktemp)"
    printf '%s' "0123456789abcdef0123456789abcdef" >"$token"
    export STUDIO_PUBLIC_HOST="validate.invalid"
    export STUDIO_TOKEN_FILE="$token"
    for file in deploy/compose.yaml deploy/compose.smoke.yaml; do
        if docker compose -f "$file" config --quiet >/dev/null 2>&1; then
            note "ok   $file"
        else
            bad "$file (docker compose config)"
            docker compose -f "$file" config --quiet 2>&1 | sed 's/^/         /'
        fi
    done
    rm -f "$token"
    unset STUDIO_PUBLIC_HOST STUDIO_TOKEN_FILE
fi

echo "==> Caddyfile syntax"
caddy_output="$(mktemp)"
# 0 = valid, 1 = invalid (output in $caddy_output), 2 = no caddy available to check with.
caddy_validate() {
    local file="$1"
    if command -v caddy >/dev/null 2>&1; then
        STUDIO_PUBLIC_HOST=validate.invalid STUDIO_UPSTREAM=127.0.0.1:7519 \
            caddy validate --config "$file" --adapter caddyfile >"$caddy_output" 2>&1
        return $?
    fi
    if command -v docker >/dev/null 2>&1; then
        # Prefer the tag the stack uses; otherwise any cached caddy image can check the syntax.
        local image="caddy:${CADDY_IMAGE_TAG:-2}"
        if ! docker image inspect "$image" >/dev/null 2>&1; then
            image="$(docker image ls --format '{{.Repository}}:{{.Tag}}' | grep '^caddy:' | head -1)"
        fi
        if [ -n "$image" ] && docker image inspect "$image" >/dev/null 2>&1; then
            # --entrypoint explicitly: the official image's entrypoint is not applied when a
            # command is given on some docker versions, which made `validate` the executable.
            docker run --rm --entrypoint /usr/bin/caddy \
                -e STUDIO_PUBLIC_HOST=validate.invalid \
                -e STUDIO_UPSTREAM=127.0.0.1:7519 \
                -e STUDIO_LOG_FILE=/dev/stdout \
                -v "$root/$file:/etc/caddy/Caddyfile:ro,z" \
                "$image" validate --config /etc/caddy/Caddyfile --adapter caddyfile \
                >"$caddy_output" 2>&1
            return $?
        fi
    fi
    return 2
}
for file in deploy/Caddyfile deploy/Caddyfile.smoke; do
    caddy_validate "$file"
    status=$?
    case "$status" in
        0) note "ok   $file" ;;
        2) note "skipped $file (no caddy binary and no cached caddy image)" ;;
        *)
            bad "$file (caddy validate)"
            sed 's/^/         /' "$caddy_output"
            ;;
    esac
done
rm -f "$caddy_output"

if [ "$fail" -ne 0 ]; then
    echo "deployment assets are not valid" >&2
    exit 1
fi
echo "deployment assets are valid"
