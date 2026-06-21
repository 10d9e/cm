// CUDA driver for the block-parallel codec: one GPU thread per segment, with a
// grid-stride loop so a bounded pool of per-thread model-state slots can cover
// arbitrarily many segments. Builds on the 4090 (sm_89); does not build on this
// Mac (no nvcc) — that's expected, it runs on the rented instance.
//
// Usage:
//   gpu c <in> <out> [seg_size]
//   gpu d <in> <out>
//   gpu selftest <file> [seg_size]
//   gpu bench <file> [seg_size_list]   # csv: file,seg_size,orig,comp,ratio,c_MBps,d_MBps
#include "segment.cuh"
#include "container.hpp"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <vector>
#include <string>
#include <thread>
#include <chrono>
#include <algorithm>

#define CUDA_CHECK(x) do { cudaError_t e = (x); if (e != cudaSuccess) { \
    fprintf(stderr, "CUDA error %s at %s:%d\n", cudaGetErrorString(e), __FILE__, __LINE__); \
    exit(1); } } while (0)

// Per-thread state-slot layout, sized by the model knobs in common.cuh.
static const size_t SM_WORDS = (size_t)NORDER * CTX_SIZE;
static const size_t MW_WORDS = (size_t)NMIXCTX * NINPUT;
static const size_t MH_WORDS = MATCH_SIZE;

struct DevState {
    uint32_t* sm;   // [nslots * SM_WORDS]
    int32_t*  mw;   // [nslots * MW_WORDS]
    uint32_t* mh;   // [nslots * MH_WORDS]
};

__global__ void compress_kernel(const uint8_t* data, uint32_t orig_len,
                                uint32_t seg_size, uint32_t seg_count,
                                uint8_t* outbuf, uint32_t outcap, uint32_t* outlen,
                                Tables tb, DevState st, int nslots) {
    int tid = blockIdx.x * blockDim.x + threadIdx.x;
    int total = gridDim.x * blockDim.x;
    for (uint32_t s = tid; s < seg_count; s += total) {
        int slot = tid; // tid < nslots by construction (we launch <= nslots threads)
        uint32_t* sm = st.sm + (size_t)slot * SM_WORDS;
        int32_t*  mw = st.mw + (size_t)slot * MW_WORDS;
        uint32_t* mh = st.mh + (size_t)slot * MH_WORDS;
        size_t off = (size_t)s * seg_size;
        uint32_t len = (orig_len - off < seg_size) ? (uint32_t)(orig_len - off) : seg_size;
        uint8_t* out = outbuf + (size_t)s * outcap;
        outlen[s] = compress_segment(data + off, len, out, outcap, tb, sm, mw, mh);
    }
}

__global__ void decompress_kernel(const uint8_t* blobs, const uint32_t* blob_off,
                                  const uint32_t* seg_len, uint32_t seg_size,
                                  uint32_t seg_count, uint32_t orig_len,
                                  uint8_t* out, Tables tb, DevState st, int nslots) {
    int tid = blockIdx.x * blockDim.x + threadIdx.x;
    int total = gridDim.x * blockDim.x;
    for (uint32_t s = tid; s < seg_count; s += total) {
        int slot = tid;
        uint32_t* sm = st.sm + (size_t)slot * SM_WORDS;
        int32_t*  mw = st.mw + (size_t)slot * MW_WORDS;
        uint32_t* mh = st.mh + (size_t)slot * MH_WORDS;
        size_t boff = (size_t)s * seg_size;
        uint32_t len = (orig_len - boff < seg_size) ? (uint32_t)(orig_len - boff) : seg_size;
        decompress_segment(blobs + blob_off[s], seg_len[s], out + boff, len, tb, sm, mw, mh);
    }
}

// ---- host helpers -----------------------------------------------------------
static std::vector<uint8_t> read_file(const char* p) {
    FILE* f = fopen(p, "rb"); if (!f) { fprintf(stderr, "open %s\n", p); exit(1); }
    fseek(f, 0, SEEK_END); long sz = ftell(f); fseek(f, 0, SEEK_SET);
    std::vector<uint8_t> v(sz > 0 ? sz : 0);
    if (sz > 0) { size_t r = fread(v.data(), 1, sz, f); (void)r; }
    fclose(f); return v;
}
static void write_file(const char* p, const std::vector<uint8_t>& v) {
    FILE* f = fopen(p, "wb"); if (!f) { fprintf(stderr, "write %s\n", p); exit(1); }
    if (!v.empty()) fwrite(v.data(), 1, v.size(), f); fclose(f);
}

// Upload tables once; returns a device Tables view.
static Tables upload_tables(int32_t** keep) {
    int32_t hs[4096], hq[4096], hd[256];
    tables_build(hs, hq, hd);
    int32_t *ds, *dq, *dd;
    CUDA_CHECK(cudaMalloc(&ds, sizeof(hs)));
    CUDA_CHECK(cudaMalloc(&dq, sizeof(hq)));
    CUDA_CHECK(cudaMalloc(&dd, sizeof(hd)));
    CUDA_CHECK(cudaMemcpy(ds, hs, sizeof(hs), cudaMemcpyHostToDevice));
    CUDA_CHECK(cudaMemcpy(dq, hq, sizeof(hq), cudaMemcpyHostToDevice));
    CUDA_CHECK(cudaMemcpy(dd, hd, sizeof(hd), cudaMemcpyHostToDevice));
    keep[0] = ds; keep[1] = dq; keep[2] = dd;
    return Tables{ds, dq, dd};
}

// Choose how many state slots (== resident threads) fit a memory budget.
static int choose_slots(uint32_t seg_count) {
    size_t per = (SM_WORDS + MH_WORDS) * 4 + MW_WORDS * 4;
    size_t freeb, totalb;
    CUDA_CHECK(cudaMemGetInfo(&freeb, &totalb));
    size_t budget = (size_t)(freeb * 0.6); // leave room for data + out buffers
    int slots = (int)(budget / per);
    if (slots < 1) slots = 1;
    if ((uint32_t)slots > seg_count) slots = (int)seg_count;
    if (slots > 1 << 16) slots = 1 << 16;
    return slots;
}

// Compress a (already e8e9-filtered) buffer on the GPU into a Container.
// If timing != null, writes compress MB/s.
static Container gpu_compress(const uint8_t* data, size_t n, uint32_t seg_size,
                             Tables tb, double* mbps) {
    Container c; c.orig_len = n; c.seg_size = seg_size;
    c.seg_count = (uint32_t)((n + seg_size - 1) / seg_size);
    if (c.seg_count == 0) return c;

    uint32_t outcap = seg_size * 2 + 64;
    // grid*block rounds the thread count up to a multiple of `block`; allocate a
    // state slot for EVERY launched thread (slot == tid) or the extra rounding
    // threads read out of bounds when seg_count > slots.
    int block = 128;
    int grid = (choose_slots(c.seg_count) + block - 1) / block;
    int total = grid * block;

    uint8_t* d_data; CUDA_CHECK(cudaMalloc(&d_data, n));
    CUDA_CHECK(cudaMemcpy(d_data, data, n, cudaMemcpyHostToDevice));
    uint8_t* d_out; CUDA_CHECK(cudaMalloc(&d_out, (size_t)c.seg_count * outcap));
    uint32_t* d_len; CUDA_CHECK(cudaMalloc(&d_len, c.seg_count * sizeof(uint32_t)));
    DevState st;
    CUDA_CHECK(cudaMalloc(&st.sm, (size_t)total * SM_WORDS * 4));
    CUDA_CHECK(cudaMalloc(&st.mw, (size_t)total * MW_WORDS * 4));
    CUDA_CHECK(cudaMalloc(&st.mh, (size_t)total * MH_WORDS * 4));

    cudaEvent_t t0, t1; cudaEventCreate(&t0); cudaEventCreate(&t1);
    cudaEventRecord(t0);
    compress_kernel<<<grid, block>>>(d_data, (uint32_t)n, seg_size, c.seg_count,
                                     d_out, outcap, d_len, tb, st, total);
    cudaEventRecord(t1); CUDA_CHECK(cudaDeviceSynchronize());
    float ms = 0; cudaEventElapsedTime(&ms, t0, t1);
    if (mbps) *mbps = (n / 1e6) / (ms / 1e3);

    std::vector<uint32_t> lens(c.seg_count);
    CUDA_CHECK(cudaMemcpy(lens.data(), d_len, c.seg_count * 4, cudaMemcpyDeviceToHost));
    std::vector<uint8_t> allout((size_t)c.seg_count * outcap);
    CUDA_CHECK(cudaMemcpy(allout.data(), d_out, allout.size(), cudaMemcpyDeviceToHost));
    for (uint32_t s = 0; s < c.seg_count; s++) {
        c.seg_len.push_back(lens[s]);
        c.blobs.insert(c.blobs.end(), allout.begin() + (size_t)s * outcap,
                       allout.begin() + (size_t)s * outcap + lens[s]);
    }
    cudaFree(d_data); cudaFree(d_out); cudaFree(d_len);
    cudaFree(st.sm); cudaFree(st.mw); cudaFree(st.mh);
    return c;
}

static std::vector<uint8_t> gpu_decompress(const std::vector<uint8_t>& v,
                                          Tables tb, double* mbps) {
    Container c; size_t off;
    if (!container_parse(v, c, off)) { fprintf(stderr, "bad container\n"); exit(1); }
    std::vector<uint8_t> out(c.orig_len);
    if (c.seg_count == 0) { if (c.flag) e8e9(out.data(), out.size(), false); return out; }

    std::vector<uint32_t> blob_off(c.seg_count);
    size_t run = 0;
    for (uint32_t s = 0; s < c.seg_count; s++) { blob_off[s] = (uint32_t)run; run += c.seg_len[s]; }

    int block = 128;
    int grid = (choose_slots(c.seg_count) + block - 1) / block;
    int total = grid * block; // allocate a state slot for every launched thread

    uint8_t* d_blobs; CUDA_CHECK(cudaMalloc(&d_blobs, run));
    CUDA_CHECK(cudaMemcpy(d_blobs, &v[off], run, cudaMemcpyHostToDevice));
    uint32_t* d_boff; CUDA_CHECK(cudaMalloc(&d_boff, c.seg_count * 4));
    CUDA_CHECK(cudaMemcpy(d_boff, blob_off.data(), c.seg_count * 4, cudaMemcpyHostToDevice));
    uint32_t* d_slen; CUDA_CHECK(cudaMalloc(&d_slen, c.seg_count * 4));
    CUDA_CHECK(cudaMemcpy(d_slen, c.seg_len.data(), c.seg_count * 4, cudaMemcpyHostToDevice));
    uint8_t* d_out; CUDA_CHECK(cudaMalloc(&d_out, c.orig_len ? c.orig_len : 1));
    DevState st;
    CUDA_CHECK(cudaMalloc(&st.sm, (size_t)total * SM_WORDS * 4));
    CUDA_CHECK(cudaMalloc(&st.mw, (size_t)total * MW_WORDS * 4));
    CUDA_CHECK(cudaMalloc(&st.mh, (size_t)total * MH_WORDS * 4));

    cudaEvent_t t0, t1; cudaEventCreate(&t0); cudaEventCreate(&t1);
    cudaEventRecord(t0);
    decompress_kernel<<<grid, block>>>(d_blobs, d_boff, d_slen, c.seg_size,
                                       c.seg_count, (uint32_t)c.orig_len, d_out, tb, st, total);
    cudaEventRecord(t1); CUDA_CHECK(cudaDeviceSynchronize());
    float ms = 0; cudaEventElapsedTime(&ms, t0, t1);
    if (mbps) *mbps = (c.orig_len / 1e6) / (ms / 1e3);

    CUDA_CHECK(cudaMemcpy(out.data(), d_out, c.orig_len, cudaMemcpyDeviceToHost));
    if (c.flag) e8e9(out.data(), out.size(), false);
    cudaFree(d_blobs); cudaFree(d_boff); cudaFree(d_slen); cudaFree(d_out);
    cudaFree(st.sm); cudaFree(st.mw); cudaFree(st.mh);
    return out;
}

// ---- single-process multi-GPU compress -------------------------------------
// One host thread per GPU, each with its OWN device tables/state/stream and
// pinned host staging buffers. Avoids the multi-process killers: 8x CUDA context
// init, 8x redundant state allocation, and pageable (slow, non-overlapping)
// transfers. Each thread owns a contiguous shard of segments; outputs assemble
// in order into one container.
// Per-GPU resources, allocated ONCE during setup (outside the timed region).
// cudaMalloc takes a global driver lock, so allocating inside the parallel timed
// region serializes the threads — the multi-GPU scaling killer. Pre-allocating
// makes the timed region pure transfer+compute.
struct DevRes {
    int dev;
    Tables tb; int32_t* tk[3];
    uint8_t *d_data, *d_out; uint32_t* d_len; DevState st;
    cudaStream_t stream;
    uint32_t s0, segN; size_t boff, blen; int grid, total;
};

// Timed: transfer + kernel + transfer on pre-allocated buffers. No allocation.
static void worker_run(DevRes* r, const uint8_t* in_pinned, uint32_t seg_size,
                       uint32_t outcap, uint32_t* lens_out, uint8_t* blob_out, double* ms) {
    cudaSetDevice(r->dev);
    auto t0 = std::chrono::steady_clock::now();
    cudaMemcpyAsync(r->d_data, in_pinned + r->boff, r->blen, cudaMemcpyHostToDevice, r->stream);
    compress_kernel<<<r->grid, 128, 0, r->stream>>>(r->d_data, (uint32_t)r->blen, seg_size,
        r->segN, r->d_out, outcap, r->d_len, r->tb, r->st, r->total);
    cudaMemcpyAsync(lens_out, r->d_len, r->segN * sizeof(uint32_t), cudaMemcpyDeviceToHost, r->stream);
    cudaMemcpyAsync(blob_out, r->d_out, (size_t)r->segN * outcap, cudaMemcpyDeviceToHost, r->stream);
    cudaStreamSynchronize(r->stream);
    auto t1 = std::chrono::steady_clock::now();
    *ms = std::chrono::duration<double, std::milli>(t1 - t0).count();
}

static void cmd_mgpu(const char* path, uint32_t seg_size, int ngpu_req) {
    std::vector<uint8_t> orig = read_file(path);
    std::vector<uint8_t> data = orig;
    uint8_t flag = want_e8e9(data.data(), data.size()) ? 1 : 0;
    if (flag) e8e9(data.data(), data.size(), true);
    size_t n = data.size();

    int devcount = 0; cudaGetDeviceCount(&devcount);
    int N = ngpu_req > 0 ? std::min(ngpu_req, devcount) : devcount;
    if (N < 1) N = 1;
    uint32_t total_segs = (uint32_t)((n + seg_size - 1) / seg_size);
    uint32_t per = (total_segs + N - 1) / N;
    uint32_t outcap = seg_size * 2 + 64;

    int32_t hs[4096], hq[4096], hd[256]; tables_build(hs, hq, hd);

    // ---- SETUP (untimed): pin host buffers + allocate every GPU's buffers ----
    cudaHostRegister(data.data(), n, cudaHostRegisterDefault);
    uint8_t* pin_out; uint32_t* pin_len;
    cudaMallocHost(&pin_out, (size_t)total_segs * outcap);
    cudaMallocHost(&pin_len, (size_t)total_segs * sizeof(uint32_t));

    std::vector<DevRes> res;
    for (int d = 0; d < N; d++) {
        uint32_t s0 = (uint32_t)d * per;
        if (s0 >= total_segs) break;
        DevRes r; r.dev = d; r.s0 = s0;
        r.segN = std::min(per, total_segs - s0);
        r.boff = (size_t)s0 * seg_size;
        r.blen = std::min((size_t)r.segN * seg_size, n - r.boff);
        r.grid = (choose_slots(r.segN) + 127) / 128;
        r.total = r.grid * 128;
        cudaSetDevice(d);
        cudaMalloc(&r.tk[0], sizeof(hs)); cudaMalloc(&r.tk[1], sizeof(hq)); cudaMalloc(&r.tk[2], sizeof(hd));
        cudaMemcpy(r.tk[0], hs, sizeof(hs), cudaMemcpyHostToDevice);
        cudaMemcpy(r.tk[1], hq, sizeof(hq), cudaMemcpyHostToDevice);
        cudaMemcpy(r.tk[2], hd, sizeof(hd), cudaMemcpyHostToDevice);
        r.tb = Tables{r.tk[0], r.tk[1], r.tk[2]};
        cudaMalloc(&r.d_data, r.blen);
        cudaMalloc(&r.d_out, (size_t)r.segN * outcap);
        cudaMalloc(&r.d_len, r.segN * sizeof(uint32_t));
        cudaMalloc(&r.st.sm, (size_t)r.total * SM_WORDS * 4);
        cudaMalloc(&r.st.mw, (size_t)r.total * MW_WORDS * 4);
        cudaMalloc(&r.st.mh, (size_t)r.total * MH_WORDS * 4);
        cudaStreamCreate(&r.stream);
        res.push_back(r);
    }

    // ---- TIMED: pure transfer + compute, one thread per GPU -----------------
    std::vector<double> ms(res.size(), 0.0);
    std::vector<std::thread> th;
    auto t0 = std::chrono::steady_clock::now();
    for (size_t i = 0; i < res.size(); i++)
        th.emplace_back(worker_run, &res[i], data.data(), seg_size, outcap,
                        pin_len + res[i].s0, pin_out + (size_t)res[i].s0 * outcap, &ms[i]);
    for (auto& t : th) t.join();
    auto t1 = std::chrono::steady_clock::now();
    double wall = std::chrono::duration<double, std::milli>(t1 - t0).count() / 1e3;

    // assemble in global segment order + verify via the single-GPU decompress path
    Container c; c.orig_len = n; c.seg_size = seg_size; c.seg_count = total_segs; c.flag = flag;
    c.seg_len.assign(pin_len, pin_len + total_segs);
    for (uint32_t g = 0; g < total_segs; g++)
        c.blobs.insert(c.blobs.end(), pin_out + (size_t)g * outcap,
                       pin_out + (size_t)g * outcap + pin_len[g]);
    std::vector<uint8_t> ser = container_serialize(c);

    for (auto& r : res) {
        cudaSetDevice(r.dev);
        cudaFree(r.d_data); cudaFree(r.d_out); cudaFree(r.d_len);
        cudaFree(r.st.sm); cudaFree(r.st.mw); cudaFree(r.st.mh);
        cudaFree(r.tk[0]); cudaFree(r.tk[1]); cudaFree(r.tk[2]);
        cudaStreamDestroy(r.stream);
    }
    cudaHostUnregister(data.data());
    cudaFreeHost(pin_out); cudaFreeHost(pin_len);

    cudaSetDevice(0);
    int32_t* keep[3]; Tables tbv = upload_tables(keep);
    std::vector<uint8_t> back = gpu_decompress(ser, tbv, nullptr);
    bool ok = (back == orig);

    double maxms = 0; for (double m : ms) maxms = std::max(maxms, m);
    printf("mgpu: gpus=%d seg=%u in=%zuMB comp=%zu ratio=%.4f wall=%.3fs "
           "aggregate=%.1f MB/s (slowest-shard %.0fms) roundtrip=%s\n",
           (int)res.size(), seg_size, n / 1000000, ser.size(), (double)ser.size() / n, wall,
           (n / 1e6) / wall, maxms, ok ? "OK" : "FAIL");
}

int main(int argc, char** argv) {
    if (argc < 3) { fprintf(stderr, "usage: gpu c|d|selftest|bench|mgpu ...\n"); return 1; }
    if (std::string(argv[1]) == "mgpu") {   // gpu mgpu <file> <seg> [ngpu]
        uint32_t seg = argc >= 4 ? (uint32_t)atoi(argv[3]) : 16384;
        int ngpu = argc >= 5 ? atoi(argv[4]) : 0;  // 0 = all visible GPUs
        cmd_mgpu(argv[2], seg, ngpu);
        return 0;
    }
    int32_t* keep[3];
    Tables tb = upload_tables(keep);
    std::string cmd = argv[1];

    if (cmd == "c") {
        uint32_t seg = argc >= 5 ? (uint32_t)atoi(argv[4]) : 65536;
        std::vector<uint8_t> data = read_file(argv[2]);
        uint8_t flag = want_e8e9(data.data(), data.size()) ? 1 : 0;
        if (flag) e8e9(data.data(), data.size(), true);
        Container c = gpu_compress(data.data(), data.size(), seg, tb, nullptr);
        c.flag = flag;
        write_file(argv[3], container_serialize(c));
        return 0;
    }
    if (cmd == "d") {
        std::vector<uint8_t> v = read_file(argv[2]);
        write_file(argv[3], gpu_decompress(v, tb, nullptr));
        return 0;
    }
    if (cmd == "selftest") {
        uint32_t seg = argc >= 4 ? (uint32_t)atoi(argv[3]) : 65536;
        std::vector<uint8_t> orig = read_file(argv[2]);
        std::vector<uint8_t> data = orig;
        uint8_t flag = want_e8e9(data.data(), data.size()) ? 1 : 0;
        if (flag) e8e9(data.data(), data.size(), true);
        Container c = gpu_compress(data.data(), data.size(), seg, tb, nullptr);
        c.flag = flag;
        std::vector<uint8_t> ser = container_serialize(c);
        std::vector<uint8_t> back = gpu_decompress(ser, tb, nullptr);
        bool ok = (back == orig);
        printf("%s seg=%u orig=%zu comp=%zu ratio=%.4f %s\n", argv[2], seg, orig.size(),
               ser.size(), orig.empty() ? 0.0 : (double)ser.size() / orig.size(),
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
            uint32_t seg = (uint32_t)atoi(tok); if (!seg) continue;
            double cmbps = 0, dmbps = 0;
            Container c = gpu_compress(data.data(), data.size(), seg, tb, &cmbps);
            c.flag = flag;
            std::vector<uint8_t> ser = container_serialize(c);
            std::vector<uint8_t> back = gpu_decompress(ser, tb, &dmbps);
            bool ok = (back == orig);
            printf("%s,%u,%zu,%zu,%.4f,%.1f,%.1f,%s\n", argv[2], seg, orig.size(), ser.size(),
                   orig.empty() ? 0.0 : (double)ser.size() / orig.size(), cmbps, dmbps,
                   ok ? "ok" : "FAIL");
        }
        return 0;
    }
    fprintf(stderr, "unknown command %s\n", cmd.c_str());
    return 1;
}
