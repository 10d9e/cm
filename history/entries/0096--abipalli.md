# Entry 0096 — SCORE 571495 (-49 (new record))

| Field | Value |
|-------|-------|
| Date | 2026-06-23 |
| Author | @abipalli |
| Model | opus 4.8 |
| Git author | unknown \<unknown\> |
| Commit | `0f56d86` (0f56d8626f484a5759f54c79cb42ec9b6ea10923) |
| SCORE | 571495 |
| Δ vs previous record | -49 (new record) |
| vs zstd -22 | +17.38% |
| WORK | 5952215066 |
| MEMCOST | 2353755528 |
| Status | record |

## Approach

perf: count-based run-map StateMap adaptation — SCORE 571544 -> 571495 (-49)
The run-map StateMaps used a fixed >>6 adaptation rate (mirrored from the match
models, never tuned). A fixed-rate sweep showed slower is better (>>8 ~ -29),
which pointed at count-based adaptation: each (run-length, expected-bit) cell now
keeps a hit count and adapts at 1/(cnt+K) — fast on freshly-hit cells, asymptoting
to the slow rate the sweep wanted — exactly the scheme the main context StateMap
uses. Cells are now u32 (prob22<<10 | count); K=1 (fastest early, K=0 would divide
by zero on the first hit), CAP=255 (asymptote ~1/256, matching the >>8 fixed
optimum; CAP was flat 255..1023).
Local SCORE 571544 -> 571495 (-49): nci -31, dickens -13, xml -4 lead (count-based
helps the repetitive/structured cells most). 9/9 round-trip lossless; only
src/algorithm/ touched.

## Algorithm changes

```
 src/algorithm/model.rs | 22 ++++++++++++++++------
 1 file changed, 16 insertions(+), 6 deletions(-)
```

## Eval snapshot

```
file                        orig      ours    ratio  vs zstd    vs xz  lossless
binary_mozilla.bin        393216    228018    1.724    +3.5%    +3.6%  OK
repetitive_nci.bin        393216     13286   29.596   +47.3%   +43.5%  OK
source_samba.bin          393216    175247    2.244   +11.0%    +9.5%  OK
struct_xml.bin            393216      7401   53.130   +49.9%   +46.4%  OK
text_dickens.bin          393216     90414    4.349   +28.0%   +27.2%  OK
text_reymont.bin          393216     57129    6.883   +38.4%   +37.1%  OK
--------------------------------------------------------------------------------
TOTAL                    2359296    571495    4.128
  vs zstd -22 total: 691699 bytes  ->  +17.38% (smaller, WIN)
  vs xz -9e   total: 682460 bytes  ->  +16.26% (smaller, WIN)

SCORE: 571495 (total compressed bytes; lower is better)
```
