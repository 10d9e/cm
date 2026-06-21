# GPU Block-Parallel Compression — Results & Findings

**Experiment:** parallelize the frontier-style context-mixing compressor on a GPU,
measure the speed-vs-ratio tradeoff it forces, and scale it across multiple GPUs.
**Hardware:** 1× RTX 4090 (Ada, sm_89) and 8× RTX 5090 (Blackwell, sm_120), rented
on vast.ai (~$0.35/hr and ~$3.20/hr).
**Status:** working end-to-end; lossless; CUDA output byte-identical to the CPU
reference at every segment size. Multi-GPU scales **near-linearly — 7.5× on 8
GPUs, >1 GB/s aggregate compress** (§8).

---

## 1. Background: why the frontier is serial

The frontier (`src/algorithm/`) is an lpaq-class context-mixing coder. For every
*bit* it predicts → arithmetic-codes → **updates every model and mixer weight
before the next bit**. Each bit depends on the fully-updated state of the previous
bit, so a single stream cannot be split across GPU threads without changing the
output. There is no data-parallelism *within* a stream.

## 2. Approach: block-parallel

Split the input into independent **segments**; compress each serially on its own
GPU thread (grid-stride over a memory-bounded pool of per-thread state slots).
Parallelism comes from thousands of concurrent segments. The cost is ratio: each
segment restarts model adaptation, so smaller segments compress worse. **That
tradeoff is the experiment.**

### Model (scaled, GPU-resident)

A faithful but reduced lpaq-lite, because every parallel thread needs its own
model state in GPU memory:

- StateMaps over byte-history context orders 0–4
- one match model (predicts the next byte from a hashed earlier occurrence)
- single-layer logistic mixer (weights selected by the previous byte)
- the **exact** carry-less arithmetic coder and stretch/squash math ported from
  `src/algorithm/{coder.rs,tables.rs}`

All hot-path code is integer-only and shared between the CPU reference and the GPU
kernels → identical containers. Per-thread state ≈ 1.5 MB (5×64K StateMap entries +
64K match-hash entries).

## 3. Correctness

`decompress(compress(x)) == x` verified on:

- all 6 corpus files, segment sizes 4 KB … whole-file (CPU + GPU)
- a 56.6 MB synthetic file (corpus ×24), all segment sizes (GPU)
- edge cases: empty, 1-byte, 100 KB random (CPU)

The GPU and CPU paths produce **byte-identical** compressed output (same integer
codec, deterministic). E.g. `binary_mozilla` @ 64 KB → 251,972 bytes on both.

## 4. Ratio vs segment size (CPU reference, 6-file corpus, 2,359,296 bytes)

| segment size | segments | total bytes | ratio |
|-------------:|---------:|------------:|------:|
| 393,216 (1/file) | 6   | 697,692   | 0.296 |
| 262,144          | 12  | 711,191   | 0.301 |
| 65,536           | 36  | 750,974   | 0.318 |
| 16,384           | 144 | 861,734   | 0.365 |
| 4,096            | 576 | 1,084,050 | 0.460 |

Reference points: frontier record **572,064**; xz-9e 573,460; zstd-22 599,697. The
scaled model at one-segment-per-file (697,692) is intentionally weaker than the
full 106-model frontier — the point is the *shape* of the curve, not absolute
ratio.

## 5. GPU throughput (RTX 4090, 56.6 MB input)

Corpus files are too small (≤96 segments) to fill the GPU, so throughput is
measured on the 56.6 MB synthetic file. Ratios match the CPU reference exactly.

| segment size | ratio | compress MB/s | decompress MB/s |
|-------------:|------:|--------------:|----------------:|
| 393,216 | 0.296 |  6.0 |  6.2 |
| 262,144 | 0.303 |  8.8 |  9.3 |
|  65,536 | 0.318 | 33.2 | 34.9 |
|  16,384 | 0.365 | **87.0** | **91.6** |
|   4,096 | 0.459 | 78.0 | 78.7 |

**Serial CPU reference** (same model, 1 core, seg=16,384): **7.2 MB/s.**
→ The 4090 is **~12× faster at identical compression** (ratio 0.365).

## 6. Findings

1. **Throughput peaks at ~16 KB segments (~90 MB/s) and *drops* at 4 KB.**
   Counterintuitive: more segments should mean more parallelism. The cause is that
   each segment re-initializes ~1.5 MB of per-thread state (zeroing the StateMap +
   match-hash tables in `Cm::init`). At 4 KB the init cost dominates the actual
   compression work, so throughput regresses. **This is the #1 optimization
   target** — shrink the per-thread tables (`CTX_BITS`/`MATCH_BITS`) or zero less.

2. **The speed/ratio knob is real and smooth.** Trading ratio 0.296 → 0.459 buys
   6 → ~90 MB/s. Sweet spot ≈ 16 KB (ratio 0.365, ~90 MB/s).

3. **Small inputs don't benefit.** Single 393 KB corpus files hit only 0.2–3.5
   MB/s — too few segments to fill 16k+ GPU threads, and per-kernel overhead
   dominates. Block-parallel only pays off at tens of MB and up.

4. **GPU parallelism does not change the model's quality** — only the segmentation
   does. Identical bytes out of CPU and GPU confirm the parallelization is purely a
   throughput transform, not an approximation.

## 7. Engineering notes (issues hit & fixed)

| problem | fix |
|---------|-----|
| `nvcc` ran but runtime: "forward compatibility on non-supported HW" | host driver < container toolkit; pin image to CUDA **12.2.2** and require host `cuda_vers>=12.2` |
| instances not destroyed → silent billing | `vastai destroy` prompts; add **`-y`** in non-interactive scripts |
| SSH `Permission denied (publickey)` on the proxy host | use the **direct endpoint** (`public_ipaddr` + host-mapped port 22), attach key on create |
| `illegal memory access` on the big file only | launched `grid*block` threads but allocated only `slots` state slots; allocate for **every launched thread** |

## 8. RTX 5090 (single GPU) and 8× multi-GPU scaling

Re-ran on an 8× RTX 5090 box (Blackwell, `sm_120`, CUDA 12.8, vast.ai ~$3.20/hr).
Same codec — output byte-identical to the 4090 and the CPU reference.

**Single 5090 vs 4090** (kernel-timed MB/s, 56.6 MB input):

| seg | 4090 compress | 5090 compress | 4090 decomp | 5090 decomp |
|----:|--------------:|--------------:|------------:|------------:|
| 4 KB  | 78  | 108 | 79 | 101 |
| 16 KB | 87  | **123** | 92 | 92 |
| 64 KB | 33  | 34  | 35 | 26 |

The 5090 is ~30–40% faster on compress at the sweet spot; decompress is roughly
even. Both are gated by the same per-segment overhead at larger segment sizes.

**8-GPU scaling — the optimization journey.** Compress a fixed 448 MB workload
across N GPUs, seg=16 KB. Three implementations, each fixing the next bottleneck:

*v1 — naive multi-process* (one `./gpu` process per GPU, `CUDA_VISIBLE_DEVICES`):

| GPUs | MB/s | speedup |
|--:|--:|--:|
| 1 | 28 | 1.0× |
| 8 | 80 | **2.9×** (36% eff.) |

Sub-linear. Each process pays 8× CUDA context init (8 empty processes alone took
5.6 s just to init), 8× redundant ~5 GB state alloc, and pageable transfers.

*v2 — single process, one thread per GPU, input pinned once + shared pinned output:*
removed the per-thread `cudaMallocHost` page-lock contention. 1→2 went near-linear
(134→290 MB/s) but **8 GPUs regressed to 192 MB/s** — the per-thread `cudaMalloc`
calls (including the ~1 GB state buffer) serialize on CUDA's global allocator lock,
inside the timed region.

*v3 — pre-allocate every GPU's buffers during setup (untimed); timed region is pure
transfer+compute:*

| GPUs | aggregate MB/s | scaling | efficiency |
|--:|--:|--:|--:|
| 1 | 134 | 1.0× | — |
| 2 | 290 | 2.2× | 108% |
| 4 | 618 | 4.6× | 115% |
| 8 | **1004** | **7.5×** | **94%** |

**Near-linear, >1 GB/s on 8× 5090.** (Slightly super-linear at 2–4× because the
1-GPU run is itself throttled by grid-stride serialization over 27 k segments.)
Right-sizing the StateMap tables to the segment (`CTX_BITS` 16→14, since a 16 KB
segment can't use 64 K entries) cost only ~0.7% ratio (0.3652→0.3724) and trimmed
GPU compute.

**Finding (multi-GPU):** independent processes are the wrong model — linear scaling
needs (a) one process, one host thread per GPU, (b) all host memory pinned **once**,
and critically (c) **all device allocation hoisted out of the parallel region**,
because `cudaMalloc`/`cudaMallocHost` take global driver locks that serialize
concurrent threads. With those three, 8 GPUs deliver 7.5× (94% efficiency). The
`mgpu` mode in `main.cu` implements this.

## 9. Limitations & future work

- **Achieved:** near-linear multi-GPU scaling (7.5× on 8 GPUs, >1 GB/s aggregate)
  via single-process + pin-once + pre-allocated buffers (§8).
- The single-GPU run is grid-stride-throttled over 27 k segments — capping device
  concurrency lower or one-**block**-per-segment could lift the 1-GPU baseline (and
  shrink the apparent super-linear effect).
- Per-shard fixed cost (state alloc + table upload) is now in *setup*, not the timed
  region; for a streaming service it amortizes across inputs, but a cold single call
  still pays it once.
- Scaled model (orders 0–4 + 1 match model) is well below the full frontier; not a
  ratio competitor, by design.

## 10. Reproduce

```bash
# local correctness + ratio (no GPU)
cd gpu && make cpu_ref && bash bench/run.sh

# rent a 4090, build, bench, keep it alive
MAX_DPH=0.50 remote/up.sh           # provisions, saves /tmp/cmgpu/instance.env
remote/iter.sh                      # sync + build + GPU selftest
remote/iter.sh 'cd /root/cm/gpu && bash bench/run.sh ../corpus bench/results'
remote/down.sh                      # destroy when done

# rent an 8x RTX 5090 box (Blackwell needs sm_120 + CUDA >=12.8)
GPU_NAME=RTX_5090 NUM_GPUS=8 MIN_CUDA=12.8 MAX_DPH=4.50 DISK=40 \
  IMAGE=nvidia/cuda:12.8.1-devel-ubuntu24.04 remote/up.sh
# near-linear multi-GPU scaling (one process, pinned-once, pre-allocated):
CUDA_ARCH=sm_120 remote/iter.sh 'cd /root/cm/gpu && for N in 1 2 4 8; do ./gpu mgpu /tmp/mg/big.bin 16384 $N; done'
remote/down.sh
```

Raw data: [`bench/results.gpu.csv`](bench/results.gpu.csv),
[`bench/results.cpu.csv`](bench/results.cpu.csv).
