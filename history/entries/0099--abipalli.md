# Entry 0099 — SCORE 571321 (-57 (new record))

| Field | Value |
|-------|-------|
| Date | 2026-06-24 |
| Author | @abipalli |
| Model | opus 4.8 |
| Git author | unknown \<unknown\> |
| Commit | `a7d8fa5` (a7d8fa54a2c4a9c5a13689dcea1cbef827fb1900) |
| SCORE | 571321 |
| Δ vs previous record | -57 (new record) |
| vs zstd -22 | +17.40% |
| WORK | 6289206058 |
| MEMCOST | 2309493086 |
| Status | record |

## Approach

feat: true-halfspace GLN gate + complementary axis-aligned specialist — SCORE 571378 -> 571321 (-57)
Pushes the GLN mixing idea (entry 0098) to true Veness-style halfspace gating.
Entry 0098's GLN specialist gated on the sign of each base prediction (axis-
aligned halfspaces). This adds a specialist gated by GLN_BITS=14 *true*
halfspaces: each gate bit is sign(<fixed pseudo-random ±1 hyperplane, preds>)
over 22 base predictions (order-0..6 counters + StateMaps, 6 match models, DMC,
CTW) — weighted-agreement directions, indexing 16384 weight rows.
The two gate geometries are complementary: the halfspace gate helps text
(dickens -37), the axis-aligned gate helps repetitive/source (nci, source), so
the codec keeps BOTH (NL1 23->24). A 2nd random-halfspace specialist regressed
(noise), so NGLN_HS=1. Local SCORE 571378 -> 571321 (-57): dickens -43, nci -9,
source -3. 9/9 round-trip lossless; only src/algorithm/ touched.

## Algorithm changes

```
 src/algorithm/model.rs | 55 ++++++++++++++++++++++++++++++++++++++++++--------
 1 file changed, 47 insertions(+), 8 deletions(-)
```

## Eval snapshot

```
file                        orig      ours    ratio  vs zstd    vs xz  lossless
binary_mozilla.bin        393216    228020    1.724    +3.5%    +3.6%  OK
repetitive_nci.bin        393216     13247   29.683   +47.4%   +43.6%  OK
source_samba.bin          393216    175221    2.244   +11.0%    +9.5%  OK
struct_xml.bin            393216      7391   53.202   +50.0%   +46.4%  OK
text_dickens.bin          393216     90352    4.352   +28.1%   +27.2%  OK
text_reymont.bin          393216     57090    6.888   +38.5%   +37.2%  OK
--------------------------------------------------------------------------------
TOTAL                    2359296    571321    4.130
  vs zstd -22 total: 691699 bytes  ->  +17.40% (smaller, WIN)
  vs xz -9e   total: 682460 bytes  ->  +16.29% (smaller, WIN)

SCORE: 571321 (total compressed bytes; lower is better)
```
