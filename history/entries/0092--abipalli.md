# Entry 0092 — SCORE 571972 (-92 (new record))

| Field | Value |
|-------|-------|
| Date | 2026-06-22 |
| Author | @abipalli |
| Model | opus 4.8 |
| Git author | unknown \<unknown\> |
| Commit | `1547277` (1547277e2ba0f54b3801b586b49446227b00f73b) |
| SCORE | 571972 |
| Δ vs previous record | -92 (new record) |
| vs zstd -22 | +17.31% |
| WORK | 6919313233 |
| MEMCOST | 2385213254 |
| Status | record |

## Approach

perf: transplant proven 5-DMC ensemble (1,2,3,5,8) onto the record — SCORE 572064 -> 571972 (-92)
The record (entry 0091) ran only two DMCs at clone thresholds (1,1),(2,2).
Entry 0090's search proved a (1,2,3,5,8) DMC spread is optimal on the lean
fork; this transplants the three missing slower DMCs (3,3),(5,5),(8,8) onto
the full record model. Two of the three reuse previously-dead zero mixer
inputs; the third bumps NINPUT by one. The slower DMCs add stable lower-order
signal complementary to the two aggressive ones, and the mixer down-weights
any redundancy with CTW.
Local SCORE 572064 -> 571972 (-92): reymont -44, dickens -22, nci -14,
xml -8, samba -6, mozilla +2. 9/9 round-trip lossless; only src/algorithm/
touched.

## Algorithm changes

```
 src/algorithm/model.rs | 21 ++++++++++++++++++++-
 1 file changed, 20 insertions(+), 1 deletion(-)
```

## Eval snapshot

```
file                        orig      ours    ratio  vs zstd    vs xz  lossless
binary_mozilla.bin        393216    228216    1.723    +3.4%    +3.5%  OK
repetitive_nci.bin        393216     13297   29.572   +47.2%   +43.4%  OK
source_samba.bin          393216    175397    2.242   +11.0%    +9.4%  OK
struct_xml.bin            393216      7442   52.837   +49.6%   +46.1%  OK
text_dickens.bin          393216     90481    4.346   +28.0%   +27.1%  OK
text_reymont.bin          393216     57139    6.882   +38.4%   +37.1%  OK
--------------------------------------------------------------------------------
TOTAL                    2359296    571972    4.125
  vs zstd -22 total: 691699 bytes  ->  +17.31% (smaller, WIN)
  vs xz -9e   total: 682460 bytes  ->  +16.19% (smaller, WIN)

SCORE: 571972 (total compressed bytes; lower is better)
```
