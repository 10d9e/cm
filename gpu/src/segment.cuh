// Serial compress/decompress of one independent segment. This is the body each
// GPU thread runs (one thread per segment); on the host it is the CPU reference.
#pragma once
#include "common.cuh"
#include "tables.hpp"
#include "coder.cuh"
#include "model.cuh"

// Compress `in[0..n)` into `out` (capacity outcap). Returns the byte length
// (may exceed outcap on overflow; the caller sizes out at n*2+64 which is safe).
HD static inline uint32_t compress_segment(
        const uint8_t* in, uint32_t n, uint8_t* out, uint32_t outcap,
        const Tables& tb, uint32_t* sm, int32_t* mw, uint32_t* mh) {
    Cm cm; cm.init(tb, sm, mw, mh, in);
    Encoder enc; enc.init(out, outcap);
    for (uint32_t i = 0; i < n; i++) {
        int ch = in[i];
        for (int b = 7; b >= 0; b--) {
            int bit = (ch >> b) & 1;
            int p = cm.predict();
            enc.encode(p, bit);
            cm.update_bit(bit);
        }
        cm.advance_byte(ch);
    }
    enc.finish();
    return enc.len;
}

// Decompress `cin[0..cn)` (one segment's blob) into `out[0..n)`.
HD static inline void decompress_segment(
        const uint8_t* cin, uint32_t cn, uint8_t* out, uint32_t n,
        const Tables& tb, uint32_t* sm, int32_t* mw, uint32_t* mh) {
    Cm cm; cm.init(tb, sm, mw, mh, out);
    Decoder dec; dec.init(cin, cn);
    for (uint32_t k = 0; k < n; k++) {
        int byte = 0;
        for (int b = 0; b < 8; b++) {
            int p = cm.predict();
            int bit = dec.decode(p);
            cm.update_bit(bit);
            byte = (byte << 1) | bit;
        }
        out[k] = (uint8_t)byte;   // visible to the match model before advance
        cm.advance_byte(byte);
    }
}
