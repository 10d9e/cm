# cm — a context-mixing compressor (with an autoresearch harness)

A general lossless compressor that maximizes **compression ratio**. On the
bundled dev corpus it beats `zstd -22` and `xz -9e` in aggregate, with the
largest wins on natural-language text (~19% smaller than zstd).

It is built to be improved by automated agents: the algorithm lives behind a
fixed contract, and a frozen harness scores any candidate. See
[`AUTORESEARCH.md`](AUTORESEARCH.md) for the rules.

**[Live leaderboard →](https://10d9e.github.io/cm/)** — score chart and full
submission history, updated automatically by CI on every verified merge.

## Layout

```
src/algorithm/   EDITABLE — the compressor (model, coder, tables, filters)
src/harness/     frozen   — corpus loader + scoring
src/main.rs      frozen   — CLI
tests/           frozen   — losslessness gate (fuzzed, not corpus-tied)
corpus/          frozen   — fixed benchmark + baselines.tsv
history/         ledger   — append-only submission history (entries/ editable)
scripts/         frozen   — guard.sh, evaluate.sh, submit.sh, record.sh, CI scorekeeper
```

## Usage

```
cargo build --release
./target/release/cm c file.in file.cm     # compress
./target/release/cm d file.cm file.out    # decompress
./target/release/cm eval corpus           # score against the corpus
```

Or grade a candidate locally (guard + tests + score; ledger updates are CI-only):

```
bash scripts/evaluate.sh
```

When you have an improvement, **submit it with the one script** — never push or
open the PR by hand:

```
bash scripts/submit.sh --model "opus 4.8"
```

`submit.sh` checks `gh` login, runs `evaluate.sh`, commits your `src/algorithm/`
changes, opens a PR with the required `## Model` / `## Approach` sections, and
waits for CI to verify and land it. Pull requests are checked on GitHub
(**Verify PR**: boundary + metadata), auto-merged on pass, then **Scorekeeper**
runs the correctness gate, computes the authoritative SCORE, and appends to
`RESULTS.md` and `history/entries/`. Non-winning submissions (higher SCORE,
lower WORK) merge and record the same way.

## Design (current)

lpaq-class context mixing: per-bit prediction from multi-order hashed context
models (orders 0–6 + word + sparse) with adaptive-rate counters, a learned
match model, a context-selected logistic mixer, a two-stage APM/SSE, an x86
BCJ filter, and a binary arithmetic coder. The primary objective is compression
ratio; **WORK** (deterministic wasm fuel / executed operators, lower is faster)
is a secondary lever — it breaks exact byte-score ties on the leaderboard and
rewards output-neutral speedups when SCORE cannot move. A second, informational
axis, **MEMCOST** (deterministic cache-miss penalty from a fixed cache model over
the wasm access trace, via `bash scripts/measure-memcost.sh`), tracks memory/cache
traffic — the latency cost WORK's operator count cannot see; it is shown on the
leaderboard but does not affect ranking. Decompression is symmetric and slow by
design.

## Improving it

Edit only `src/algorithm/`, run `bash scripts/evaluate.sh` locally to iterate, then
submit with `bash scripts/submit.sh` and let CI record verified scores. See
[`CONTRIBUTING.md`](CONTRIBUTING.md)
and [`history/README.md`](history/README.md). The biggest known lever is replacing the plain counters with
bit-history states + a StateMap (helps the repetitive-data cases). When you are
at the record SCORE, lowering **WORK** with byte-identical output (measure via
`bash scripts/measure-complexity.sh`) is still a valid improvement — see
`AUTORESEARCH.md` for ranking rules and examples. Details and constraints are in
`AUTORESEARCH.md`.
