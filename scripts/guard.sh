#!/usr/bin/env bash
# Local boundary guard: fail if anything outside src/algorithm/ was changed
# relative to HEAD. Ledger files (RESULTS.md, history/entries/) are CI-only.
# FROZEN — do not edit as part of autoresearch.
# signatures were altered. FROZEN — do not edit as part of autoresearch.
set -euo pipefail
cd "$(dirname "$0")/.."

if ! git rev-parse --git-dir >/dev/null 2>&1; then
  echo "guard: not a git repo; run 'git init && git add -A && git commit -m base' first" >&2
  exit 2
fi

violations=()
while IFS= read -r f; do
  [[ -z "$f" ]] && continue
  case "$f" in
    src/algorithm/*) ;;
    *) violations+=("$f") ;;
  esac
done < <( { git diff --name-only HEAD; git ls-files --others --exclude-standard; } | sort -u )

if (( ${#violations[@]} )); then
  echo "BOUNDARY VIOLATION — these frozen files were modified:"
  printf '  %s\n' "${violations[@]}"
  echo "Only src/algorithm/ may change locally. CI updates RESULTS.md and history/entries/."
  exit 1
fi

# Frozen contract signatures must remain intact.
if ! grep -q 'pub fn compress(input: &\[u8\]) -> Vec<u8>' src/algorithm/mod.rs \
  || ! grep -q 'pub fn decompress(input: &\[u8\]) -> Vec<u8>' src/algorithm/mod.rs; then
  echo "BOUNDARY VIOLATION — frozen compress/decompress signatures were changed."
  exit 1
fi

# A submission must not register its own allocator: a #[global_allocator] in
# src/algorithm/ shadows the metering allocator and disables the WORK/MEM meters.
if grep -rqE '#\[\s*global_allocator\s*\]' src/algorithm/ 2>/dev/null; then
  echo "BOUNDARY VIOLATION — src/algorithm/ must not declare a #[global_allocator]"
  echo "(it would shadow the metering allocator and disable WORK/MEM)."
  exit 1
fi

echo "boundary OK (only src/algorithm/ changed; contract intact)"
