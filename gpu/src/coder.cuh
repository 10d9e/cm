// Carry-less 32-bit binary arithmetic coder — a buffer-backed port of
// src/algorithm/coder.rs (so each GPU thread can encode into its own output
// slot). Probabilities are 16-bit P(bit==1) in [1, 65534].
#pragma once
#include "common.cuh"

struct Encoder {
    uint32_t x1, x2;
    uint8_t* out;   // caller-owned output buffer for this segment
    uint32_t cap;   // capacity in bytes
    uint32_t len;   // bytes written (may exceed cap on overflow — caller checks)

    HD void init(uint8_t* buf, uint32_t capacity) {
        x1 = 0; x2 = 0xffffffffu; out = buf; cap = capacity; len = 0;
    }
    HD void put(uint8_t b) {
        if (len < cap) out[len] = b;
        len++;
    }
    HD void encode(int p, int bit) {
        if (p < 1) p = 1;
        if (p > 65534) p = 65534;
        uint64_t range = (uint64_t)(x2 - x1);
        uint32_t xmid = x1 + (uint32_t)((range * (uint64_t)p) >> 16);
        if (bit) x2 = xmid; else x1 = xmid + 1;
        while (((x1 ^ x2) & 0xff000000u) == 0) {
            put((uint8_t)(x2 >> 24));
            x1 <<= 8;
            x2 = (x2 << 8) | 255;
        }
    }
    HD void finish() {
        put((uint8_t)(x1 >> 24));
        put((uint8_t)(x1 >> 16));
        put((uint8_t)(x1 >> 8));
        put((uint8_t)(x1));
    }
};

struct Decoder {
    uint32_t x1, x2, x;
    const uint8_t* inp;
    uint32_t cap, pos;

    HD void init(const uint8_t* buf, uint32_t capacity) {
        x1 = 0; x2 = 0xffffffffu; x = 0; inp = buf; cap = capacity; pos = 0;
        for (int i = 0; i < 4; i++) x = (x << 8) | getc();
    }
    HD uint32_t getc() {
        if (pos < cap) return inp[pos++];
        return 255; // past-end padding matches reference
    }
    HD int decode(int p) {
        if (p < 1) p = 1;
        if (p > 65534) p = 65534;
        uint64_t range = (uint64_t)(x2 - x1);
        uint32_t xmid = x1 + (uint32_t)((range * (uint64_t)p) >> 16);
        int bit = (x <= xmid) ? 1 : 0;
        if (bit) x2 = xmid; else x1 = xmid + 1;
        while (((x1 ^ x2) & 0xff000000u) == 0) {
            x1 <<= 8;
            x2 = (x2 << 8) | 255;
            x = (x << 8) | getc();
        }
        return bit;
    }
};
