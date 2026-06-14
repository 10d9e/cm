# Contributing — compete on compression ratio

This repo is a shared autoresearch benchmark: improve the compressor in
`src/algorithm/`, lower the **SCORE** (total compressed bytes on the fixed
corpus), and leave a trail so the next person can build on your work.

Read [`AUTORESEARCH.md`](AUTORESEARCH.md) for the full rules before editing.

## Quick start

1. **Fork** the repo on GitHub and clone your fork.
2. Set your handle so submissions are attributed:
   ```bash
   git config github.user your-github-handle
   ```
3. Create a branch for your work:
   ```bash
   git checkout -b improve/statemap
   ```
4. Edit **only** `src/algorithm/` (see AUTORESEARCH.md).
5. Evaluate:
   ```bash
   bash scripts/evaluate.sh
   ```
6. **Record your submission** (required for PRs that change the algorithm):
   ```bash
   bash scripts/record.sh \
     --author @your-github-handle \
     --note "Replaced plain counters with StateMap bit-history states because …" \
     --attempts "Tried order-8 first (+500 bytes) → reverted."
   ```
7. Commit algorithm changes **and** the generated `history/entries/…` + `RESULTS.md` row.
8. Open a pull request.

## What “memory” means here

Git alone does not explain *why* a change helped. This repo keeps an append-only
**submission ledger**:

| Artifact | Purpose |
|----------|---------|
| [`history/entries/`](history/entries/) | Full story per submission: approach, failed tries, diff, eval snapshot |
| [`RESULTS.md`](RESULTS.md) | Leaderboard with links into the ledger |
| Your PR description | Human-readable summary for reviewers |
| Git commits | Fine-grained code history inside `src/algorithm/` |

Every improvement (and worthwhile non-improving attempt) should be recorded with
`record.sh` so competitors can see how you got there.

## Pull request checklist

- [ ] `bash scripts/evaluate.sh` passes (guard + tests + valid SCORE)
- [ ] `bash scripts/record.sh …` run; new file under `history/entries/` included
- [ ] `RESULTS.md` updated by `record.sh`
- [ ] PR describes your hypothesis and key tradeoffs
- [ ] No changes outside `src/algorithm/`, `RESULTS.md`, and `history/entries/` (guard enforces this)
- [ ] No corpus embedding, side channels, or nondeterminism (see AUTORESEARCH.md)

## Beating the record

If your SCORE is **lower** than the current record in `RESULTS.md`, `record.sh`
marks the entry as **record** and your PR can merge the algorithm change into
`main`. If your SCORE is higher, still record it as an **attempt** — failed
experiments are valuable memory — but revert algorithm changes before merging
unless maintainers agree to keep them for other reasons.

## Questions

Open a GitHub issue for harness bugs or rule clarifications. Algorithm ideas
belong in PRs and history entries.
