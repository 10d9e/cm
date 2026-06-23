# Entry 0093 — SCORE 571565 (-407 (new record))

| Field | Value |
|-------|-------|
| Date | 2026-06-23 |
| Author | @abipalli |
| Model | opus 4.8 |
| Git author | unknown \<unknown\> |
| Commit | `7892d1a` (7892d1a0dd9e7e1254eaba7ccec5e15c2ef09b74) |
| SCORE | 571565 |
| Δ vs previous record | -407 (new record) |
| vs zstd -22 | +17.37% |
| WORK | 7334462767 |
| MEMCOST | 2472103026 |
| Status | record |

## Approach

perf: shrink run-map tables 2^22 -> 2^20 (memory) — SCORE 571565 (-407), records reliably
The 21-context run-map record (571563) is correct but its ~336 MB of run
tables tipped the model over the CI Scorekeeper's memory ceiling — the frozen
record.sh re-runs the full eval a second time, which got OOM/SIGTERM-killed at
~9 min across every attempt (same for the 27-context -430 variant).
Fix: each corpus file is only 393 KB (~393K byte positions), so a 2^22 (4M-slot)
run table is wildly oversized (load factor 0.09). Dropping to 2^20 (1M slots,
load 0.37, still collision-light with the 8-bit checksum) cuts run-map memory
336 MB -> 84 MB. SCORE moves +2 only (571563 -> 571565), still -407 vs the
571972 record. Small round-trip inputs stay at 2^18 (unchanged).
9/9 round-trip lossless; only src/algorithm/ touched.

## Algorithm changes

```
 src/algorithm/model.rs | 6 +++---
 1 file changed, 3 insertions(+), 3 deletions(-)
```

## Eval snapshot

```
file                        orig      ours    ratio  vs zstd    vs xz  lossless
binary_mozilla.bin        393216    228046    1.724    +3.5%    +3.6%  OK
repetitive_nci.bin        393216     13318   29.525   +47.2%   +43.3%  OK
source_samba.bin          393216    175248    2.244   +11.0%    +9.5%  OK
struct_xml.bin            393216      7406   53.094   +49.9%   +46.3%  OK
text_dickens.bin          393216     90414    4.349   +28.0%   +27.2%  OK
text_reymont.bin          393216     57133    6.882   +38.4%   +37.1%  OK
--------------------------------------------------------------------------------
TOTAL                    2359296    571565    4.128
  vs zstd -22 total: 691699 bytes  ->  +17.37% (smaller, WIN)
  vs xz -9e   total: 682460 bytes  ->  +16.25% (smaller, WIN)

SCORE: 571565 (total compressed bytes; lower is better)
```
