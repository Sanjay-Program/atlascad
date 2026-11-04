#!/bin/sh
# Run one CAM job and exit with its status.
#
# The job file and the part are read-only by design; the output directory is the only writable path.
# `exec` makes the container's exit code atlas's, so 0/1/2/3 mean what
# docs/operations/cam-deployment.md says: posted, internal or I/O fault, bad job input, or the CAM
# checks refused the job.
#
#   ATLAS_JOB      versioned job file, default /work/job.json
#   ATLAS_OUT      output directory, default /out
#   ATLAS_PROJECT  optional read-only design to pin the program to; a copy is recorded in $ATLAS_OUT
#   ATLAS_RECORD   artifact id to record, required together with ATLAS_PROJECT
set -eu

job="${ATLAS_JOB:-/work/job.json}"
out="${ATLAS_OUT:-/out}"
project="${ATLAS_PROJECT:-}"
record="${ATLAS_RECORD:-}"

refuse() {
    printf 'atlas-cam: %s\n' "$2" >&2
    exit "$1"
}

[ -r "$job" ] || refuse 2 "cannot read the job $job; mount it there"
[ -d "$out" ] || refuse 2 "the output directory $out does not exist; mount a volume there"
[ -w "$out" ] || refuse 2 "the output directory $out is not writable"

# Provenance is opt-in and all-or-nothing: a design revision plus an artifact id, or neither. The
# input stays read-only; the project is copied into the writable output so the recorded revision and
# its toolpath artifact travel with the program.
if [ -n "$project" ] || [ -n "$record" ]; then
    [ -n "$project" ] || refuse 2 "ATLAS_RECORD needs ATLAS_PROJECT, the design to pin the program to"
    [ -n "$record" ] || refuse 2 "ATLAS_PROJECT needs ATLAS_RECORD, the artifact id to record"
    [ -r "$project" ] || refuse 2 "cannot read the project $project; mount it read-only"
    cp "$project" "$out/project.zcad" || refuse 2 "cannot copy the project into $out"
    exec /usr/local/bin/atlas cam job --job "$job" --out-dir "$out" \
        --project "$out/project.zcad" --record "$record"
fi

exec /usr/local/bin/atlas cam job --job "$job" --out-dir "$out"
