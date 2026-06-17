# RFC 0001 — Account for complexity and memory, not just bytes

**Status:** Draft / request-for-comment.
**Scope:** how submissions are judged (the `metrics/` meters, `record.sh`,
`build-leaderboard.py`, CI gates, `AUTORESEARCH.md`). Touches frozen paths on
purpose — this is an infra/policy proposal, **not** a competition entry, so it
intentionally fails the `src/algorithm/`-only boundary guard.

## TL;DR

1. WORK-as-tiebreaker has almost no optimization gradient — it bites only on an
   *exact* byte tie, so bots ignore complexity. (The current record spends ~4 GB
   freely; WORK never affected its ranking.)
2. Don't fold WORK into SCORE with a product or weighted sum (`b·g`, `b+λg`) on
   the leaderboard — bytes are a *quality* target, cost is a *cost*; combining
   needs an arbitrary, gameable exchange rate.
3. **Better: fold cost into WORK itself**, so a single `(SCORE asc, WORK asc)`
   ranking already prices compute *and* memory. This PR does that — WORK now
   counts heap allocation alongside executed operators, flowing through the
   existing WORK plumbing unchanged.
4. **But a key subtlety (measured below): WORK is *differenced* to cancel
   one-time setup, which also cancels the one-time table allocation.** So WORK
   captures allocation *churn*, not the table *footprint*. The footprint (the
   real "GBs for a few hundred KB" pain) needs a *non-differenced, full-scale*
   memory number — provided here as a separate meter, and best enforced as a
   hard budget.

## 1. The problem

SCORE (compressed bytes) is optimized well, but nothing constrains resources, so
the frontier drifted to correct-but-degenerate solutions: the current record
peaks at **~4.4 GB reserved memory** (and tens of minutes) to compress a 2.36 MB
corpus. A valid SCORE winner; a poor codec.

## 2. Why the current WORK tiebreaker doesn't change behavior

Ranking is `(SCORE asc, WORK asc)` with WORK breaking *exact* byte ties only.
Independent algorithm changes essentially never tie to the byte, so WORK has no
gradient and a rational optimizer spends it freely. (Its one active effect is to
reward output-neutral micro-optimization of an existing record — real but narrow.)

## 3. Why a combined SCORE formula is the wrong tool

`b·g` treats bytes and cost symmetrically (a 1 % cost cut "buys" a 1 % byte cut),
contradicting "bytes primary," and its optimum sits at a mediocre middle. `b+λg`
needs an arbitrary λ that bots park on and that you tune forever. The ECDSA `t·q`
analogy doesn't transfer: there both factors are pure costs; here one is the goal.

## 4. Implemented: fold cost into WORK; meter the footprint separately

Two complementary meters, both outside `src/algorithm/` (tamper-proof):

**(a) WORK now includes heap allocation** (`metrics/`). A reusable tracking
allocator (`metrics/telemetry`) meters heap bytes requested; the wasm shim runs
under it, and the host charges `HEAP_GAS_PER_BYTE` per allocated byte on top of
the executed-operator fuel — both **differenced** (full prefix − half prefix) so
the one-time Cm setup cancels. Result still prints as `WORK:`, so it flows
through `measure-complexity.sh → ci-score.sh → record.sh → build-leaderboard.py`
with no plumbing change. The host also reports **peak wasm linear-memory pages**
(heap + shadow stack + statics; wasm memory only grows, so its final size is the
peak).

**(b) Full-scale reserved-memory meter** (`metrics/mem`, `scripts/measure-memory.sh`).
The same tracking allocator wraps the *native* codec over the *whole* corpus and
reports `MEM:` = peak live reserved HEAP bytes. Deterministic (it sums requested
byte sizes, independent of OS/page size/RSS), full-scale (no wasm32 4 GiB ceiling).
**Limitation (confirmed by review): heap only.** `static`/BSS, stack, and arena
memory are invisible — a `static mut [u8; N]` reports MEM ≈ 0. So a `MEM ≤ MEM_MAX`
gate built on this number is bypassable by moving tables into statics. A gate must
instead use a **source-agnostic** footprint: the wasm linear-memory high-water mark
(`cm_mem_pages`, already reported) or the touched-lines meter `(c)`.

**(c) Touched-cache-lines meter** (`metrics/lines`, prototype). Instruments every
wasm load/store (Walrus) to call `track(addr,size)` and counts distinct 64-byte
lines. Deterministic, and **counts touched memory regardless of source** (heap,
static, or stack) — so it closes the gaming hole in `(b)` and is also the
memory-*traffic* proxy that operator-fuel is blind to (a cache miss and an L1 hit
are the same one operator). Caveat: runs on the meter's sub-4 GiB prefix today;
full-corpus/5 GB scale needs `memory64` or a native DBI tool.

## 5. Measured finding — why both meters are needed

Running the unified WORK meter on the 8 KB prefix:

```
full 8192B  fuel 17,464,329,637   heap 1,173,403,332 B
half 4096B  fuel  9,231,366,194   heap 1,173,229,764 B
peak linear memory: 1.17 GB (heap + stack + statics)
WORK: 8,233,137,011  (= 8,232,963,443 fuel + 1 × 173,568 heap-bytes)
```

The table allocation (~1.17 GB) is **one-time at `Cm::new`**, so it is identical
in the full and half runs and **cancels in the differenced WORK** — the heap term
contributes only 173 KB. So:

- **WORK** (differenced) prices *per-byte* compute + allocation *churn*. Good for
  rewarding leaner hot loops; blind to one-time footprint.
- **MEM** (`metrics/mem`, non-differenced, full scale) prices the *footprint*
  (4.36 GB here) — the thing that actually hurts.

They measure different costs; keep both.

## 6. Recommendation

- **WORK = fuel + heap-allocation** is fine to keep, but note it is currently
  near-inert: per §5 the heap term is ~0.002 % of WORK, because this codec is
  allocation-free in steady state. The cost that actually dominates wall-clock is
  **memory traffic** (random scatter across multi-GB tables), which neither fuel
  nor allocation sees — a cache miss and an L1 hit are one operator either way. If
  WORK is meant to track real cost, its next term should be **modeled cache
  misses / touched lines `(c)`**, not allocation. Report heap churn as its own
  line rather than overstating it as "now prices memory."
- **Add a memory budget** as a hard *validity gate* (Hutter-Prize pattern), but
  base it on a **source-agnostic** footprint — wasm linear-memory high-water or
  the touched-lines meter `(c)` — **not** the heap-only `MEM`, which a `static`
  table bypasses.
- Optionally a `WORK_MAX` / wall-time cap for the slowest solutions.
- Tunables to decide: `MEM_MAX`, and if kept, `HEAP_GAS_PER_BYTE`.

## 7. Measurement / gaming cautions

- **OPEN — WORK differencing is gameable (not closed by this PR).** `FULL=8192`/
  `HALF=4096` are public constants over the public 8 KB sample; `compress()` sees
  a plain `&[u8]` and can branch on `input.len()` to run a cheap path only at
  those two lengths while the real 393,216-byte corpus runs the full algorithm —
  undetectable by the meter. The robust fix is to meter WORK on the **full SCORE
  corpus** (no distinguishable meter input), which forces `memory64` for the heavy
  build (wasm32's 4 GiB ceiling). This is a maintainer design decision; until then
  WORK is advisory, not adversarially sound.
- **Prefix is a different regime.** WORK runs at the ≤8 KB / 2^20-table / 2^16-buf
  regime (both prefixes sit below the 256 KB table gate), while SCORE/MEM run the
  2^22-table / 384 KB regime. The setup-cancellation only holds within the small
  regime; magnitudes don't transfer.
- **Keep ranking inputs deterministic, and pin the toolchain.** Wasm fuel,
  heap-byte counts and touched-lines are reproducible *given a pinned toolchain*
  (wasm codegen, hence fuel, varies by rustc version) — this PR adds a
  `rust-toolchain.toml`; builds should also use `--locked`. Native RSS and
  wall-clock are non-deterministic — never rank on those.

## 7a. Status after independent multi-agent review

- **Fixed in this PR:** (blocker) a `#[global_allocator]` in `src/algorithm/` is
  now rejected by `guard.sh`/`guard-pr.sh` (it would shadow the meters); (blocker)
  `ci-score.sh` no longer swallows a failed/empty WORK — it aborts rather than
  recording WORK-free; heap-only limitation of `MEM` documented; `verify.yml`
  WORK strings corrected; `rust-toolchain.toml` added; fuel subtraction made
  fail-loud.
- **Open (maintainer calls):** WORK differencing gameability (§7, needs
  full-corpus/`memory64` metering); a source-agnostic footprint for the MEM gate
  (use `(c)` or linear-memory high-water); `HEAP_GAS_PER_BYTE` value (or drop the
  term). This PR wires **no** gate, so these gate honest submissions today.

## 8. Suggested migration

1. Land these meters and report `WORK` (now incl. heap) and `MEM` next to each
   other (no ranking change).
2. Calibrate `MEM_MAX` (and `HEAP_GAS_PER_BYTE`) from the entry distribution.
3. Enforce `MEM ≤ MEM_MAX` as a validity gate in `evaluate.sh` / verify; keep
   `(SCORE asc, WORK asc)` among valid entries.
4. Document budgets in `AUTORESEARCH.md`.

## Appendix — files in this PR

- `metrics/telemetry/` — reusable tracking-allocator wrapper (heap volume + peak).
- `metrics/mem/` + `scripts/measure-memory.sh` — full-scale reserved-memory meter.
- `metrics/wasm/`, `metrics/host/` — WORK now folds in heap allocation and reports
  peak linear memory.
- `docs/proposals/0001-complexity-and-memory-scoring.md` — this document.

No `src/algorithm/` changes; no ranking code is altered (steps 2–4 are the
maintainers' call). For discussion.
