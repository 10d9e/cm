// Logistic transform tables, mirroring src/algorithm/tables.rs.
//   stretch[p]      : 12-bit prob (0..4095)      -> logit d in [-2047, 2047]
//   squash16[d+2048]: logit d in [-2047, 2047]   -> 16-bit prob (1..65534)
//   dt[n]           : StateMap adaptation rate    = round(65536 / (n + 1.5))
// Built once on the host with doubles, then used as integer lookups in the hot
// path so the CPU reference and the GPU path stay byte-identical.
#pragma once
#include "common.cuh"
#include <math.h>
#include <stdlib.h>

struct Tables {
    int32_t* stretch;   // [4096]
    int32_t* squash16;  // [4096], indexed by d+2048
    int32_t* dt;        // [256]
};

// Fill caller-provided arrays (host). stretch:[4096] squash16:[4096] dt:[256].
static inline void tables_build(int32_t* stretch, int32_t* squash16, int32_t* dt) {
    // squash16(d) = 65536 / (1 + e^(-d/256)), clamped to [1, 65534].
    for (int i = 0; i < 4096; i++) {
        double d = (i - 2048.0) / 256.0;
        double v = 65536.0 / (1.0 + exp(-d));
        int iv = (int)(v + 0.5);
        if (iv < 1) iv = 1;
        if (iv > 65534) iv = 65534;
        squash16[i] = iv;
    }
    // stretch is the inverse over the 12-bit probability range.
    // Build a 12-bit squash first, then invert it.
    int sq12[4096];
    for (int i = 0; i < 4096; i++) {
        double d = (i - 2048.0) / 256.0;
        double v = 4096.0 / (1.0 + exp(-d));
        int iv = (int)(v + 0.5);
        if (iv < 0) iv = 0;
        if (iv > 4095) iv = 4095;
        sq12[i] = iv;
    }
    int pi = 0;
    for (int d = -2047; d <= 2047; d++) {
        int p = (d <= -2047) ? 0 : (d >= 2047 ? 4095 : sq12[d + 2048]);
        while (pi <= p && pi < 4096) { stretch[pi] = d; pi++; }
    }
    while (pi < 4096) { stretch[pi] = 2047; pi++; }

    for (int n = 0; n < 256; n++) {
        dt[n] = (int)(65536.0 / (n + 1.5) + 0.5);
    }
}

HD static inline int squash16_d(const int32_t* squash16, int d) {
    if (d >= 2047) return 65534;
    if (d <= -2047) return 1;
    return squash16[d + 2048];
}
