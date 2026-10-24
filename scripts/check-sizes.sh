#!/usr/bin/env bash
# File-length gate. Function length / parameter count are enforced by clippy (Rust)
# and detekt (Kotlin); this covers whole files across languages.
set -euo pipefail

SOFT=300
HARD=500
cd "$(dirname "$0")/.."

mapfile -t files < <(
  git ls-files --cached --others --exclude-standard -- '*.rs' '*.kt' '*.kts' '*.ts' '*.tsx' \
    | grep -Ev '(^|/)(tests?|generated)/|(^|/)tests\.rs$|\.(test|spec)\.tsx?$|/build/|/target/'
)

status=0
for f in "${files[@]}"; do
  [[ -f "$f" ]] || continue
  n=$(wc -l < "$f")
  if (( n > HARD )); then
    echo "FAIL  $f: $n lines (hard limit $HARD)"
    status=1
  elif (( n > SOFT )); then
    echo "WARN  $f: $n lines (soft limit $SOFT, review for a split)"
  fi
done

(( status == 0 )) && echo "sizes ok (${#files[@]} files checked)"
exit $status
