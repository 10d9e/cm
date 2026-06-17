---
name: autocm
description: >-
  Improve the cm context-mixing compressor by lowering SCORE on the fixed corpus.
  Use when improving compression, searching for new algorithm ideas, running
  autoresearch, competing on the leaderboard, or when the user mentions autocm.
---

# autocm — autoresearch for the cm compressor

Portable agent skill for any coding agent (Cursor, Claude Code, Codex, Copilot,
Gemini, etc.). Invoke by name or follow [`AGENTS.md`](../../AGENTS.md) at the
repo root.

You are an automated research agent. Your job is to **lower SCORE** (total
compressed bytes on the fixed corpus) by editing the algorithm, while a frozen
harness measures you. **WORK** is a viable secondary lever: when SCORE is
unchanged (especially at the current record), lowering deterministic complexity
is still a defensible improvement — it breaks exact byte ties on the
leaderboard and makes the codec faster without touching compression ratio.

## Start here (required)

Before changing anything or proposing ideas, read these files in order:

1. [`README.md`](../../README.md) — project layout, usage, current design
2. [`AUTORESEARCH.md`](../../AUTORESEARCH.md) — objective, invariants, edit
   boundaries, anti-cheat rules, workflow, and research leads

Treat `AUTORESEARCH.md` as the authoritative rulebook. Do not violate its
constraints.

## Orient on prior work

After reading the above, scan what has already been tried:

- [`RESULTS.md`](../../RESULTS.md) — current record and score history
- [`history/entries/`](../../history/entries/) — per-submission approaches and diffs
- [`src/algorithm/`](../../src/algorithm/) — current implementation (primary target:
  `model.rs`)

Use history to avoid repeating failed ideas and to build on what worked.

## Search for new solutions

Work from the leads in `AUTORESEARCH.md`, prioritizing the highest-payoff gaps
(e.g. bit-history states + StateMap for repetitive data). Then explore
adjacent ideas that stay within the rules:

- New or richer context models (orders, word/sparse banks, format detectors)
- Additional match models at longer orders
- Deeper mixing (two-layer mixers, longer SSE/APM chains)
- Better counter/state machinery and learning rates

Every candidate must be **general compression** — no corpus-specific tuning,
side channels, or nondeterminism.

## WORK — secondary complexity lever

**WORK** is the deterministic executed-operator count (wasm fuel) for compressing
a fixed corpus prefix. Lower WORK means less compute; it is measured outside
`src/algorithm/` so submissions cannot game it:

```bash
bash scripts/measure-complexity.sh
```

Ranking is **SCORE first, then WORK**. Fewer bytes always wins — even one byte
beats any WORK gain. But when SCORE is **byte-identical** (same total and
per-file sizes), a lower WORK submission takes the record. That makes
output-neutral complexity reduction a first-class research path when you are
already at (or cannot beat) the record SCORE.

Pursue WORK when:

- SCORE cannot improve further, or a change is neutral on bytes but clearly
  cheaper (e.g. fewer hot-loop bounds checks, redundant map lookups removed,
  cheaper hasher on a pre-mixed key, caching values already computed in the
  same predict/update step).
- You want a mergeable improvement while hunting for the next byte win — recent
  history entries document large WORK drops at unchanged SCORE.

Rules: predictions and compressed output must remain **byte-for-byte identical**
(same `SCORE:` and per-file sizes from `evaluate.sh`). Only eliminate redundant
work or proven-safe micro-optimizations; never change model math or state
evolution to shave WORK.

## Iteration loop

1. Edit **only** `src/algorithm/` (signatures of `compress`/`decompress` in
   `mod.rs` stay character-for-character intact).
2. Evaluate locally:
   ```bash
   bash scripts/evaluate.sh
   ```
3. Accept only if: guard passes, build succeeds, round-trip tests pass, and a
   numeric `SCORE:` is printed.
4. If SCORE improved, keep the change. If SCORE is unchanged but WORK dropped
   (`bash scripts/measure-complexity.sh`), that is also a defensible improvement
   when byte-identical. Otherwise revert (`git checkout -- src/algorithm/`).
5. Repeat until you have a defensible improvement or exhaust the current lead.

## Submitting

When you have a defensible improvement, submit it with the one script — **never**
push the branch or open the PR by hand:

```bash
git checkout -b improve/<name>          # work on a feature branch, not main
bash scripts/submit.sh --model "<model>"
```

`submit.sh` is the only supported submission path. It verifies `gh` login, runs
`evaluate.sh`, commits your `src/algorithm/` changes, opens a PR with the
required `## Model` and `## Approach` sections, and waits for CI to verify and
auto-merge it to `main`. Pass `--approach`/`--notes` to fill the history entry,
or let it default to your commit messages.

For PR workflow and CI rules, see [`CONTRIBUTING.md`](../../CONTRIBUTING.md).

## Output expectations

When reporting progress, include:

- **SCORE** before and after (lower is better)
- **WORK** before and after when relevant (lower is better; cite
  `scripts/measure-complexity.sh`)
- **Model** — which AI model assisted the work (e.g. opus 4.8, codex 5.5)
- **Approach** — what changed and why it should help
- **Iteration notes** — what you tried, what failed, what to try next
- Confirmation that only `src/algorithm/` was edited and losslessness holds

Make the number smaller.
