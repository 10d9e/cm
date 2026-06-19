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
has_algorithm=0
has_ci=0
while IFS= read -r f; do
  [[ -z "$f" ]] && continue
  case "$f" in
    src/algorithm/*) has_algorithm=1 ;;
    docs/*|scripts/build-leaderboard.py|scripts/guard-pr.sh) ;;
    # CI / workflow / infra changes (maintainer PRs): allowed on their own, but
    # NEVER combined with a src/algorithm submission — a submission must not be
    # able to edit the verify/scorekeeper workflows or scoring/metering scripts.
    .github/*|scripts/*) has_ci=1 ;;
    *) violations+=("$f") ;;
  esac
done < <(git diff --name-only "$base"...HEAD)

if (( ${#violations[@]} )); then
  echo "PR BOUNDARY VIOLATION — submissions may only change src/algorithm/;"
  echo "leaderboard/site PRs may only change docs/ or scripts/build-leaderboard.py;"
  echo "CI/infra PRs may only change .github/ or scripts/:"
  printf '  %s\n' "${violations[@]}"
  echo
  echo "Do not commit RESULTS.md or history/entries/ — CI records the score on merge."
  exit 1
fi

if (( has_algorithm && has_ci )); then
  echo "PR BOUNDARY VIOLATION — CI/infra changes (.github/ or scripts/) may not be"
  echo "combined with a src/algorithm submission; submit them as separate PRs."
  exit 1
fi

if (( has_algorithm )); then
  if ! grep -q 'pub fn compress(input: &\[u8\]) -> Vec<u8>' src/algorithm/mod.rs \
    || ! grep -q 'pub fn decompress(input: &\[u8\]) -> Vec<u8>' src/algorithm/mod.rs; then
    echo "PR BOUNDARY VIOLATION — frozen compress/decompress signatures were changed."
    exit 1
  fi

  # A submission must not register its own allocator in the algorithm surface.
  if grep -rqE '#\[\s*global_allocator\s*\]' src/algorithm/ 2>/dev/null; then
    echo "PR BOUNDARY VIOLATION — src/algorithm/ must not declare a #[global_allocator]"
    exit 1
  fi

  echo "PR boundary OK (only src/algorithm/ changed; contract intact)"
else
  echo "PR boundary OK (leaderboard/site changes only)"
fi
