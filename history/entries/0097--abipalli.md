# Entry 0097 — SCORE 571415 (-80 (new record))

| Field | Value |
|-------|-------|
| Date | 2026-06-23 |
| Author | @abipalli |
| Model | opus 4.8 |
| Git author | unknown \<unknown\> |
| Commit | `09ff0f3` (09ff0f31833db8f8cf70b37e25d502fc596078d9) |
| SCORE | 571415 |
| Δ vs previous record | -80 (new record) |
| vs zstd -22 | +17.39% |
| WORK | 5952263742 |
| MEMCOST | 2356480904 |
| Status | record |

## Approach

perf: count-based match-model StateMaps — SCORE 571495 -> 571415 (-80)
Applies the same count-based 1/(cnt+K) adaptation that worked for the run maps
(entry 0096) to the six match-model StateMaps, which were on fixed shifts
(>>6,>>6,>>5,>>1,>>5,>>5). Each (match-length, expected-bit) cell now packs
prob22<<10 | count and adapts at 1/(cnt+K) — fast on freshly-hit cells, slow once
established. Cells are u32; a small macro shares the update across the 6 models.
Swept K/CAP: K=2, CAP=255 best (run maps preferred K=1; match models want
slightly slower early adaptation). Local SCORE 571495 -> 571415 (-80): reymont
-36, struct_xml -19, nci -14 lead; binary_mozilla +7 (its long matches liked the
old fast >>1 rate), net strongly positive. 9/9 round-trip lossless; only
src/algorithm/ touched.

## Algorithm changes

```
 src/algorithm/model.rs | 74 ++++++++++++++++++++++++++------------------------
 1 file changed, 39 insertions(+), 35 deletions(-)
```

## Eval snapshot

```
file                        orig      ours    ratio  vs zstd    vs xz  lossless
binary_mozilla.bin        393216    228025    1.724    +3.5%    +3.6%  OK
repetitive_nci.bin        393216     13272   29.627   +47.3%   +43.5%  OK
source_samba.bin          393216    175239    2.244   +11.0%    +9.5%  OK
struct_xml.bin            393216      7382   53.267   +50.0%   +46.5%  OK
text_dickens.bin          393216     90404    4.350   +28.1%   +27.2%  OK
text_reymont.bin          393216     57093    6.887   +38.5%   +37.2%  OK
--------------------------------------------------------------------------------
TOTAL                    2359296    571415    4.129
  vs zstd -22 total: 691699 bytes  ->  +17.39% (smaller, WIN)
  vs xz -9e   total: 682460 bytes  ->  +16.27% (smaller, WIN)

SCORE: 571415 (total compressed bytes; lower is better)
```
