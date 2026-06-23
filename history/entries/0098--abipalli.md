# Entry 0098 — SCORE 571378 (-37 (new record))

| Field | Value |
|-------|-------|
| Date | 2026-06-23 |
| Author | @abipalli |
| Model | opus 4.8 |
| Git author | unknown \<unknown\> |
| Commit | `927bba9` (927bba97eeb8e67dce25b83873fedc2087108ae4) |
| SCORE | 571378 |
| Δ vs previous record | -37 (new record) |
| vs zstd -22 | +17.39% |
| WORK | 6081496453 |
| MEMCOST | 2386714544 |
| Status | record |

## Approach

feat: GLN-style halfspace-gated mixer specialist — SCORE 571415 -> 571378 (-37)
Adds a 23rd layer-1 mixer specialist whose gating is the GLN (Gated Linear
Networks, Veness et al. 2017) idea: instead of selecting its weight row by a
hand-designed byte context like the other 22 specialists, it selects by a
*data-dependent* gate — the sign-agreement pattern of 14 base predictions
(order-0..6 direct counters + their bit-history StateMaps), i.e. *which models
currently lean toward 1*. That 14-bit pattern indexes 16384 weight rows, trained
online toward the bit exactly like the other specialists, then fed to the
layer-2 combiners.
This partitions the input space orthogonally to the byte-context gates, capturing
model-agreement regimes they miss. Local SCORE 571415 -> 571378 (-37): nci -16,
source -15, dickens -9 lead. A 2nd GLN specialist (wider StateMap gate) regressed
(overlapping gate), so the bank is tapped at one. 9/9 round-trip lossless; only
src/algorithm/ touched.

## Algorithm changes

```
 src/algorithm/model.rs | 16 +++++++++++++++-
 1 file changed, 15 insertions(+), 1 deletion(-)
```

## Eval snapshot

```
file                        orig      ours    ratio  vs zstd    vs xz  lossless
binary_mozilla.bin        393216    228021    1.724    +3.5%    +3.6%  OK
repetitive_nci.bin        393216     13256   29.663   +47.4%   +43.6%  OK
source_samba.bin          393216    175224    2.244   +11.0%    +9.5%  OK
struct_xml.bin            393216      7388   53.224   +50.0%   +46.4%  OK
text_dickens.bin          393216     90395    4.350   +28.1%   +27.2%  OK
text_reymont.bin          393216     57094    6.887   +38.5%   +37.2%  OK
--------------------------------------------------------------------------------
TOTAL                    2359296    571378    4.129
  vs zstd -22 total: 691699 bytes  ->  +17.39% (smaller, WIN)
  vs xz -9e   total: 682460 bytes  ->  +16.28% (smaller, WIN)

SCORE: 571378 (total compressed bytes; lower is better)
```
