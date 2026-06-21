# `gpu/` — block-parallel CUDA compressor (research experiment)

A **separate research subproject**, outside the competition. It does **not** touch
`src/algorithm/` and is never submitted. The goal is to explore GPU
parallelization of the frontier-style context-mixing compressor and **measure the
speed-vs-ratio tradeoff** it forces.

## Why the frontier can't just be "CUDA-ified"

The frontier (`src/algorithm/`) is an lpaq-class context-mixing coder. For every
*bit* it predicts, arithmetic-codes, then **updates every model and mixer weight
before the next bit**. Each bit depends on the fully-updated state of the previous
bit — the stream is *inherently serial*. You cannot spread one stream across GPU
threads and keep identical output.

## The approach: block-parallel

Split the input into independent **segments** and compress each segment serially
on its own GPU thread. Parallelism comes from running thousands of segments at
once (grid-stride loop over a bounded pool of per-thread state slots). The cost is
**compression ratio**: each segment restarts model adaptation from scratch, so
smaller segments (more parallelism) compress worse. That tradeoff *is* the
experiment.

### Model (scaled, not the full frontier)

Porting all 106 models + DMC + CTW + two-layer mixer is impractical — every
parallel thread needs its *own* model state in GPU memory. So `gpu/` runs a
faithful but reduced **lpaq-lite** ([model.cuh](src/model.cuh)):

- StateMaps over byte-history context orders 0–4
- one match model (predicts the next byte from a hashed earlier occurrence)
- a single-layer logistic mixer (weights selected by the previous byte)
- the **exact** carry-less arithmetic coder ([coder.cuh](src/coder.cuh)) and
  stretch/squash math ([tables.hpp](src/tables.hpp)) ported from
  `src/algorithm/`

All hot-path code is integer-only and shared between the CPU reference and the GPU
kernels, so they produce **byte-identical** containers.

### Container

`"GPCM" | orig_len | e8e9_flag | seg_size | seg_count | seg_len[] | blobs`
([container.hpp](src/container.hpp)). The x86 BCJ (e8e9) filter is applied once
over the whole buffer before splitting and reversed after reassembly.

## Layout

| path | what |
|------|------|
| `src/common.cuh` | host/device macros + model size knobs (`CTX_BITS`, `MATCH_BITS`) |
| `src/tables.hpp` | stretch / squash / rate tables (mirror `tables.rs`) |
| `src/coder.cuh` | buffer-backed binary arithmetic coder |
| `src/model.cuh` | scaled context-mixing model |
| `src/segment.cuh` | per-segment serial codec (the body each GPU thread runs) |
| `src/container.hpp` | container format + e8e9 filter (host) |
| `src/cpu_ref.cpp` | host reference CLI — proves losslessness for free |
| `src/main.cu` | CUDA driver (kernels + host orchestration) |
| `bench/run.sh` | ratio sweep on corpus + throughput on a synthetic large file |
| `remote/provision.sh` | vast.ai: rent 4090 → build → bench → fetch → destroy |

## Build & run

```bash
# Local (no GPU): correctness + ratio
make cpu_ref
./cpu_ref selftest ../corpus/text_dickens.bin 65536   # roundtrip check
bash bench/run.sh                                      # writes bench/results.cpu.csv

# On the rented 4090 (provisioning script does this end-to-end):
make gpu CUDA_ARCH=sm_89
./gpu selftest ../corpus/binary_mozilla.bin 65536
./gpu bench ../corpus/text_dickens.bin 4096,16384,65536,262144
```

### On vast.ai (1× RTX 4090, cost-guarded, auto-destroy)

```bash
MAX_DPH=0.50 remote/provision.sh
```

Picks the cheapest single-4090 offer under `$MAX_DPH`/hr, syncs `gpu/` + `corpus/`,
builds with `nvcc`, runs the GPU roundtrip selftest on all six corpus files, runs
the benchmark, copies `bench/results.gpu.csv` back, and **always destroys the
instance on exit**.

## Result so far — ratio vs segment size (CPU reference, 6-file corpus)

Total over the 2,359,296-byte corpus. Fewer/larger segments → better ratio but
less parallelism; smaller segments → more GPU parallelism but more bytes.

| segment size | segments (corpus) | total bytes | ratio |
|-------------:|------------------:|------------:|------:|
| 393,216 (1/file) | 6   | 697,692   | 0.296 |
| 262,144          | 12  | 711,191   | 0.301 |
| 65,536           | 36  | 750,974   | 0.318 |
| 16,384           | 144 | 861,734   | 0.365 |
| 4,096            | 576 | 1,084,050 | 0.460 |

For context: frontier record **572,064**; zstd-22 **599,697**. The scaled model at
one-segment-per-file (697,692) is intentionally weaker than the full frontier —
the experiment is about the *shape* of the parallelism/ratio curve, isolated by
comparing the GPU run against this same scaled model run serially.

## GPU throughput — RTX 4090, 56.6 MB input (`bench/results.gpu.csv`)

The corpus files are too small (≤96 segments) to fill the GPU, so throughput is
measured on a 56.6 MB synthetic file (corpus ×24). Same integer codec as the CPU
reference — **every segment size roundtrips byte-identically**.

| segment size | ratio | compress MB/s | decompress MB/s |
|-------------:|------:|--------------:|----------------:|
| 393,216 | 0.296 |  6.0 |  6.2 |
| 262,144 | 0.303 |  8.8 |  9.3 |
|  65,536 | 0.318 | 33.2 | 34.9 |
|  16,384 | 0.365 | **87.0** | **91.6** |
|   4,096 | 0.459 | 78.0 | 78.7 |

Serial CPU reference (same model, 1 core) at seg=16,384: **7.2 MB/s** → the 4090 is
**~12× faster at identical compression (ratio 0.365)**.

### Findings

- **Throughput peaks at ~16 KB segments (~90 MB/s), then *drops* at 4 KB.**
  Counterintuitive — more segments should mean more parallelism. The cause: each
  segment re-initializes ~1.5 MB of per-thread model state (zeroing the StateMap +
  match tables in `Cm::init`). At 4 KB that fixed init cost dominates the actual
  compression, so throughput regresses. Clear optimization target: shrink
  per-thread tables (`CTX_BITS`/`MATCH_BITS`) or make init cheaper.
- **The speed/ratio knob is real and smooth:** ratio 0.296 → 0.459 buys 6 → 78
  MB/s; the sweet spot is ~16 KB (ratio 0.365 at ~90 MB/s).
- **Small inputs don't benefit:** single 393 KB corpus files hit only 0.2–3.5 MB/s
  — too few segments to fill 16k+ GPU threads. Block-parallel only pays off at
  tens of MB and up.

## Caveats

- Block-parallel **loses ratio** vs the frontier by design — that's the measured
  result, not a regression.
- The 393 KB corpus files yield few segments; `bench/run.sh` also builds a ~57 MB
  synthetic file (corpus concatenated) to give the GPU enough segments to show
  peak throughput.
- First cut is one-thread-per-segment. A one-*block*-per-segment variant (intra-
  block model parallelism + shared memory) is plausible future work.
