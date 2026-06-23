# Entry 0095 — SCORE 571544 (0 bytes, -24755055 WORK (new record))

| Field | Value |
|-------|-------|
| Date | 2026-06-23 |
| Author | @abipalli |
| Model | opus 4.8 |
| Git author | unknown \<unknown\> |
| Commit | `140a07b` (140a07b5b0756debfdc1bcd039fc3817472e4217) |
| SCORE | 571544 |
| Δ vs previous record | 0 bytes, -24755055 WORK (new record) |
| vs zstd -22 | +17.37% |
| WORK | 5951138295 |
| MEMCOST | 2359310938 |
| Status | record |

## Approach

perf: CTW transcendental skips (output-neutral) — WORK 5.795G -> 5.771G (-24.8M)
Two byte-identical short-circuits in the CTW walk, both relying on f64 ULP:
- ln_add(a,b): when the gap |a-b| > 40, the smaller term ln_1p(e^{lo-hi}) is below
  half the ULP of hi (callers always have hi <= ln(1-W_EST) = -0.386, so
  |hi| >= 0.386, ULP >= 2^-54), so hi+small == hi exactly. Skip exp/ln_1p.
- step(): when arg <= -38, WRATIO*e^arg < 2^-53, so 1+that == 1.0 and alpha is
  exactly 1.0 -> result is exactly nd.kt_p1(). Skip exp.
SCORE unchanged at 571544 on all 6 files (the bulk of CTW WORK is inherent
HashMap ops, which can't be cut without regressing memory). 9/9 round-trip
lossless; only src/algorithm/ touched.

## Algorithm changes

```
 src/algorithm/ctw.rs | 18 +++++++++++++++++-
 1 file changed, 17 insertions(+), 1 deletion(-)
```

## Eval snapshot

```
file                        orig      ours    ratio  vs zstd    vs xz  lossless
binary_mozilla.bin        393216    228025    1.724    +3.5%    +3.6%  OK
repetitive_nci.bin        393216     13314   29.534   +47.2%   +43.3%  OK
source_samba.bin          393216    175235    2.244   +11.0%    +9.5%  OK
struct_xml.bin            393216      7406   53.094   +49.9%   +46.3%  OK
text_dickens.bin          393216     90424    4.349   +28.0%   +27.2%  OK
text_reymont.bin          393216     57140    6.882   +38.4%   +37.1%  OK
--------------------------------------------------------------------------------
TOTAL                    2359296    571544    4.128
  vs zstd -22 total: 691699 bytes  ->  +17.37% (smaller, WIN)
  vs xz -9e   total: 682460 bytes  ->  +16.25% (smaller, WIN)

SCORE: 571544 (total compressed bytes; lower is better)
```
