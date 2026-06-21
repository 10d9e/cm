// Scaled lpaq-class context-mixing model for one segment.
//
// A deliberately reduced version of the frontier (src/algorithm/model.rs):
//   * NORDER StateMaps over byte-history contexts orders 0..NORDER-1
//   * one match model (predicts the next byte from a hashed earlier occurrence)
//   * a single-layer logistic mixer (weights selected by the previous byte)
//   * 16-bit binary arithmetic coding
// Enough to demonstrate the block-parallel pipeline and measure the
// speed-vs-ratio tradeoff; not the full 106-model ensemble.
//
// All mutable state is supplied by the caller (Cm::tables / scalars) so each
// GPU thread owns a private slice of one big allocation.
#pragma once
#include "common.cuh"
#include "tables.hpp"

// Per-segment model state. Table pointers reference caller-owned memory:
//   sm  : NORDER * CTX_SIZE uint32  (one StateMap per order)
//   mw  : NMIXCTX * NINPUT  int32   (mixer weights)
//   mh  : MATCH_SIZE        uint32  (match hash -> position+1, 0 = empty)
// buf is the segment bytes: the source on compress, the bytes decoded so far on
// decompress (the match model only ever reads indices < pos, valid in both).
struct Cm {
  // global (read-only) logistic tables
  const int32_t *stretch;
  const int32_t *squash16;
  const int32_t *dt;
  // per-segment tables
  uint32_t *sm;
  int32_t *mw;
  uint32_t *mh;
  const uint8_t *buf;

  // scalar state
  uint32_t c0;  // partial byte: 1..255 with a leading 1 bit
  uint32_t c4;  // last 4 whole bytes, most-recent in the low byte
  int bitpos;   // bits seen in the current byte (0..7)
  uint32_t pos; // index in buf of the byte currently being modeled

  // match model
  uint32_t matchptr; // index in buf of the predicted byte
  int matchlen;      // run length of the current match (0 = none)
  int predbyte;      // buf[matchptr] cached for this byte (-1 if no match)

  // carried from predict() to update()
  uint32_t mi[NORDER];    // StateMap index per order
  int32_t inputs[NINPUT]; // stretched mixer inputs
  int mixctx;             // selected mixer weight set
  int pr12;               // last prediction, 12-bit

  HD void init(const Tables &t, uint32_t *sm_, int32_t *mw_, uint32_t *mh_,
               const uint8_t *buf_) {
    stretch = t.stretch;
    squash16 = t.squash16;
    dt = t.dt;
    sm = sm_;
    mw = mw_;
    mh = mh_;
    buf = buf_;
    c0 = 1;
    c4 = 0;
    bitpos = 0;
    pos = 0;
    matchptr = 0;
    matchlen = 0;
    predbyte = -1;
    for (uint32_t i = 0; i < (uint32_t)NORDER * CTX_SIZE; i++)
      sm[i] = 0x80000000u;
    for (uint32_t i = 0; i < (uint32_t)NMIXCTX * NINPUT; i++)
      mw[i] = 0;
    for (uint32_t i = 0; i < MATCH_SIZE; i++)
      mh[i] = 0;
  }

  HD static uint32_t hashk(uint32_t ctxbytes, int k, uint32_t c0) {
    uint32_t h = ctxbytes * 0x9E3779B1u + (uint32_t)k * 0x85EBCA77u;
    h ^= c0 * 0xC2B2AE3Du;
    h ^= h >> 15;
    return h;
  }

  HD int predict() {
    // order-0 indexed directly by the partial byte; higher orders hashed.
    mi[0] = c0;
    inputs[0] = stretch[sm[mi[0]] >> 20];
    for (int k = 1; k < NORDER; k++) {
      uint32_t mask = (k >= 4) ? 0xffffffffu : ((1u << (8 * k)) - 1u);
      uint32_t ctxbytes = c4 & mask;
      uint32_t idx = hashk(ctxbytes, k, c0) & CTX_MASK;
      mi[k] = (uint32_t)k * CTX_SIZE + idx;
      inputs[k] = stretch[sm[mi[k]] >> 20];
    }
    // match model input
    int expbit = 0, mstr = 0;
    if (matchlen > 0 && predbyte >= 0) {
      expbit = (predbyte >> (7 - bitpos)) & 1;
      int s = matchlen * 64;
      if (s > 2047)
        s = 2047;
      mstr = expbit ? s : -s;
    }
    inputs[NORDER] = mstr;
    inputs[NORDER + 1] = 256; // bias

    // mixer: dot product of weights and stretched inputs
    mixctx = (int)(c4 & 0xff);
    int32_t *w = mw + (size_t)mixctx * NINPUT;
    int64_t sum = 0;
    for (int j = 0; j < NINPUT; j++)
      sum += (int64_t)w[j] * inputs[j];
    int d = (int)(sum >> 16);
    if (d > 2047)
      d = 2047;
    if (d < -2047)
      d = -2047;
    int p16 = squash16_d(squash16, d);
    pr12 = p16 >> 4;
    return p16;
  }

  HD void update_bit(int bit) {
    // StateMaps
    for (int k = 0; k < NORDER; k++) {
      uint32_t v = sm[mi[k]];
      int n = v & 1023;
      int p = (int)(v >> 10); // 22-bit probability
      if (n < 255)
        n++;
      int target = bit << 22;
      int delta = (int)(((int64_t)(target - p) * dt[n]) >> 16);
      p += delta;
      if (p < 0)
        p = 0;
      if (p > 0x3fffff)
        p = 0x3fffff;
      sm[mi[k]] = ((uint32_t)p << 10) | (uint32_t)n;
    }
    // mixer weights
    int err = ((bit << 12) - pr12) * 7;
    int32_t *w = mw + (size_t)mixctx * NINPUT;
    for (int j = 0; j < NINPUT; j++) {
      w[j] += (inputs[j] * err + 0x8000) >> 16;
    }
    // match validity: a wrong predicted bit ends the match for this byte
    if (matchlen > 0 && predbyte >= 0) {
      int expbit = (predbyte >> (7 - bitpos)) & 1;
      if (expbit != bit)
        matchlen = 0;
    }
    c0 = (c0 << 1) | (uint32_t)bit;
    bitpos++;
  }

  // Called after a whole byte `cb` (== buf[pos]) has been coded.
  HD void advance_byte(int cb) {
    c4 = (c4 << 8) | (uint32_t)(cb & 0xff);
    // continue or restart the match
    if (matchlen > 0 && (int)buf[matchptr] == cb) {
      matchptr++;
      matchlen++;
    } else {
      matchlen = 0;
    }
    uint32_t h = (c4 * 2654435761u) >> (32 - MATCH_BITS);
    if (matchlen == 0) {
      uint32_t cand = mh[h];
      if (cand != 0 && cand <= pos) {
        matchptr = cand;
        matchlen = 1;
      }
    }
    mh[h] = pos + 1; // context ending at pos predicts the next byte
    predbyte = (matchlen > 0) ? (int)buf[matchptr] : -1;

    pos++;
    c0 = 1;
    bitpos = 0;
  }
};
