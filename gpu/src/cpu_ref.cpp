// CPU reference for the block-parallel codec. Runs the exact same per-segment
// integer codec the GPU runs, but serially on the host. Two jobs:
//   1. prove losslessness locally (free) before spending any GPU money
//   2. produce a byte-identical container the GPU path can be checked against
//
// Usage:
//   cpu_ref c <in> <out> [seg_size]
//   cpu_ref d <in> <out>
//   cpu_ref selftest <file> [seg_size]
//   cpu_ref bench <file> [seg_size_list]   # csv: file,seg_size,orig,comp,ratio
#include "segment.cuh"
#include "container.hpp"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <vector>
#include <string>

static const uint32_t DEFAULT_SEG = 65536;

struct Scratch {
    std::vector<uint32_t> sm;
    std::vector<int32_t>  mw;
    std::vector<uint32_t> mh;
    Scratch() : sm((size_t)NORDER * CTX_SIZE), mw((size_t)NMIXCTX * NINPUT), mh(MATCH_SIZE) {}
};

static std::vector<uint8_t> read_file(const char* path) {
    FILE* f = fopen(path, "rb");
    if (!f) { fprintf(stderr, "cannot open %s\n", path); exit(1); }
    fseek(f, 0, SEEK_END); long sz = ftell(f); fseek(f, 0, SEEK_SET);
    std::vector<uint8_t> v(sz > 0 ? sz : 0);
    if (sz > 0 && fread(v.data(), 1, sz, f) != (size_t)sz) { fprintf(stderr, "read err\n"); exit(1); }
    fclose(f);
    return v;
}
static void write_file(const char* path, const std::vector<uint8_t>& v) {
    FILE* f = fopen(path, "wb");
    if (!f) { fprintf(stderr, "cannot write %s\n", path); exit(1); }
    if (!v.empty()) fwrite(v.data(), 1, v.size(), f);
    fclose(f);
}

static Container compress_buffer(const uint8_t* data, size_t n, uint32_t seg_size,
                                 const Tables& tb, Scratch& sc) {
    Container c;
    c.orig_len = n;
    c.seg_size = seg_size;
    c.seg_count = (uint32_t)((n + seg_size - 1) / seg_size);
    std::vector<uint8_t> tmp;
    for (uint32_t s = 0; s < c.seg_count; s++) {
        size_t off = (size_t)s * seg_size;
        uint32_t len = (uint32_t)((n - off < seg_size) ? (n - off) : seg_size);
        uint32_t cap = len * 2 + 64;
        tmp.resize(cap);
        uint32_t clen = compress_segment(data + off, len, tmp.data(), cap, tb,
                                         sc.sm.data(), sc.mw.data(), sc.mh.data());
        c.seg_len.push_back(clen);
        c.blobs.insert(c.blobs.end(), tmp.begin(), tmp.begin() + clen);
    }
    return c;
}

static std::vector<uint8_t> decompress_container(const std::vector<uint8_t>& v,
                                                 const Tables& tb, Scratch& sc) {
    Container c; size_t off;
    if (!container_parse(v, c, off)) { fprintf(stderr, "bad container\n"); exit(1); }
    std::vector<uint8_t> out(c.orig_len);
    for (uint32_t s = 0; s < c.seg_count; s++) {
        size_t boff = (size_t)s * c.seg_size;
        uint32_t len = (uint32_t)((c.orig_len - boff < c.seg_size) ? (c.orig_len - boff) : c.seg_size);
        decompress_segment(&v[off], c.seg_len[s], &out[boff], len, tb,
                           sc.sm.data(), sc.mw.data(), sc.mh.data());
        off += c.seg_len[s];
    }
    if (c.flag) e8e9(out.data(), out.size(), false);
    return out;
}

int main(int argc, char** argv) {
    if (argc < 3) { fprintf(stderr, "usage: cpu_ref c|d|selftest|bench ...\n"); return 1; }

    std::vector<int32_t> stretch(4096), squash16(4096), dt(256);
    tables_build(stretch.data(), squash16.data(), dt.data());
    Tables tb{stretch.data(), squash16.data(), dt.data()};
    Scratch sc;

    std::string cmd = argv[1];

    if (cmd == "c") {
        uint32_t seg = argc >= 5 ? (uint32_t)atoi(argv[4]) : DEFAULT_SEG;
        std::vector<uint8_t> data = read_file(argv[2]);
        uint8_t flag = want_e8e9(data.data(), data.size()) ? 1 : 0;
        if (flag) e8e9(data.data(), data.size(), true);
        Container c = compress_buffer(data.data(), data.size(), seg, tb, sc);
        c.flag = flag;
        write_file(argv[3], container_serialize(c));
        return 0;
    }
    if (cmd == "d") {
        std::vector<uint8_t> v = read_file(argv[2]);
        write_file(argv[3], decompress_container(v, tb, sc));
        return 0;
    }
    if (cmd == "selftest") {
        uint32_t seg = argc >= 4 ? (uint32_t)atoi(argv[3]) : DEFAULT_SEG;
        std::vector<uint8_t> orig = read_file(argv[2]);
        std::vector<uint8_t> data = orig;
        uint8_t flag = want_e8e9(data.data(), data.size()) ? 1 : 0;
        if (flag) e8e9(data.data(), data.size(), true);
        Container c = compress_buffer(data.data(), data.size(), seg, tb, sc);
        c.flag = flag;
        std::vector<uint8_t> ser = container_serialize(c);
        std::vector<uint8_t> back = decompress_container(ser, tb, sc);
        bool ok = (back == orig);
        printf("%s seg=%u orig=%zu comp=%zu ratio=%.4f %s\n",
               argv[2], seg, orig.size(), ser.size(),
               orig.empty() ? 0.0 : (double)ser.size() / orig.size(),
               ok ? "ROUNDTRIP-OK" : "ROUNDTRIP-FAIL");
        return ok ? 0 : 2;
    }
    if (cmd == "bench") {
        std::vector<uint8_t> orig = read_file(argv[2]);
        std::vector<uint8_t> data = orig;
        uint8_t flag = want_e8e9(data.data(), data.size()) ? 1 : 0;
        if (flag) e8e9(data.data(), data.size(), true);
        const char* list = argc >= 4 ? argv[3] : "4096,16384,65536,262144";
        char buf[256]; strncpy(buf, list, sizeof(buf) - 1); buf[sizeof(buf) - 1] = 0;
        for (char* tok = strtok(buf, ","); tok; tok = strtok(NULL, ",")) {
            uint32_t seg = (uint32_t)atoi(tok);
            if (seg == 0) continue;
            Container c = compress_buffer(data.data(), data.size(), seg, tb, sc);
            c.flag = flag;
            size_t comp = container_serialize(c).size();
            printf("%s,%u,%zu,%zu,%.4f\n", argv[2], seg, orig.size(), comp,
                   orig.empty() ? 0.0 : (double)comp / orig.size());
        }
        return 0;
    }
    fprintf(stderr, "unknown command %s\n", cmd.c_str());
    return 1;
}
