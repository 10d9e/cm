#!/usr/bin/env bash
# PR boundary guard: pull requests may change ONLY src/algorithm/.
# RESULTS.md and history/entries/ are updated exclusively by CI on merge to main.
# FROZEN — do not edit as part of autoresearch.
set -euo pipefail
cd "$(dirname "$0")/.."

base="${1:-}"
if [[ -z "$base" ]]; then
  if [[ -n "${GITHUB_BASE_SHA:-}" ]]; then
    base="$GITHUB_BASE_SHA"
  elif git rev-parse origin/main >/dev/null 2>&1; then
    base="$(git merge-base HEAD origin/main)"
  else
    base="$(git rev-parse HEAD~1)"
  fi
fi

violations=()
while IFS= read -r f; do
  [[ -z "$f" ]] && continue
  case "$f" in
    src/algorithm/*) ;;
    *) violations+=("$f") ;;
  esac
done < <(git diff --name-only "$base"...HEAD)

if (( ${#violations[@]} )); then
  echo "PR BOUNDARY VIOLATION — pull requests may only change src/algorithm/:"
  printf '  %s\n' "${violations[@]}"
  echo
  echo "Do not commit RESULTS.md or history/entries/ — CI records the score on merge."
  exit 1
fi

if ! grep -q 'pub fn compress(input: &\[u8\]) -> Vec<u8>' src/algorithm/mod.rs \
  || ! grep -q 'pub fn decompress(input: &\[u8\]) -> Vec<u8>' src/algorithm/mod.rs; then
  echo "PR BOUNDARY VIOLATION — frozen compress/decompress signatures were changed."
  exit 1
fi

# A submission must not register its own allocator: a #[global_allocator] in
# src/algorithm/ shadows the metering allocator and disables the WORK/MEM meters.
if grep -rqE '#\[\s*global_allocator\s*\]' src/algorithm/ 2>/dev/null; then
  echo "PR BOUNDARY VIOLATION — src/algorithm/ must not declare a #[global_allocator]"
  echo "(it would shadow the metering allocator and disable WORK/MEM)."
  exit 1
fi

echo "PR boundary OK (only src/algorithm/ changed; contract intact)"
