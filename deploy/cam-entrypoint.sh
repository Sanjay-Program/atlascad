#!/bin/sh
# Run one CAM job and exit with its status.
#
# The job file and the part are read-only by design; the output directory is the only writable path.
# `exec` makes the container's exit code atlas's, so 0/1/2/3 mean what
# docs/operations/cam-deployment.md says: posted, internal or I/O fault, bad job input, or the CAM
# checks refused the job.
#
#   ATLAS_JOB   versioned job file, default /work/job.json
#   ATLAS_OUT   output directory, default /out
set -eu

job="${ATLAS_JOB:-/work/job.json}"
out="${ATLAS_OUT:-/out}"

refuse() {
    printf 'atlas-cam: %s\n' "$2" >&2
    exit "$1"
}

[ -r "$job" ] || refuse 2 "cannot read the job $job; mount it there"
[ -d "$out" ] || refuse 2 "the output directory $out does not exist; mount a volume there"
[ -w "$out" ] || refuse 2 "the output directory $out is not writable"

exec /usr/local/bin/atlas cam job --job "$job" --out-dir "$out"
