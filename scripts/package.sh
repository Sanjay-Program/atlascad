#!/usr/bin/env bash
# Build and verify a deployable bundle.
#
#   ./scripts/package.sh              # build, stage, tar, verify
#   ./scripts/package.sh --no-verify  # skip extraction and smoke tests (not recommended)
#
# The bundle is self-contained: the three binaries, the licence and notice files, the measured
# quality documents and a CycloneDX SBOM, plus SHA256SUMS. Signing and notarisation are NOT done
# here — they need keys this repository does not hold. See docs/operations/release-process.md.
set -euo pipefail
cd "$(dirname "$0")/.."

JOBS="${JOBS:-2}"
VERIFY=1
if [ "${1:-}" = "--no-verify" ]; then
  VERIFY=0
fi

VERSION="$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -1)"
if [ -z "$VERSION" ]; then
  echo "cannot read the workspace version from Cargo.toml" >&2
  exit 1
fi
TARGET="$(rustc -vV | sed -n 's/^host: //p')"
NAME="atlas-cad-$VERSION-$TARGET"
DIST="dist"
STAGE="$DIST/$NAME"

printf '==> building release binaries (%s)\n' "$TARGET"
cargo build --offline --release -p cad-studio -p cad-cli -p cad-engineering -j "$JOBS"

printf '==> SBOM\n'
cargo run --offline -q -p cad-engineering --bin atlas-eng -- sbom --write

printf '==> staging %s\n' "$STAGE"
rm -rf "$STAGE"
mkdir -p "$STAGE/bin" "$STAGE/docs"

cp target/release/atlas "$STAGE/bin/atlas"
cp target/release/cad-studio "$STAGE/bin/cad-studio"
cp target/release/atlas-eng "$STAGE/bin/atlas-eng"
cp LICENSE COMMERCIAL-LICENSE.md README.md "$STAGE/"
cp docs/feature-matrix.md docs/quality/production-readiness.md "$STAGE/docs/"
cp engineering/sbom/atlascad.cdx.json "$STAGE/atlascad.cdx.json"

cat > "$STAGE/RUN.txt" <<'TXT'
Atlas CAD bundle
================

Binaries (x86-64/arm64 host build, static Rust runtime, no installer yet):

  bin/cad-studio   local application; `cad-studio --new part.zcad`, then open the printed URL
  bin/atlas        headless contract consumer; `atlas --help`
  bin/atlas-eng    engineering control plane; `atlas-eng --help`

Documents:

  README.md                       what this is and what it cannot do
  COMMERCIAL-LICENSE.md           the licence path; not a grant
  docs/feature-matrix.md          exactly which capabilities are verified, absent or advertised
  docs/production-readiness.md    measured gaps against a paid professional bar
  atlascad.cdx.json               CycloneDX 1.5 SBOM for the shipped dependency graph
  SHA256SUMS                      checksums of every file above

Verify before running:  sha256sum -c SHA256SUMS
TXT

printf '==> checksums\n'
(
  cd "$STAGE"
  find . -type f ! -name SHA256SUMS -print0 | sort -z | xargs -0 sha256sum > SHA256SUMS
)

printf '==> archive\n'
TARBALL="$DIST/$NAME.tar.gz"
rm -f "$TARBALL"
tar --sort=name --mtime='UTC 2026-01-01' --owner=0 --group=0 --numeric-owner \
  -czf "$TARBALL" -C "$DIST" "$NAME"

if [ "$VERIFY" -eq 1 ]; then
  printf '==> verifying the archive in a clean directory\n'
  WORK="$(mktemp -d)"
  trap 'rm -rf "$WORK"' EXIT
  tar -xzf "$TARBALL" -C "$WORK"
  cd "$WORK/$NAME"
  sha256sum -c --quiet SHA256SUMS
  ./bin/atlas --help >/dev/null
  ./bin/cad-studio --help >/dev/null
  ./bin/atlas-eng --help >/dev/null
  ./bin/atlas box --size 10,10,5 --out part.stl >/dev/null
  [ -s part.stl ] || { echo "the packaged CLI produced no STL" >&2; exit 1; }
  printf 'verify: %s extracts, checksums pass, and the packaged CLIs run\n' "$TARBALL"
  cd - >/dev/null
fi

printf '\n==> bundle ready\n'
ls -lh "$TARBALL" | awk '{print $5, $9}'
printf 'extract: tar -xzf %s\n' "$TARBALL"
