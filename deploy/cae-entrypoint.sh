#!/bin/sh
# Run one linear static study and exit with its status.
#
# The inputs are read-only by design and the result directory is the only writable path. `exec`
# makes the container's exit code atlas's, so 0/1/2/3 mean what
# docs/operations/cae-deployment.md says: success, internal or I/O fault, bad job input, or an
# analysis refusal.
#
#   ATLAS_PROJECT   project file, default /work/part.zcad
#   ATLAS_STUDY     versioned study file, default /work/study.json
#   ATLAS_RESULT    result file, default /results/result.json
#   ATLAS_RECORD    optional artifact id: record a simulation at the project revision
#   ATLAS_LABEL     label for that record
set -eu

project="${ATLAS_PROJECT:-/work/part.zcad}"
study="${ATLAS_STUDY:-/work/study.json}"
result="${ATLAS_RESULT:-/results/result.json}"

refuse() {
    printf 'atlas-cae: %s\n' "$2" >&2
    exit "$1"
}

[ -r "$project" ] || refuse 2 "cannot read the project $project; mount it there"
[ -r "$study" ] || refuse 2 "cannot read the study $study; mount it there"

result_dir="$(dirname "$result")"
[ -d "$result_dir" ] || refuse 2 "the result directory $result_dir does not exist; mount a volume there"
[ -w "$result_dir" ] || refuse 2 "the result directory $result_dir is not writable"

set -- cae static --file "$project" --study "$study" --out "$result"
if [ -n "${ATLAS_RECORD:-}" ]; then
    # Recording writes the project, so the project volume must be writable in this mode.
    set -- "$@" --record "$ATLAS_RECORD"
    if [ -n "${ATLAS_LABEL:-}" ]; then
        set -- "$@" --label "$ATLAS_LABEL"
    fi
fi

exec /usr/local/bin/atlas "$@"
