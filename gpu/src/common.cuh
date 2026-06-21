// Shared host/device qualifiers and tunable parameters for the block-parallel
// codec. Compiled by both nvcc (CUDA, main.cu) and a host C++ compiler
// (cpu_ref.cpp) — the same integer code runs in both, so the CPU reference and
// the GPU path produce byte-identical containers for identical parameters.
#pragma once
#include <stdint.h>

#ifdef __CUDACC__
#define HD __host__ __device__
#else
#define HD
#endif

// ---- Model size knobs (per-segment state lives in these tables) ------------
// Each context-model StateMap table is 2^CTX_BITS entries * 4 bytes.
// Smaller tables => less memory per thread => faster per-segment alloc/zero-init
// and less memory traffic. The table only needs to cover the distinct contexts
// in ONE segment, so it should track the segment size: a 16 KB segment has
// <=16K distinct byte-history contexts, so 2^14 entries is ample. At 64K (2^16)
// the tables were 4x oversized — the dominant cost in alloc-bound multi-GPU runs.
#ifndef CTX_BITS
#define CTX_BITS 14
#endif
#ifndef MATCH_BITS
#define MATCH_BITS 14
#endif

#define CTX_SIZE  (1u << CTX_BITS)
#define CTX_MASK  (CTX_SIZE - 1u)
#define MATCH_SIZE (1u << MATCH_BITS)
#define MATCH_MASK (MATCH_SIZE - 1u)

// Context model orders 0..NORDER-1 (order-0 plus NORDER-1 byte-history orders),
// plus one match-model input, plus one always-on bias input.
#define NORDER  5
#define NINPUT  (NORDER + 2)

// Mixer weight sets are selected by the previous whole byte (256 sets).
#define NMIXCTX 256
