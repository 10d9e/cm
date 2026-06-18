#!/usr/bin/env bash
# Scorekeeper — PUBLISH phase. FROZEN — do not edit as part of autoresearch.
#
# This phase holds the privileged push token (SCOREKEEPER_PAT) but NEVER builds
# or runs competitor code. It only applies the ledger files produced by the
# score phase and pushes them to main. Because no untrusted code runs here, the
# token cannot be exfiltrated by a malicious submission.
set -euo pipefail
cd "$(dirname "$0")/.."

IN_DIR="${IN_DIR:-ledger-in}"
if [[ ! -f "$IN_DIR/meta.env" ]]; then
  echo "publish: no ledger artifact; nothing to do"
  exit 0
fi

# shellcheck disable=SC1090,SC1091
source "$IN_DIR/meta.env"
if [[ "${RECORD:-0}" != "1" ]]; then
  echo "publish: score phase recorded nothing; nothing to publish"
  exit 0
fi

# Validate the handoff before trusting any of it (the score phase shares a
# filesystem with untrusted code; constrain what we will commit).
if [[ ! "${ENTRY_ID:-}" =~ ^[0-9]{4}$ ]]; then
  echo "publish: bad ENTRY_ID '${ENTRY_ID:-}'" >&2
  exit 1
fi
case "${ENTRY_FILE:-}" in
  ""|*..*|*/*) echo "publish: bad ENTRY_FILE '${ENTRY_FILE:-}'" >&2; exit 1 ;;
esac
if [[ ! -f "$IN_DIR/RESULTS.md" || ! -f "$IN_DIR/entries/$ENTRY_FILE" ]]; then
  echo "publish: missing ledger files in artifact" >&2
  exit 1
fi

# Apply only the ledger paths onto the current (fresh) checkout of main.
git config user.name "github-actions[bot]"
git config user.email "41898282+github-actions[bot]@users.noreply.github.com"

# Re-apply the ledger onto the LATEST main and push, retrying on a lost race with
# concurrent algorithm auto-merges. Those merges advance main during the multi-
# minute score phase but only touch src/algorithm/ — never RESULTS.md or
# history/entries/ — and the scorekeeper workflow is serialized (concurrency:
# scorekeeper-main), so no other run can change the ledger underneath us.
# Re-applying the already-computed ledger files onto a freshly fetched main is
# therefore always correct; we just have to land on top of whatever algorithm
# commits arrived, instead of pushing a now-stale parent (the old single push
# failed non-fast-forward whenever the next PR merged before publish).
for attempt in $(seq 1 8); do
  git fetch --quiet origin main
  git reset --quiet --hard origin/main
  cp "$IN_DIR/RESULTS.md" RESULTS.md
  mkdir -p history/entries
  cp "$IN_DIR/entries/$ENTRY_FILE" "history/entries/$ENTRY_FILE"
  git add RESULTS.md "history/entries/$ENTRY_FILE"
  if git diff --staged --quiet; then
    echo "publish: ledger already current (entry ${ENTRY_ID}); nothing to push"
    exit 0
  fi
  git commit -q -m "$(cat <<EOF
ci: record submission ${ENTRY_ID} [skip ci]

Authoritative ledger update from verified evaluate on main.
EOF
)"
  if git push --quiet origin HEAD:main; then
    echo "publish: ledger committed and pushed (entry ${ENTRY_ID}, attempt ${attempt})"
    exit 0
  fi
  echo "publish: push rejected — main advanced during scoring; retrying (attempt ${attempt})"
  sleep $(( (RANDOM % 5) + 1 ))
done
echo "publish: failed to push ledger after 8 attempts" >&2
exit 1

echo "publish: ledger committed and pushed (entry ${ENTRY_ID})"
