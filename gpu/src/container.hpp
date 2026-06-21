// Host-side container format + x86 BCJ (e8e9) filter for the block-parallel
// codec. The e8e9 filter is applied once over the whole buffer before splitting
// (mirrors src/algorithm/mod.rs) and reversed after segments are reassembled.
//
// Layout:  "GPCM" | orig_len:u64 | flag:u8 | seg_size:u32 | seg_count:u32
//          | seg_comp_len[seg_count]:u32 | blob_0 ... blob_{count-1}
#pragma once
#include <stdint.h>
#include <vector>
#include <string>

static const uint8_t GPCM_MAGIC[4] = {'G', 'P', 'C', 'M'};

struct Container {
    uint64_t orig_len = 0;
    uint8_t  flag = 0;
    uint32_t seg_size = 0;
    uint32_t seg_count = 0;
    std::vector<uint32_t> seg_len;     // compressed length of each segment
    std::vector<uint8_t>  blobs;       // concatenated compressed segments
};

// ---- x86 BCJ filter (reversible) -------------------------------------------
static inline void e8e9(uint8_t* b, size_t n, bool enc) {
    if (n < 5) return;
    size_t i = 0;
    while (i + 4 < n) {
        if (b[i] == 0xE8 || b[i] == 0xE9) {
            int32_t v = (int32_t)(b[i+1] | (b[i+2] << 8) | (b[i+3] << 16) | (b[i+4] << 24));
            int32_t p = (int32_t)i + 1 + 4;
            int32_t nv = enc ? (v + p) : (v - p);
            b[i+1] = (uint8_t)nv; b[i+2] = (uint8_t)(nv >> 8);
            b[i+3] = (uint8_t)(nv >> 16); b[i+4] = (uint8_t)(nv >> 24);
            i += 5;
        } else {
            i += 1;
        }
    }
}

static inline bool want_e8e9(const uint8_t* b, size_t n) {
    if (n >= 4 && b[0] == 'M' && b[1] == 'Z') return true;
    if (n >= 4 && b[0] == 0x7f && b[1] == 'E' && b[2] == 'L' && b[3] == 'F') return true;
    size_t lim = n < (1u << 20) ? n : (1u << 20);
    size_t cnt = 0;
    for (size_t i = 0; i < lim; i++) if (b[i] == 0xE8 || b[i] == 0xE9) cnt++;
    return cnt * 200 > lim;
}

// ---- little-endian helpers --------------------------------------------------
static inline void put_u32(std::vector<uint8_t>& v, uint32_t x) {
    v.push_back(x); v.push_back(x >> 8); v.push_back(x >> 16); v.push_back(x >> 24);
}
static inline void put_u64(std::vector<uint8_t>& v, uint64_t x) {
    for (int i = 0; i < 8; i++) v.push_back((uint8_t)(x >> (8 * i)));
}
static inline uint32_t get_u32(const uint8_t* p) {
    return p[0] | (p[1] << 8) | (p[2] << 16) | ((uint32_t)p[3] << 24);
}
static inline uint64_t get_u64(const uint8_t* p) {
    uint64_t x = 0; for (int i = 0; i < 8; i++) x |= (uint64_t)p[i] << (8 * i); return x;
}

static inline std::vector<uint8_t> container_serialize(const Container& c) {
    std::vector<uint8_t> v;
    v.insert(v.end(), GPCM_MAGIC, GPCM_MAGIC + 4);
    put_u64(v, c.orig_len);
    v.push_back(c.flag);
    put_u32(v, c.seg_size);
    put_u32(v, c.seg_count);
    for (uint32_t i = 0; i < c.seg_count; i++) put_u32(v, c.seg_len[i]);
    v.insert(v.end(), c.blobs.begin(), c.blobs.end());
    return v;
}

// Parse header + segment table; sets blob_offset to where blobs begin.
static inline bool container_parse(const std::vector<uint8_t>& v, Container& c,
                                   size_t& blob_offset) {
    if (v.size() < 4 + 8 + 1 + 4 + 4) return false;
    if (v[0] != 'G' || v[1] != 'P' || v[2] != 'C' || v[3] != 'M') return false;
    size_t o = 4;
    c.orig_len = get_u64(&v[o]); o += 8;
    c.flag = v[o]; o += 1;
    c.seg_size = get_u32(&v[o]); o += 4;
    c.seg_count = get_u32(&v[o]); o += 4;
    if (v.size() < o + (size_t)c.seg_count * 4) return false;
    c.seg_len.resize(c.seg_count);
    for (uint32_t i = 0; i < c.seg_count; i++) { c.seg_len[i] = get_u32(&v[o]); o += 4; }
    blob_offset = o;
    return true;
}
