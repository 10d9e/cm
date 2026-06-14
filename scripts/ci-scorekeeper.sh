#!/usr/bin/env bash
# Run on push to main: verify ledger integrity, score algorithm changes, commit memory.
# Only this script (via GitHub Actions) may update RESULTS.md and history/entries/.
# FROZEN — do not edit as part of autoresearch.
set -euo pipefail
cd "$(dirname "$0")/.."

commit_msg="${GITHUB_EVENT_HEAD_COMMIT_MESSAGE:-$(git log -1 --format=%B)}"
if [[ "$commit_msg" == *"[skip ci]"* ]]; then
  echo "scorekeeper: skipping bot ledger commit"
  exit 0
fi

if ! git rev-parse HEAD~1 >/dev/null 2>&1; then
  echo "scorekeeper: no parent commit; nothing to compare"
  exit 0
fi

algo_changed="$(git diff --name-only HEAD~1 HEAD -- src/algorithm/ || true)"
ledger_changed="$(git diff --name-only HEAD~1 HEAD -- RESULTS.md history/entries/ || true)"

if [[ -n "$ledger_changed" && -z "$algo_changed" ]]; then
  echo "INTEGRITY VIOLATION: RESULTS.md or history/entries/ changed without an algorithm update." >&2
  echo "Only CI may update the ledger (commits tagged [skip ci])." >&2
  exit 1
fi

if [[ -n "$ledger_changed" && -n "$algo_changed" ]]; then
  echo "INTEGRITY VIOLATION: do not commit RESULTS.md or history/entries/ in your PR." >&2
  echo "CI records the verified score after merge." >&2
  exit 1
fi

if [[ -z "$algo_changed" ]]; then
  echo "scorekeeper: no algorithm changes on main; nothing to record"
  exit 0
fi

echo "== algorithm changed =="
printf '  %s\n' $algo_changed

echo "== evaluate (authoritative score) =="
bash scripts/evaluate.sh --no-guard

author="@${GITHUB_ACTOR:-unknown}"
note=""
attempts=""
pr_body=""

if [[ -n "${GITHUB_REPOSITORY:-}" && -n "${GITHUB_SHA:-}" ]]; then
  pr_body="$(gh api "repos/${GITHUB_REPOSITORY}/commits/${GITHUB_SHA}/pulls" \
    --jq '.[0].body // empty' 2>/dev/null || true)"
  pr_author="$(gh api "repos/${GITHUB_REPOSITORY}/commits/${GITHUB_SHA}/pulls" \
    --jq '.[0].user.login // empty' 2>/dev/null || true)"
  [[ -n "$pr_author" ]] && author="@${pr_author}"
fi

if [[ -n "$pr_body" ]]; then
  note="$(bash scripts/ci-parse-pr-body.sh Approach "$pr_body" || true)"
  attempts="$(bash scripts/ci-parse-pr-body.sh "Iteration notes" "$pr_body" || true)"
fi

if [[ -z "$note" ]]; then
  note="$(git log -1 --format=%B | sed '/^$/d' | head -5)"
fi
if [[ -z "$note" ]]; then
  note="Algorithm update merged to main (no PR description captured)."
fi

record_args=(--ci --author "$author" --note "$note" --diff-base HEAD~1)
if [[ -n "$attempts" ]]; then
  record_args+=(--attempts "$attempts")
fi

echo "== record submission =="
bash scripts/record.sh "${record_args[@]}"

git add RESULTS.md history/entries/
if git diff --staged --quiet; then
  echo "scorekeeper: record.sh made no ledger changes"
  exit 0
fi

entry_line="$(git diff --staged --name-only | grep '^history/entries/' | head -1 || true)"
entry_id="${entry_line##*/}"
entry_id="${entry_id%%-*}"

git config user.name "github-actions[bot]"
git config user.email "41898282+github-actions[bot]@users.noreply.github.com"
git commit -m "$(cat <<EOF
ci: record submission ${entry_id} [skip ci]

Authoritative ledger update from verified evaluate on main.
EOF
)"
git push origin HEAD:main

echo "scorekeeper: ledger committed and pushed"
