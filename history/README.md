# Submission history ledger

This directory is the repo's **memory**: a permanent, append-only record of how
each competitor arrived at their compressor changes and what score they achieved.

## Layout

```
history/
  README.md          this file
  entries/           one markdown file per recorded submission (never edit old entries)
  TEMPLATE.md        copy/paste guide for what a good entry looks like
```

Each entry captures:

- **Who** submitted (GitHub handle, git author, commit)
- **What** changed (`git diff` summary of `src/algorithm/`)
- **Score** (total compressed bytes) and delta vs the previous record
- **Approach** — the narrative: hypothesis, what you tried, what you kept or reverted
- **Eval snapshot** — per-file breakdown at submission time

## Recording a submission

After a passing run of `bash scripts/evaluate.sh`:

```bash
bash scripts/record.sh \
  --author @your-github-handle \
  --note "One paragraph on what you changed and why." \
  --attempts "Optional: failed tries you reverted along the way."
```

This appends a row to `RESULTS.md` (leaderboard) and writes a new file under
`history/entries/`. Entries are numbered sequentially (`0001`, `0002`, …).

## Rules

- **Append only.** Do not rewrite or delete past entries; they are the audit trail.
- **Valid candidates only.** Run `evaluate.sh` first — lossless round-trip must pass.
- **Explain the journey.** A score without notes is not useful to the next researcher.
- Entries with status `record` beat the previous best; `attempt` is a valid but
  non-improving run worth documenting anyway.

See [`CONTRIBUTING.md`](../CONTRIBUTING.md) for the full competition workflow.
