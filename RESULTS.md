# Results log

Leaderboard of recorded submissions. Full narratives live in
[`history/entries/`](history/entries/).

| # | date | author | SCORE | Δ vs record | vs zstd-22 | commit | entry | note |
|---|------|--------|-------|-------------|------------|--------|-------|------|
| 0001 | 2026-06-14 | @10d9e | 642822 | — (baseline) | +7.06% | `d12023b` | [0001](history/entries/0001-baseline.md) | lpaq-class: orders 0-6 + word + sparse, match model, 2x APM, BCJ |
| 0002 | 2026-06-14 | @10d9e | 639105 | -3717 (new record) | +7.60% | `e838d6b` | [0002](history/entries/0002--10d9e.md) | 1. **Second match model at order-8.** Alongside the existing order-6 match model… |
| 0003 | 2026-06-14 | @10d9e | 637956 | -1149 (new record) | +7.76% | `3f837de` | [0003](history/entries/0003--10d9e.md) | Longer deterministic contexts continue to help the mixer on structured and textu… |
| 0004 | 2026-06-15 | @10d9e | 636158 | -1798 (new record) | +8.02% | `731096d` | [0004](history/entries/0004--10d9e.md) | - Add order-10, order-12, and order-14 match models to catch longer deterministi… |
| 0005 | 2026-06-15 | @10d9e | 628826 | -7332 (new record) | +9.08% | `019c128` | [0005](history/entries/0005--10d9e.md) | Adds three general-purpose shape/layout context models to the existing context m… |
| 0006 | 2026-06-15 | @10d9e | 614363 | -14463 (new record) | +11.17% | `847678f` | [0006](history/entries/0006--10d9e.md) | Adds an adaptive bit-history `StateMap` per context model and indexes each State… |
| 0007 | 2026-06-15 | @10d9e | 610511 | -3852 (new record) | +11.73% | `d8a8cd9` | [0007](history/entries/0007--10d9e.md) | Retunes three online-learning adaptation-rate constants — no new models, no co… |
| 0008 | 2026-06-15 | @10d9e | 606779 | -3732 (new record) | +12.27% | `03e1d79` | [0008](history/entries/0008--10d9e.md) | Extends the context-model bank from 17 to 23 models — all general-purpose, no … |
| 0009 | 2026-06-15 | @10d9e | 605962 | -817 (new record) | +12.40% | `8a1b5e6` | [0009](history/entries/0009--10d9e.md) | Adds word-level n-gram context models, targeting natural-language text where the… |
| 0010 | 2026-06-15 | @10d9e | 595819 | -10143 (new record) | +13.86% | `defe1d9` | [0010](history/entries/0010--10d9e.md) | Replaces the single context-selected logistic mixer with a two-layer mixing netw… |
| 0011 | 2026-06-15 | @10d9e | 594283 | -1536 (new record) | +14.08% | `c3774ba` | [0011](history/entries/0011--10d9e.md) | Adds a third APM/SSE calibration stage after the existing two, keyed on a *dense… |
| 0012 | 2026-06-15 | @10d9e | 588570 | -5713 (new record) | +14.91% | `f60bc60` | [0012](history/entries/0012--10d9e.md) | Expands the context-model bank from 26 to 47 models, all general-purpose, exploi… |
| 0013 | 2026-06-15 | @10d9e | 588120 | -450 (new record) | +14.97% | `7ef74d8` | [0013](history/entries/0013--10d9e.md) | Adds eight gap-bigram context models to the bank (26 -> ... -> now extended): th… |
| 0014 | 2026-06-15 | @10d9e | 587905 | -215 (new record) | +15.01% | `f323fca` | [0014](history/entries/0014--10d9e.md) | Re-tunes two online-learning constants that were last set at entry 0007, when th… |
| 0015 | 2026-06-15 | @10d9e | 586819 | -1086 (new record) | +15.16% | `5c18fb8` | [0015](history/entries/0015--10d9e.md) | Doubles each context model's hash table from 2^22 to 2^23 slots. With the contex… |
| 0016 | 2026-06-15 | @10d9e | 585739 | -1080 (new record) | +15.32% | `d7d4fec` | [0016](history/entries/0016--10d9e.md) | Two related SSE/APM improvements: 1. **Fourth APM/SSE stage keyed on match lengt… |
| 0017 | 2026-06-15 | @10d9e | 585226 | -513 (new record) | +15.39% | `31d60b0` | [0017](history/entries/0017--10d9e.md) | Adds a second layer-2 combiner and averages it with the existing one in the logi… |
| 0018 | 2026-06-15 | @10d9e | 584982 | -244 (new record) | +15.43% | `50f1f5e` | [0018](history/entries/0018--10d9e.md) | Re-tunes the layer-2 combiner learning rate from 4/65536 to 12/65536. The rate w… |
| 0019 | 2026-06-15 | @10d9e | 584723 | -259 (new record) | +15.47% | `5611fb9` | [0019](history/entries/0019--10d9e.md) | Expands the layer-2 ensemble from two combiners to four, averaged in the logit d… |
| 0020 | 2026-06-15 | @10d9e | 584276 | -447 (new record) | +15.53% | `bf5b353` | [0020](history/entries/0020--10d9e.md) | Adds eleven 4-sample strided context models — each hashes bytes at pos-k, pos-… |
| 0021 | 2026-06-15 | @10d9e | 583905 | -371 (new record) | +15.58% | `56ef71a` | [0021](history/entries/0021--10d9e.md) | Adds word-level n-gram/skip-gram context models, targeting natural-language text… |
| 0022 | 2026-06-15 | @10d9e | 583868 | -37 (new record) | +15.59% | `91f5665` | [0022](history/entries/0022--10d9e.md) | Extends the layer-2 ensemble from four combiners to five, averaged in the logit … |
| 0023 | 2026-06-15 | @10d9e | 583253 | -615 (new record) | +15.68% | `a5ff3e6` | [0023](history/entries/0023--10d9e.md) | Adds a sixth layer-1 specialist mixer selected by the current match state — th… |
| 0024 | 2026-06-15 | @10d9e | 583001 | -252 (new record) | +15.71% | `1ce805f` | [0024](history/entries/0024--10d9e.md) | Adds a seventh layer-1 specialist mixer selected by the byte column since the la… |
| 0025 | 2026-06-15 | @10d9e | 582758 | -243 (new record) | +15.75% | `1696f2b` | [0025](history/entries/0025--10d9e.md) | Adds an eighth layer-1 specialist mixer selected by a hash of the last four byte… |
| 0026 | 2026-06-15 | @10d9e | 582663 | -95 (new record) | +15.76% | `b0d13d3` | [0026](history/entries/0026--10d9e.md) | Adds a ninth layer-1 specialist mixer selected by a hash of the last six bytes (… |
| 0027 | 2026-06-15 | @10d9e | 582587 | -76 (new record) | +15.77% | `9330f7c` | [0027](history/entries/0027--10d9e.md) | Re-tunes the layer-1 specialist mixers' weight-update rate from 14/65536 to 12/6… |
| 0028 | 2026-06-15 | @10d9e | 582351 | -236 (new record) | +15.81% | `3f5917e` | [0028](history/entries/0028--10d9e.md) | Adds a tenth layer-1 specialist mixer selected by a stride-2 sparse context — … |
| 0029 | 2026-06-15 | @10d9e | 582052 | -299 (new record) | +15.85% | `a052d1e` | [0029](history/entries/0029--10d9e.md) | Adds an eleventh layer-1 specialist mixer selected by a stride-3 sparse context … |
| 0030 | 2026-06-15 | @10d9e | 581078 | -974 (new record) | +15.99% | `c292dc5` | [0030](history/entries/0030--10d9e.md) | Adds a new model family: 2D / 'byte-above' modelling, which predicts from the by… |
| 0031 | 2026-06-15 | @10d9e | 579415 | -1663 (new record) | +16.23% | `722ed67` | [0031](history/entries/0031--10d9e.md) | Adds a new model family: indirect context models. For each of the order-1..4 con… |
| 0032 | 2026-06-15 | @10d9e | 579224 | -191 (new record) | +16.26% | `5f3154f` | [0032](history/entries/0032--10d9e.md) | Extends the 2D / byte-above model family with two more contexts that read the up… |
| 0033 | 2026-06-15 | @10d9e | 579171 | -53 (new record) | +16.27% | `3a282e6` | [0033](history/entries/0033--10d9e.md) | Extends the indirect-model family to the word level: a hash table records the re… |
| 0034 | 2026-06-15 | @10d9e | 579101 | -70 (new record) | +16.28% | `562dd16` | [0034](history/entries/0034--10d9e.md) | Adds a run-length context: the last byte combined with the length of its current… |
| 0035 | 2026-06-15 | @10d9e | 578791 | -310 (new record) | +16.32% | `6236ca9` | [0035](history/entries/0035--10d9e.md) | Adds a sixth match model anchored on just the last 4 bytes. The existing match m… |
| 0036 | 2026-06-15 | @10d9e | 578673 | -118 (new record) | +16.34% | `ec9792e` | [0036](history/entries/0036--10d9e.md) | Retunes the short match model added in the previous entry from an order-4 anchor… |
| 0037 | 2026-06-15 | @10d9e | 578672 | -1 (new record) | +16.34% | `aa300ac` | [0037](history/entries/0037--10d9e.md) | Memory optimization. The six match-model hash tables were sized at 2^23..2^26 en… |
| 0038 | 2026-06-15 | @10d9e | 578672 | 0 (tie) | +16.34% | `de442dd` | [0038](history/entries/0038--10d9e.md) | Memory optimization with provably identical output. The order-0 context model ha… |
| 0039 | 2026-06-15 | @10d9e | 578672 | 0 (tie) | +16.34% | `c71ba05` | [0039](history/entries/0039--10d9e.md) | Memory optimization, provably identical output. The direct-counter probability t… |
| 0040 | 2026-06-15 | @10d9e | 578552 | -120 (new record) | +16.36% | `e0ecc8f` | [0040](history/entries/0040--10d9e.md) | Adds a new model family: a nesting model that tracks the stack of currently-open… |
| 0041 | 2026-06-15 | @10d9e | 578467 | -85 (new record) | +16.37% | `982c9ee` | [0041](history/entries/0041--10d9e.md) | Adds a fourteenth layer-1 specialist mixer selected by the current nesting state… |
| 0042 | 2026-06-15 | @10d9e | 578156 | -311 (new record) | +16.42% | `b738180` | [0042](history/entries/0042--10d9e.md) | Retunes the nonstationary bit-history state transition that feeds every context … |
| 0043 | 2026-06-15 | @10d9e | 578030 | -126 (new record) | +16.43% | `56c57d9` | [0043](history/entries/0043--10d9e.md) | Restructures the bit-history state byte. Under the reset-recency rule the minori… |
| 0044 | 2026-06-15 | @10d9e | 577720 | -310 (new record) | +16.48% | `daea46e` | [0044](history/entries/0044--10d9e.md) | Adds a new context family using only the high nibble of each byte, ignoring the … |
| 0045 | 2026-06-15 | @10d9e | 577319 | -401 (new record) | +16.54% | `1ec6f46` | [0045](history/entries/0045--10d9e.md) | Adds a fifteenth layer-1 specialist mixer selected by the high-nibble (opcode-cl… |
| 0046 | 2026-06-15 | @10d9e | 576969 | -350 (new record) | +16.59% | `65d60ac` | [0046](history/entries/0046--10d9e.md) | Adds a new context family using the differences between consecutive recent bytes… |
| 0047 | 2026-06-15 | @10d9e | 576705 | -264 (new record) | +16.62% | `88aa652` | [0047](history/entries/0047--10d9e.md) | Re-tunes the layer-1 specialist mixers' weight-update rate from 12/65536 to 8/65… |
| 0048 | 2026-06-15 | @10d9e | 576440 | -265 (new record) | +16.66% | `1ff5e3e` | [0048](history/entries/0048--10d9e.md) | Adds a sixteenth layer-1 specialist mixer selected by the character classes (let… |
| 0049 | 2026-06-15 | @10d9e | 576366 | -74 (new record) | +16.67% | `80d23a0` | [0049](history/entries/0049--10d9e.md) | Adds a seventeenth layer-1 specialist mixer selected by a combined 'mode': the l… |
| 0050 | 2026-06-16 | @10d9e | 575771 | -595 (new record) | +16.76% | `dd143e4` | [0050](history/entries/0050--10d9e.md) | Deepens the context-mixing network and extends the indirect family. (1) **Indire… |
| 0051 | 2026-06-16 | @10d9e | 575160 | -611 (new record) | +16.85% | `873e4b2` | [0051](history/entries/0051--10d9e.md) | Opens a new context-model vein by crossing the indirect family (the largest hist… |
| 0052 | 2026-06-16 | @10d9e | 575084 | -76 (new record) | +16.86% | `73d0d7e` | [0052](history/entries/0052--10d9e.md) | Follow-on mixer tuning that exploits the richer per-bit input set created by the… |
| 0053 | 2026-06-16 | @10d9e | 574203 | -881 (new record) | +16.99% | `f3fee9e` | [0053](history/entries/0053--10d9e.md) | Replaces direct-mapped context hash tables with 4-way set-associative buckets fo… |
| 0054 | 2026-06-16 | @10d9e | 574135 | -68 (new record) | +17.00% | `dd3f50b` | [0054](history/entries/0054--10d9e.md) | Widens the set-associative context tables (added in the previous record) from 4 … |
| 0055 | 2026-06-16 | @10d9e | 573541 | -594 (new record) | +17.08% | `9c85361` | [0055](history/entries/0055--10d9e.md) | Raises the prediction/coding precision from 12-bit to 16-bit through the final S… |
| 0056 | 2026-06-16 | @abipalli | 573376 | -165 (new record) | +17.11% | `6491617` | [0056](history/entries/0056--abipalli.md) | Size context tables to input length (recover the CI-blocked table-growth win) Th… |
| 0057 | 2026-06-16 | @10d9e | 572769 | -607 (new record) | +17.19% | `9b3433b` | [0057](history/entries/0057--10d9e.md) | Continues extending the indirect-on-transform family (each model hashes a byte-t… |
| 0058 | 2026-06-16 | @10d9e | 572769 | 0 (tie) | +17.19% | `e847a4b` | [0058](history/entries/0058--10d9e.md) | Pure efficiency change: SCORE is byte-for-byte identical (572769); the codec is … |
| 0059 | 2026-06-17 | @abipalli | 572643 | -126 (new record) | +17.21% | `1134861` | [0059](history/entries/0059--abipalli.md) | Adds a **second DMC instance at a conservative clone threshold**, complementing … |
| 0060 | 2026-06-17 | @10d9e | 573215 | +572 | +17.13% | `c495164` | [0060](history/entries/0060--10d9e.md) | Build fix. The merge of the table-shrink speed branch with the second-DMC model … |
| 0061 | 2026-06-17 | @10d9e | 573215 | +572 | +17.13% | `f163274` | [0061](history/entries/0061--10d9e.md) | Output-neutral complexity (WORK) reduction: byte-identical output on every corpu… |
| 0062 | 2026-06-17 | @10d9e | 573215 | +572 | +17.13% | `a377305` | [0062](history/entries/0062--10d9e.md) | Output-neutral WORK reduction in the hot per-model predict/update loops (the 106… |
| 0063 | 2026-06-17 | @10d9e | 573215 | +572 | +17.13% | `486bd63` | [0063](history/entries/0063--10d9e.md) | Output-neutral WORK reduction: removes bounds checks on the logistic/SSE lookup … |
| 0064 | 2026-06-17 | @10d9e | 573215 | +572 | +17.13% | `0eb4c30` | [0064](history/entries/0064--10d9e.md) | Output-neutral WORK reduction in the mixer, the hottest code (27 layer-1 + 10 la… |
| 0065 | 2026-06-17 | @abipalli | 572577 | -66 (new record) | +17.22% | `24a3698` | [0065](history/entries/0065--abipalli.md) | Two changes that together set a new record (**572643 → 572520, −123**): **1.… |
| 0066 | 2026-06-17 | @10d9e | 572577 | 0 bytes, -2276895323 WORK (new record) | +17.22% | `9d061ef` | [0066](history/entries/0066--10d9e.md) | Output-neutral WORK reduction targeting the newly-added Context Tree Weighting m… |
| 0067 | 2026-06-17 | @10d9e | 572577 | 0 bytes, -2349086360 WORK (new record) | +17.22% | `28c42a6` | [0067](history/entries/0067--10d9e.md) | Output-neutral WORK reduction in the CTW predict walk. The loop visits depths d … |
| 0068 | 2026-06-17 | @10d9e | 572577 | 0 bytes, -2510368780 WORK (new record) | +17.22% | `381d775` | [0068](history/entries/0068--10d9e.md) | Output-neutral WORK reduction in the CTW model (the dominant WORK contributor). … |
| 0069 | 2026-06-17 | @10d9e | 572577 | 0 bytes, -102694912 WORK (new record) | +17.22% | `71cbf52` | [0069](history/entries/0069--10d9e.md) | Output-neutral WORK reduction in the mixer, which profiling shows is ~74% of tot… |
| 0070 | 2026-06-17 | @10d9e | 572577 | 0 bytes, -753824008 WORK (new record) | +17.22% | `9db11bc` | [0070](history/entries/0070--10d9e.md) | Output-neutral WORK reduction in the mixer, which profiling shows is ~74% of tot… |
| 0071 | 2026-06-17 | @10d9e | 572577 | 0 bytes, -1485602816 WORK (new record) | +17.22% | `505ce52` | [0071](history/entries/0071--10d9e.md) | Output-neutral WORK reduction completing the mixer fusion. The previous PR fused… |
| 0072 | 2026-06-17 | @10d9e | 572577 | 0 bytes, -117458099 WORK (new record) | +17.22% | `6ccfc51` | [0072](history/entries/0072--10d9e.md) | Completes the mixer fusion by applying it to the 10 layer-2 combiners; the previ… |
| 0073 | 2026-06-17 | @abipalli | 572423 | -154 (new record) | +17.24% | `09048d9` | [0073](history/entries/0073--abipalli.md) | Deepens the Context Tree Weighting model's context from 32 bits (4 bytes) to **4… |
| 0074 | 2026-06-17 | @10d9e | 572423 | 0 bytes, -350612885 WORK (new record) | +17.24% | `3b0fdea` | [0074](history/entries/0074--10d9e.md) | Output-neutral WORK reduction in the Context Tree Weighting model. The depth-48 … |
| 0075 | 2026-06-17 | @10d9e | 572423 | 0 bytes, -201434556 WORK (new record) | +17.24% | `d4bcf54` | [0075](history/entries/0075--10d9e.md) | Output-neutral WORK and MEMCOST reduction in the Context Tree Weighting model. U… |
| 0076 | 2026-06-18 | @10d9e | 575829 | +3406 | +16.75% | `76d585a` | [0076](history/entries/0076--10d9e.md) | Aggressive model trimming to explore the speed/size Pareto frontier. Removed CTW… |
| 0077 | 2026-06-18 | @10d9e | 575394 | +2971 | +16.81% | `f2bea8e` | [0077](history/entries/0077--10d9e.md) | WORK/MEMCOST-neutral SCORE improvement over the lean non-winning entry 0076 (the… |
| 0078 | 2026-06-18 | @10d9e | 572232 | -191 (new record) | +17.27% | `bcf2b95` | [0078](history/entries/0078--10d9e.md) | perf: 16-bit input precision + CTW W_EST prior — NEW RECORD 572232, leaner (#9… |
| 0079 | 2026-06-18 | @10d9e | 572828 | +596 | +17.19% | `f358ca4` | [0079](history/entries/0079--10d9e.md) | perf: remove CTW from the record model (WORK -1.20G, MEMCOST -0.55G) (#95) |
| 0080 | 2026-06-18 | @10d9e | 572799 | +567 | +17.19% | `c81bf42` | [0080](history/entries/0080--10d9e.md) | perf: trim NL1 22->19 (WORK 5.66G->5.14G) (#96) |
| 0081 | 2026-06-18 | @10d9e | 572965 | +733 | +17.17% | `719ff3c` | [0081](history/entries/0081--10d9e.md) | perf: trim NL1 19->16 (mixer WORK) (#97) |
| 0082 | 2026-06-18 | @10d9e | 573659 | +1427 | +17.07% | `31ba9be` | [0082](history/entries/0082--10d9e.md) | perf: trim NL1 16->13 (mixer WORK) (#98) |
| 0083 | 2026-06-18 | @10d9e | 574109 | +1877 | +17.00% | `5e5b000` | [0083](history/entries/0083--10d9e.md) | perf: trim NCTX 106->96 (drop 10 tail context models) (#99) |
| 0084 | 2026-06-18 | @10d9e | 575137 | +2905 | +16.85% | `4028a1f` | [0084](history/entries/0084--10d9e.md) | perf: trim NCTX 96->86 (drop 10 more context models) (#100) |
| 0085 | 2026-06-18 | @10d9e | 575515 | +3283 | +16.80% | `003cd56` | [0085](history/entries/0085--10d9e.md) | perf: trim NCTX 86->81 (drop 5 more context models) (#101) |
| 0086 | 2026-06-18 | @10d9e | 575678 | +3446 | +16.77% | `ec1d979` | [0086](history/entries/0086--10d9e.md) | perf: trim NL1 13->11 (mixer WORK) (#102) |
| 0087 | 2026-06-18 | @10d9e | 576105 | +3873 | +16.71% | `42e5846` | [0087](history/entries/0087--10d9e.md) | perf: trim NL1 11->9 (mixer WORK) (#103) |
| 0088 | 2026-06-18 | @10d9e | 576105 | +3873 | +16.71% | `c558b58` | [0088](history/entries/0088--10d9e.md) | perf: remove 18 dead follow tables left by the NCTX trims (output-neutral) (#1… |
| 0089 | 2026-06-19 | @10d9e | 572470 | +238 | +17.24% | `19220f0` | [0089](history/entries/0089--10d9e.md) | Forks entry 0080 (572799 @ 5.14G WORK / 1.62G MEMCOST) and recovers -329 SCORE a… |
| 0090 | 2026-06-19 | @10d9e | 572407 | +175 | +17.25% | `048c7eb` | [0090](history/entries/0090--10d9e.md) | Builds on entry 0089 (tuned fork of 0080). The DMC clone-threshold vein the fron… |
| 0091 | 2026-06-19 | @10d9e | 572064 | -168 (new record) | +17.30% | `1e25c52` | [0091](history/entries/0091--10d9e.md) | Re-records the new record (SCORE 572060, beating 572232 by -172). The record alg… |
| 0092 | 2026-06-22 | @abipalli | 571972 | -92 (new record) | +17.31% | `1547277` | [0092](history/entries/0092--abipalli.md) | perf: transplant proven 5-DMC ensemble (1,2,3,5,8) onto the record — SCORE 572… |

**Current record: 571972** (@abipalli, entry 0092)

Ledger updates are **CI-only** — see [`.github/workflows/scorekeeper.yml`](.github/workflows/scorekeeper.yml).
