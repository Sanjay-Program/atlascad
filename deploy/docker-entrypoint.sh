#!/bin/sh
# Container entrypoint: create the document on first start, then serve it.
#
# The token is never a flag here. It arrives as ATLAS_STUDIO_TOKEN (read by the binary directly) or
# as ATLAS_STUDIO_TOKEN_FILE, which this script points the binary at. A container is exactly the
# place where a command line is logged and visible from outside, so neither form puts it in `ps`.
set -eu

project="${ATLAS_PROJECT:-/data/part.zcad}"
port="${ATLAS_PORT:-7519}"
host="${ATLAS_HOST:-0.0.0.0}"
advertise="${ATLAS_ADVERTISE:-localhost}"
name="${ATLAS_NAME:-part}"
size="${ATLAS_SIZE:-40,30,10}"

if [ ! -d "$(dirname "$project")" ]; then
    mkdir -p "$(dirname "$project")"
fi

if [ -n "${ATLAS_STUDIO_TOKEN_FILE:-}" ]; then
    token_args="--token-file ${ATLAS_STUDIO_TOKEN_FILE}"
else
    token_args=""
fi

if [ -f "$project" ]; then
    # shellcheck disable=SC2086
    exec /usr/local/bin/cad-studio \
        --file "$project" --share --host "$host" --advertise "$advertise" \
        --port "$port" $token_args "$@"
fi

# `--new` creates the document and serves it in the same process.
# shellcheck disable=SC2086
exec /usr/local/bin/cad-studio \
    --file "$project" --new --name "$name" --size "$size" \
    --share --host "$host" --advertise "$advertise" --port "$port" $token_args "$@"
