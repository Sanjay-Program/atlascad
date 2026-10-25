#!/usr/bin/env bash
# Fetch the big-assembly STEP corpus. These files are ~1 GB and are deliberately
# NOT stored in this repository -- see README.md for why.
#
#   ./fetch.sh [target-dir]     default: $MONSTERTRUCK_STEP_CORPUS or ~/code/step-corpus/bigassy
set -euo pipefail

DEST="${1:-${MONSTERTRUCK_STEP_CORPUS:-$HOME/code/step-corpus/bigassy}}"
BASE="https://www.steptools.com/docs/stpfiles/bigassy"
FILES=(
  Ai-14R.stp
  Cruise_Assembly.stp
  NissanGT-R.STEP
  Rocky_House.stp
  ROTOR-201NAL-Z7.STEP
  Scania-8x4.stp
  Scania-Engine-V8-XT-Turbo.step
  UMC-500_SS_Solid_Model_2019-06_r1.stp
)

mkdir -p "$DEST"
echo "fetching into $DEST"
for f in "${FILES[@]}"; do
  if [ -s "$DEST/$f" ]; then
    printf '  have %s\n' "$f"
    continue
  fi
  printf '  get  %-42s' "$f"
  curl -fsSL --max-time 1800 -o "$DEST/$f" "$BASE/$f"
  printf '%s\n' "$(du -sh "$DEST/$f" | cut -f1)"
done

# Integrity: every file must open with the part-21 magic. Note `head -c 13`
# includes the trailing semicolon.
echo "verifying"
fail=0
for f in "${FILES[@]}"; do
  if [ "$(head -c 13 "$DEST/$f")" != "ISO-10303-21;" ]; then
    printf '  BAD  %s (not an ISO-10303-21 file)\n' "$f"
    fail=1
  fi
done
[ "$fail" -eq 0 ] && echo "all ${#FILES[@]} files verified" || { echo "verification FAILED"; exit 1; }

cat <<'NOTE'

Reminder: Ai-14R.stp is ISO-8859, not UTF-8 (Cyrillic FILE_NAME). Census it with
`grep -a` -- plain grep treats it as binary and reports 0 for every entity.
NOTE
