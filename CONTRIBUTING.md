# Contributing — compete on compression ratio

This repo is a shared autoresearch benchmark: improve the compressor in
`src/algorithm/`, lower the **SCORE** (total compressed bytes on the fixed
corpus), and leave a trail so the next person can build on your work.

Read [`AUTORESEARCH.md`](AUTORESEARCH.md) for the full rules before editing.

## Quick start

1. **Fork** the repo on GitHub and clone your fork.
2. Create a branch for your work:
   ```bash
   git checkout -b improve/statemap
   ```
3. Edit **only** `src/algorithm/` (see AUTORESEARCH.md).
4. Evaluate locally (optional, for iteration):
   ```bash
   bash scripts/evaluate.sh
   ```
5. Commit **only** your algorithm changes and open a pull request.
6. Fill in the PR template — especially **`## Approach`** and
   **`## Iteration notes`**. CI uses these when writing the history entry.
7. Wait for **Verify PR** — it scores on GitHub, then **auto-merges** to `main`.
8. **Scorekeeper** runs on merge and appends the verified ledger entry.

## CI is the source of truth

| What | Who updates it |
|------|----------------|
| `src/algorithm/` | You (via PR) |
| `RESULTS.md`, `history/entries/` | **Scorekeeper CI only** (on merge to `main`) |
| SCORE on the leaderboard | Computed by CI — never trust local claims |

**Do not** commit `RESULTS.md` or `history/entries/` in your PR. If you do, the
**Verify PR** and **Scorekeeper** workflows will fail.

Local `bash scripts/record.sh` is a preview helper only; it cannot push ledger
updates to `main`.

## Pull request checklist

- [ ] Only `src/algorithm/` changed
- [ ] PR template filled in (`## Approach` required for history)
- [ ] **Verify PR** GitHub Actions check passes
- [ ] No corpus-specific tuning or side channels (see AUTORESEARCH.md)
- [ ] Did **not** commit `RESULTS.md` or `history/entries/`

## Beating the record

If CI reports a SCORE **lower** than the current record in `RESULTS.md`, the PR
still auto-merges like any other passing submission — Scorekeeper marks the
entry as **record**. Non-record attempts merge too; Scorekeeper records the
verified score either way.

## Branch protection (maintainers)

On `main`, enable:

- Require pull request before merging
- Require status check **Verify PR / verify**
- Restrict who can push directly to `main`
- **Do not** require approving reviews (verification is the gate)

Auto-merge uses the default **`GITHUB_TOKEN`** via a separate **Auto-merge**
workflow (`workflow_run` after **Verify PR** succeeds). That pattern runs in the
base-repo context, so fork PRs merge without a PAT.

In **Settings → Actions → General → Workflow permissions**, choose **Read and
write permissions** for the default `GITHUB_TOKEN`.

Flow: **Verify PR** passes → **Auto-merge** squash-merges → **Scorekeeper**
commits the ledger.

## Questions

Open a GitHub issue for harness bugs or rule clarifications. Algorithm ideas
belong in PRs — the narrative goes in the PR description for CI to archive.
