//! Context-mixing predictor: multi-order adaptive counters, a learned match
//! model, a logistic mixer, and a two-stage APM/SSE.
//!
//! THIS IS THE PRIMARY EDITABLE SURFACE. Change models, add models, retune,
//! restructure — anything goes, provided `compress`/`decompress` remain exactly
//! lossless on all inputs and the predict/update sequence stays identical
//! between encode and decode.

use super::ctw::Ctw;
use super::dmc::Dmc;
use super::tables::{build, build16, squash16_d, squash_d};

const NCTX: usize = 106; // orders + word/n-gram + sparse + 2D + record + indirect + run + nest + nibble + text shape/layout
                         // Mixer input layout:
                         //   [0 .. NCTX)            direct adaptive counters
                         //   [SM_BASE .. SM_BASE+NCTX) bit-history StateMap predictions (one per context)
                         //   [MM_BASE .. MM_BASE+5)  five match models (order-6, -8, -10, -12, -14)
const SM_BASE: usize = NCTX;
const MM_BASE: usize = 2 * NCTX;
const DMC_IN: usize = 2 * NCTX + 6; // DMC variable-order prediction (one extra input)
const DMC2_IN: usize = 2 * NCTX + 7; // second, slow-cloning DMC prediction
const CTW_IN: usize = 2 * NCTX + 8; // Context Tree Weighting prediction
const DMC3_IN: usize = 2 * NCTX + 9; // third DMC (clone threshold 3) — was a dead zero slot
const DMC4_IN: usize = 2 * NCTX + 10; // fourth DMC (clone threshold 5) — was a dead zero slot
const DMC5_IN: usize = 2 * NCTX + 11; // fifth DMC (clone threshold 8, slow/stable, low-order)
// paq8-style RunContextMap bank: one run map per listed context model, sharing
// that model's byte-context hash (so each context carries BOTH a bit-history
// StateMap and a run map, exactly as paq8's ContextMap does). Each remembers the
// last byte that followed the context and how many times in a row it has, and
// predicts that byte's bits with confidence learned per (run-length, bit).
const NRUN: usize = 27;
// orders 2-9, word; high orders 11/10/13/16; word bi/tri-grams; sparse b2-3,
// stride-2/3/4, gap(1,3)/(1,4); + gap(1,5), word+literal, stride-5/6/7, order-5-alt.
// Run tables are size-gated to 2^20 (corpus) so the heavier 27-bank still records
// under the CI Scorekeeper's preemption/memory window.
const RUN_CTX: [usize; NRUN] = [
    2, 3, 4, 5, 6, 7, 9, 10, 11, 18, 26, 27, 28, 23, 25, 8, 19, 29, 30, 20, 21, 22, 24, 31, 32, 33,
    17,
];
const RUN_BASE: usize = 2 * NCTX + 12; // first run-map mixer input
// Run-map StateMap: count-based adaptation (fast early, asymptotes to ~1/(CAP+K)).
// A fixed-shift sweep found slow (>>8) best, so CAP keeps the asymptotic rate slow
// while still learning quickly on freshly-hit (run-length,bit) cells.
const RUN_SM_K: i32 = 1;
const RUN_SM_CAP: i32 = 255;
// Match-model StateMaps: same count-based 1/(cnt+K) adaptation as the run maps.
const MM_SM_K: i32 = 1;
const MM_SM_CAP: i32 = 511;
const NINPUT: usize = 2 * NCTX + 12 + NRUN;
const TBITS: u32 = 20; // default per-model context-table size (2^TBITS slots)
const MIXCTX: usize = 16384;
const NGLN_HS: usize = 1; // true-halfspace GLN specialists (independent hyperplane sets)
const NL1: usize = 22 + NGLN_HS + 10; // + difficulty-regime + CTW-hit-rate specialists
// GLN-style specialist: gated by GLN_BITS *true* halfspaces over GLN_SEL base
// predictions. Each gate bit is sign(<fixed pseudo-random ±1 hyperplane, preds>),
// i.e. a weighted-agreement direction (the Veness GLN gate), not a single sign.
const GLN_BITS: u32 = 14;
const GLN_NSEL: usize = 22; // base predictions fed to the gate
const GLN_SEL: [usize; GLN_NSEL] = [
    0, 1, 2, 3, 4, 5, 6, // order-0..6 direct counters
    SM_BASE, SM_BASE + 1, SM_BASE + 2, SM_BASE + 3, SM_BASE + 4, SM_BASE + 5, SM_BASE + 6, // StateMaps
    MM_BASE, MM_BASE + 1, MM_BASE + 2, MM_BASE + 3, MM_BASE + 4, MM_BASE + 5, // 6 match models
    DMC_IN, CTW_IN,
];
const L1LR: i32 = 8; // layer-1 specialist learning rate
const L2LR: i32 = 8; // layer-2 combiner learning rate
const MIX3CTX: usize = 8192; // order-2 specialist rows
const MIX4CTX: usize = 8192; // order-3 specialist rows
const FBITS: u32 = 23; // indirect order-3/-4/-5/-6 follow-history hash table bits
const WAYS: usize = 8; // set-associative ways for context tables
const WAYS_LOG: u32 = 3; // log2(WAYS)
const FSIZE: usize = 1 << FBITS;
const MMBITS: u32 = 23;
const MMSIZE: usize = 1 << MMBITS;
const MMBITS2: u32 = 23;
const MMSIZE2: usize = 1 << MMBITS2;
const MMBITS3: u32 = 22;
const MMSIZE3: usize = 1 << MMBITS3;
const MMBITS4: u32 = 23;
const MMSIZE4: usize = 1 << MMBITS4;
const MMBITS5: u32 = 23;
const MMSIZE5: usize = 1 << MMBITS5;
const MMBITS6: u32 = 23; // order-4 (short) match model
const MMSIZE6: usize = 1 << MMBITS6;
const APM_S: usize = 33;
const CNT_LIMIT: i32 = 254;
const RATE_FLOOR: i32 = 16;

/// One context-table slot, packed to 5 bytes so an 8-way bucket fits in a single
/// cache line (array-of-structs). Fields: `cp` probability-2048, `cn` count,
/// `st` bit-history state, `ck` collision checksum (associative models only).
#[derive(Clone, Copy)]
#[repr(C, packed)]
struct Slot {
    cp: i16,
    cn: u8,
    st: u8,
    ck: u8,
}

#[inline]
fn hashk(h: u32, x: u32) -> u32 {
    h.wrapping_add(x).wrapping_add(1).wrapping_mul(2654435761)
}

/// Pack the letter/digit/space/other class (2 bits each) of the four bytes in
/// `c4` into an 8-bit signature (0..255).
#[inline]
fn cls4(c4: u32) -> u32 {
    let cl = |b: u32| -> u32 {
        let b = b & 0xff;
        if (b >= 97 && b <= 122) || (b >= 65 && b <= 90) {
            1
        } else if b >= 48 && b <= 57 {
            2
        } else if b == 32 || b == 9 || b == 10 || b == 13 {
            3
        } else {
            0
        }
    };
    cl(c4) | (cl(c4 >> 8) << 2) | (cl(c4 >> 16) << 4) | (cl(c4 >> 24) << 6)
}

/// Nonstationary bit-history state transition. The state byte packs two bounded
/// counts (n0 in the high nibble, n1 in the low nibble, each 0..15). On each
/// observed bit the matching count is incremented and the opposite count is
/// reset to a small floor (3), which strongly emphasises recent statistics — an
/// aggressive recency bias that lets the StateMap track nonstationary data well.
#[inline]
fn next_state(s: u8, bit: i32) -> u8 {
    // Asymmetric encoding: with the reset-recency rule the minority count is
    // always small, so pack (sign, big=majority 0..31, small=minority 0..3) to
    // distinguish run lengths up to 31 instead of the nibble's 15.
    let sign = (s >> 7) & 1;
    let big = ((s >> 2) & 31) as i32;
    let small = (s & 3) as i32;
    let (mut n0, mut n1) = if sign == 0 {
        (big, small)
    } else {
        (small, big)
    };
    if bit != 0 {
        n1 += 1;
        if n0 > 3 {
            n0 = 3;
        }
    } else {
        n0 += 1;
        if n1 > 3 {
            n1 = 3;
        }
    }
    let (ns, nb, nsm) = if n1 > n0 { (1, n1, n0) } else { (0, n0, n1) };
    let nb = if nb > 31 { 31 } else { nb };
    let nsm = if nsm > 3 { 3 } else { nsm };
    ((ns << 7) | (nb << 2) | nsm) as u8
}

struct Apm {
    t: Vec<u16>,
    idx: usize,
}

impl Apm {
    fn new(n: usize, squash: &[i32]) -> Self {
        let mut t = vec![0u16; n * APM_S];
        for c in 0..n {
            for j in 0..APM_S {
                t[c * APM_S + j] = (squash_d(squash, (j as i32 - 16) * 128) * 16) as u16;
            }
        }
        Apm { t, idx: 0 }
    }

    /// 16-bit SSE/APM step: takes a 16-bit probability `p`, stretches it via the
    /// 16-bit table, interpolates the (16-bit) calibration table, and returns a
    /// 16-bit probability. Keeps the whole final chain at 16 bits so confident
    /// predictions are not re-quantized to the 12-bit 1/4096 grid.
    #[inline]
    fn apply16(&mut self, stretch16: &[i32], ctx: usize, p: i32) -> i32 {
        // p is clamped to [1,65534] before every call (valid index into the
        // 65536-entry table). j = s>>7 is in [0,31] and ctx < n at every call site,
        // so idx and idx+1 stay within t (len n*APM_S). Skip the bounds checks.
        let s = unsafe { *stretch16.get_unchecked(p as usize) } + 2048; // 0..4095
        let w = s & 127;
        let j = (s >> 7) as usize;
        self.idx = ctx * APM_S + j;
        let lo = unsafe { *self.t.get_unchecked(self.idx) } as i32;
        let hi = unsafe { *self.t.get_unchecked(self.idx + 1) } as i32;
        let mut pp = (lo * (128 - w) + hi * w) >> 7;
        if pp < 1 {
            pp = 1;
        }
        if pp > 65534 {
            pp = 65534;
        }
        pp
    }

    #[inline]
    fn update(&mut self, bit: i32) {
        let g = (bit << 16) + (bit << 4) - bit - bit;
        // idx/idx+1 were set to in-range values by the matching apply16 call.
        let a = unsafe { *self.t.get_unchecked(self.idx) } as i32;
        let b = unsafe { *self.t.get_unchecked(self.idx + 1) } as i32;
        unsafe {
            *self.t.get_unchecked_mut(self.idx) = (a + ((g - a) >> 7)) as u16;
            *self.t.get_unchecked_mut(self.idx + 1) = (b + ((g - b) >> 7)) as u16;
        }
    }
}

/// A context-selected logistic mixer. Holds `nctx` weight rows of `n` inputs;
/// each step selects one row by context, dot-products it with the stretched
/// inputs to produce a logit, and trains that row online toward the observed
/// bit. Used both as the layer-1 specialists and the layer-2 combiner.
struct Mixer {
    nctx: usize,
    w: Vec<i32>,
    ctx: usize,
    pr: i32,
    lr: i32,
}

impl Mixer {
    fn new(n: usize, nctx: usize, lr: i32) -> Self {
        Mixer {
            nctx,
            w: vec![(1 << 16) / n as i32; n * nctx],
            ctx: 0,
            pr: 2048,
            lr,
        }
    }

    // The per-mixer mix()/update() methods were replaced by the fused layer-1 and
    // layer-2 passes in predict()/update() (all mixers share one input vector, so
    // it is loaded once instead of per mixer). Mixer is now a plain weight/ctx/pr
    // holder; `new` and the fields are used directly by the fused loops.
}

pub struct Cm {
    gln_hp: [[[i8; GLN_NSEL]; GLN_BITS as usize]; NGLN_HS], // fixed ±1 halfspace gating weights
    stretch: Vec<i32>,
    squash: Vec<i32>,
    stretch16: Vec<i32>,
    squash16: Vec<i32>,
    // Per-context slots stored array-of-structs (one packed 5-byte Slot) so a
    // bucket access touches a single cache line instead of four separate arrays.
    tab: Vec<Vec<Slot>>,  // [NCTX][TSIZE]
    sm: Vec<[u32; 2048]>, // [NCTX][256*8] StateMap: (state | bitpos<<8) -> (prob22<<10 | count)
    sm_idx: [usize; NCTX],
    tmask: [u32; NCTX],  // per-model context-table index mask
    assoc: [bool; NCTX], // model uses 2-way set-associative buckets
    bmask: [u32; NCTX],  // bucket-index mask for associative models (2^(tb-1) - 1)
    cshift: [u32; NCTX], // checksum shift for associative models (tb - 1)
    rate_tab: [i32; 256],
    ctxhash: [u32; NCTX],
    idx: [usize; NCTX],
    mix_in: [i32; NINPUT],
    mix_in64: [i64; NINPUT], // mix_in pre-widened to i64 for the L1 dot products
    l1: Vec<Mixer>,          // layer-1 specialist mixers (different selection contexts)
    l2: Mixer,               // layer-2 combiner over the layer-1 logits (last-byte ctx)
    l2b: Mixer,              // second layer-2 combiner (bit-position ctx)
    l2c: Mixer,              // third layer-2 combiner (match-state ctx)
    l2d: Mixer,              // fourth layer-2 combiner (2nd-to-last-byte ctx)
    l2e: Mixer,              // fifth layer-2 combiner (word ctx)
    l2f: Mixer,              // sixth layer-2 combiner (high-nibble / opcode-class ctx)
    l2g: Mixer,              // seventh layer-2 combiner (char-class / text-mode ctx)
    l2h: Mixer,              // eighth layer-2 combiner (nesting-state ctx)
    l2i: Mixer,              // ninth layer-2 combiner (byte-above / 2D ctx)
    l2j: Mixer,              // tenth layer-2 combiner (byte-delta / numeric ctx)
    l2k: Mixer,              // eleventh layer-2 combiner (difficulty-regime ctx)
    l2l: Mixer,              // twelfth layer-2 combiner (high-level confidence ctx)
    l2m: Mixer,              // thirteenth layer-2 combiner (StateMap confidence ctx)
    l2n: Mixer,              // fourteenth layer-2 combiner (local-counter confidence ctx)
    l2o: Mixer,              // fifteenth layer-2 combiner (word-model confidence ctx)
    l2p: Mixer,              // sixteenth layer-2 combiner (word/mid-order StateMap confidence ctx)
    l2_in: [i32; NL1],
    l2_in64: [i64; NL1], // l2_in pre-widened to i64 for the L2 dot products
    buf: Vec<u8>,
    bufmask: u32,
    pos: u32,
    mmtab: Vec<u32>,
    matchptr: u32,
    matchlen: i32,
    predicted_byte: i32,
    mm_sm: [u32; 80],
    mm_used: bool,
    mm_idx: usize,
    mmtab2: Vec<u32>,
    matchptr2: u32,
    matchlen2: i32,
    predicted_byte2: i32,
    mm_sm2: [u32; 80],
    mm_used2: bool,
    mm_idx2: usize,
    mmtab3: Vec<u32>,
    matchptr3: u32,
    matchlen3: i32,
    predicted_byte3: i32,
    mm_sm3: [u32; 184],
    mm_used3: bool,
    mm_idx3: usize,
    mmtab4: Vec<u32>,
    matchptr4: u32,
    matchlen4: i32,
    predicted_byte4: i32,
    mm_sm4: [u32; 160],
    mm_used4: bool,
    mm_idx4: usize,
    mmtab5: Vec<u32>,
    matchptr5: u32,
    matchlen5: i32,
    predicted_byte5: i32,
    mm_sm5: [u32; 160],
    mm_used5: bool,
    mm_idx5: usize,
    mmtab6: Vec<u32>,
    matchptr6: u32,
    matchlen6: i32,
    predicted_byte6: i32,
    mm_sm6: [u32; 80],
    mm_used6: bool,
    mm_idx6: usize,
    apm1: Apm,
    apm2: Apm,
    apm3: Apm,
    apm4: Apm,
    dmc: Dmc,
    dmc2: Dmc,
    dmc3: Dmc,
    dmc4: Dmc,
    dmc5: Dmc,
    ctw: Ctw,
    // RunContextMap bank (one per RUN_CTX context). Each table slot packs
    // (chk:8 | count:8 | byte:8); a small StateMap per map turns (run-length,
    // expected-bit) into a probability the mixer can weight.
    runtab: Vec<Vec<u32>>,
    runbits: u32,
    runmask: u32,
    run_sm: [[u32; 128]; NRUN], // count-based StateMap cells: (prob22 << 10) | count
    run_pbyte: [i32; NRUN], // byte predicted by this run map for the current byte (-1 = none)
    run_cnt: [i32; NRUN],   // its run length (consecutive repeats observed)
    run_used: [bool; NRUN], // run map contributed a prediction this bit (gate its update)
    run_idx: [usize; NRUN], // run_sm cell read this bit, reused by update
    c0: i32,
    bitcount: i32,
    hard: i32, // EMA of recent per-bit coding surprise (regime difficulty), 0..65536
    hard_fast: i32, // fast surprise EMA (~8-bit window) — trend numerator
    hard_slow: i32, // slow surprise EMA (~128-bit window) — trend baseline
    c4: u32,
    wordhash: u32,
    prevword: u32,
    prevword2: u32,
    prevword3: u32,
    c1: i32,
    run_len: u32,         // length of the current run of identical bytes
    nest_stack: [u8; 64], // stack of currently-open bracket chars (source nesting)
    nest_depth: usize,
    col: u32,
    line_start: u32,
    prev_line_start: u32,
    prev2_line_start: u32,
    rpos: [u32; 256],    // last position each byte value occurred (record detector)
    rlen: u32,           // current dominant recurrence period (record length)
    rcount: i32,         // confidence in rlen (Boyer-Moore majority vote)
    above_byte: u32,     // byte directly above (same column, previous line); 256 if none
    ind_pred: u32,       // most recent byte that followed the current order-2 context
    follow1: Vec<u32>,   // [256] packed recent bytes that followed each order-1 ctx
    follow2: Vec<u32>,   // [65536] packed recent bytes that followed each order-2 ctx
    follow3: Vec<u32>,   // [FSIZE] hashed: bytes that followed each order-3 ctx
    follow4: Vec<u32>,   // [FSIZE] hashed: bytes that followed each order-4 ctx
    follow5: Vec<u32>,   // [FSIZE] hashed: bytes that followed each order-5 ctx
    follow6: Vec<u32>,   // [FSIZE] hashed: bytes that followed each order-6 ctx
    followhn: Vec<u32>,  // [FSIZE] hashed: bytes that followed each high-nibble ctx
    followd: Vec<u32>,   // [FSIZE] hashed: bytes that followed each byte-delta ctx
    followc: Vec<u32>,   // [256] bytes that followed each char-class ctx
    followln: Vec<u32>,  // [FSIZE] hashed: bytes that followed each low-nibble ctx
    followg: Vec<u32>,   // [65536] bytes that followed each gap-bigram ctx
    follows2: Vec<u32>,  // [65536] bytes that followed each stride-2 ctx
    follows3: Vec<u32>,  // [65536] bytes that followed each stride-3 ctx
    followg2: Vec<u32>,  // [65536] bytes that followed each wide-gap ctx
    followhn8: Vec<u32>, // [FSIZE] bytes that followed each 8-byte high-nibble ctx
    follows4: Vec<u32>,  // [65536] bytes that followed each stride-4 ctx
    followln8: Vec<u32>, // [FSIZE] bytes that followed each 8-byte low-nibble ctx
    followg14: Vec<u32>, // [65536] bytes that followed each gap(1,4) ctx
    followcc8: Vec<u32>, // [65536] bytes that followed each 8-byte char-class ctx
    follows5: Vec<u32>,  // [65536] bytes that followed each stride-5 ctx
    followg16: Vec<u32>, // [65536] bytes that followed each gap(1,6) ctx
    followw: Vec<u32>,   // [65536] hashed: bytes that followed each word prefix
}

impl Cm {
    pub fn new(expected_len: usize) -> Self {
        let (stretch, squash) = build();
        let (stretch16, squash16) = build16();
        let mut rate_tab = [0i32; 256];
        for n in 0..256 {
            let mut r = 4096 / (n as i32 + 2);
            if r < RATE_FLOOR {
                r = RATE_FLOOR;
            }
            rate_tab[n] = r;
        }
        // Per-model context-table sizes. All models use the full 2^TBITS table
        // except order-0 (ctxhash[0] == 0), whose index is just the partial-byte
        // c0 (<=255); a 512-slot table is byte-for-byte identical there, saving
        // ~32 MB at zero cost to compression.
        // 2-way set-associative tables for the high-cardinality models (the dense
        // orders, sparse/stride banks, and indirect families). Each context maps
        // to a 2-slot bucket distinguished by an 8-bit checksum; colliding contexts
        // occupy separate warm slots instead of polluting one, recovering most of
        // the loss that a 4x-bigger direct table would. Because associativity
        // resolves collisions, the associative data tables can be SMALLER (2^22)
        // than the old direct ones (2^23) and still beat them — so this is also
        // net memory-negative. Low-cardinality models (order-0/1/2, bytes-2-3,
        // char-class) gain nothing and stay direct-mapped.
        let mut assoc = [false; NCTX];
        for i in 0..NCTX {
            let small = i == 0 || i == 1 || i == 2 || i == 8 || i == 93;
            if !small {
                assoc[i] = true;
            }
        }
        // Size the high-cardinality context tables to the input length: a larger
        // input touches more distinct contexts, so a bigger table keeps the hash
        // load factor (and the associative-bucket eviction rate) low. General
        // policy — bigger input, bigger model — not tied to any specific data.
        // CRUCIAL: the big tables are GATED on input size. Large inputs (the
        // corpus, >=256 KB) get 2^24 for SCORE; smaller inputs (incl. every
        // high-commit round-trip test) stay at 2^20 so the parallel verifier does
        // not OOM. Sizing for SCORE is fine because the objective ignores speed.
        let big = expected_len >= 262_144;
        let mut tb = [TBITS; NCTX];
        tb[0] = 9;
        for i in 0..NCTX {
            if assoc[i] {
                tb[i] = if big { 22 } else { 20 };
            }
        }
        let mut tmask = [0u32; NCTX];
        for i in 0..NCTX {
            tmask[i] = (1u32 << tb[i]) - 1;
        }
        let mut bmask = [0u32; NCTX];
        let mut cshift = [0u32; NCTX];
        for i in 0..NCTX {
            if assoc[i] {
                bmask[i] = (1u32 << (tb[i] - WAYS_LOG)) - 1;
                cshift[i] = tb[i] - WAYS_LOG;
            }
        }
        let tab: Vec<Vec<Slot>> = (0..NCTX)
            .map(|i| {
                vec![
                    Slot {
                        cp: 0,
                        cn: 0,
                        st: 0,
                        ck: 0
                    };
                    1usize << tb[i]
                ]
            })
            .collect();
        let sm = (0..NCTX).map(|_| [1u32 << 31; 2048]).collect();
        let q = L1LR;
        let l1 = vec![
            Mixer::new(NINPUT, MIXCTX, q),
            Mixer::new(NINPUT, 256, q),
            Mixer::new(NINPUT, 256, q),
            Mixer::new(NINPUT, MIX3CTX, q),
            Mixer::new(NINPUT, MIX4CTX, q),
            Mixer::new(NINPUT, 64, q),
            Mixer::new(NINPUT, 4096, q),
            Mixer::new(NINPUT, 8192, q),
            Mixer::new(NINPUT, 8192, q),
            Mixer::new(NINPUT, 4096, q),
            Mixer::new(NINPUT, 4096, q),
            Mixer::new(NINPUT, 512, q),
            Mixer::new(NINPUT, 256, q),
            Mixer::new(NINPUT, 1024, q),
            Mixer::new(NINPUT, 4096, q),
            Mixer::new(NINPUT, 256, q),
            Mixer::new(NINPUT, 256, q),
            Mixer::new(NINPUT, 64, q), // run-length regime selector
            Mixer::new(NINPUT, 32, q), // gradient / delta-sign selector
            Mixer::new(NINPUT, 16, q), // periodic / record selector
            Mixer::new(NINPUT, 64, q), // above-char-class + nest selector
            Mixer::new(NINPUT, 32, q), // gradient-magnitude selector
            Mixer::new(NINPUT, 1 << GLN_BITS, q), // GLN halfspace specialist
            Mixer::new(NINPUT, 1 << GLN_BITS, q), // GLN: axis-aligned (per-prediction sign) gate
            Mixer::new(NINPUT, 256, q), // GLN: high-level predictor sign-agreement gate
            Mixer::new(NINPUT, 2048, q), // GLN: high-level predictor confidence x bitpos gate
            Mixer::new(NINPUT, 256, q), // GLN: local-context (order-N) confidence gate
            Mixer::new(NINPUT, 256, q), // GLN: word-model confidence gate
            Mixer::new(NINPUT, 2048, q), // GLN: bit-history StateMap confidence x bitpos gate
            Mixer::new(NINPUT, 256, q), // GLN: high-order/structural StateMap confidence gate
            Mixer::new(NINPUT, 256, q), // GLN: fine dual-confidence (CTW + word statemap)
            Mixer::new(NINPUT, 256, q), // difficulty-regime specialist (surprise x bitpos)
            Mixer::new(NINPUT, 256, q), // difficulty-trend specialist (fast vs slow surprise x bitpos)
        ];
        let l2 = Mixer::new(NL1, 256, L2LR);
        let l2b = Mixer::new(NL1, 256, L2LR);
        let l2c = Mixer::new(NL1, 256, L2LR);
        let l2d = Mixer::new(NL1, 256, L2LR);
        let l2e = Mixer::new(NL1, 256, L2LR);
        let l2f = Mixer::new(NL1, 256, L2LR);
        let l2g = Mixer::new(NL1, 256, L2LR);
        let l2h = Mixer::new(NL1, 256, L2LR);
        let l2i = Mixer::new(NL1, 512, L2LR);
        let l2j = Mixer::new(NL1, 256, L2LR);
        let l2k = Mixer::new(NL1, 256, L2LR); // difficulty-regime x bitpos combiner
        let l2l = Mixer::new(NL1, 2048, L2LR); // high-level-confidence x bitpos combiner
        let l2m = Mixer::new(NL1, 2048, L2LR); // StateMap-confidence x bitpos combiner
        let l2n = Mixer::new(NL1, 256, L2LR); // local-counter-confidence combiner
        let l2o = Mixer::new(NL1, 256, L2LR); // word-model-confidence combiner
        let l2p = Mixer::new(NL1, 256, L2LR); // word/mid-order StateMap-confidence combiner

        let mut bufsize: u32 = 1;
        while (bufsize as usize) < expected_len + 16 && bufsize < (1 << 27) {
            bufsize <<= 1;
        }
        if bufsize < (1 << 16) {
            bufsize = 1 << 16;
        }

        let apm1 = Apm::new(1024, &squash);
        let apm2 = Apm::new(16384, &squash);
        let apm3 = Apm::new(1024, &squash);
        let apm4 = Apm::new(1024, &squash);

        // Fixed pseudo-random ±1 halfspace gating weights (same on encode/decode);
        // one independent hyperplane set per true-halfspace GLN specialist.
        let mut gln_hp = [[[0i8; GLN_NSEL]; GLN_BITS as usize]; NGLN_HS];
        for s in 0..NGLN_HS {
            for k in 0..GLN_BITS as usize {
                for i in 0..GLN_NSEL {
                    let seed = ((s as u32) << 24) ^ (k as u32).wrapping_mul(0x9E37_79B1) ^ 0x5bd1_e995;
                    let h = hashk(seed, i as u32);
                    gln_hp[s][k][i] = if (h >> 19) & 1 == 0 { 1 } else { -1 };
                }
            }
        }
        Cm {
            gln_hp,
            stretch,
            squash,
            stretch16,
            squash16,
            tab,
            sm,
            sm_idx: [0; NCTX],
            tmask,
            assoc,
            bmask,
            cshift,
            rate_tab,
            ctxhash: [0; NCTX],
            idx: [0; NCTX],
            mix_in: [0; NINPUT],
            mix_in64: [0; NINPUT],
            l1,
            l2,
            l2b,
            l2c,
            l2d,
            l2e,
            l2f,
            l2g,
            l2h,
            l2i,
            l2j,
            l2k,
            l2l,
            l2m,
            l2n,
            l2o,
            l2p,
            l2_in: [0; NL1],
            l2_in64: [0; NL1],
            buf: vec![0u8; bufsize as usize],
            bufmask: bufsize - 1,
            pos: 0,
            mmtab: vec![0u32; MMSIZE],
            matchptr: 0,
            matchlen: 0,
            predicted_byte: -1,
            mm_sm: [1u32 << 31; 80],
            mm_used: false,
            mm_idx: 0,
            mmtab2: vec![0u32; MMSIZE2],
            matchptr2: 0,
            matchlen2: 0,
            predicted_byte2: -1,
            mm_sm2: [1u32 << 31; 80],
            mm_used2: false,
            mm_idx2: 0,
            mmtab3: vec![0u32; MMSIZE3],
            matchptr3: 0,
            matchlen3: 0,
            predicted_byte3: -1,
            mm_sm3: [1u32 << 31; 184],
            mm_used3: false,
            mm_idx3: 0,
            mmtab4: vec![0u32; MMSIZE4],
            matchptr4: 0,
            matchlen4: 0,
            predicted_byte4: -1,
            mm_sm4: [1u32 << 31; 160],
            mm_used4: false,
            mm_idx4: 0,
            mmtab5: vec![0u32; MMSIZE5],
            matchptr5: 0,
            matchlen5: 0,
            predicted_byte5: -1,
            mm_sm5: [1u32 << 31; 160],
            mm_used5: false,
            mm_idx5: 0,
            mmtab6: vec![0u32; MMSIZE6],
            matchptr6: 0,
            matchlen6: 0,
            predicted_byte6: -1,
            mm_sm6: [1u32 << 31; 80],
            mm_used6: false,
            mm_idx6: 0,
            apm1,
            apm2,
            apm3,
            apm4,
            // Two complementary-speed DMCs (aggressive clone thresholds); paired
            // with RATE_FLOOR=16 these set the 572060 record (re-tuned from the
            // frontier's stale 2/8 + RATE_FLOOR 40).
            dmc: Dmc::new(1, 1),
            dmc2: Dmc::new(2, 2),
            // Three more DMCs spanning higher clone thresholds. Entry 0090 proved a
            // (1,2,3,5,8) ensemble is the optimal DMC spread on the lean fork; the
            // record only ran (1,2). The aggressive pair specializes fast/high-order;
            // these slower ones add stable lower-order signal the mixer can weight in.
            dmc3: Dmc::new(3, 3),
            dmc4: Dmc::new(5, 5),
            dmc5: Dmc::new(8, 8),
            ctw: Ctw::new(),
            // Run maps are size-gated like the context tables: 2^22 for the corpus,
            // 2^18 for the small/adversarial round-trip inputs (keeps the parallel
            // verifier from OOMing). Both sides of a round-trip pass the same length,
            // so the size — and thus every index/checksum — is identical on en/decode.
            runtab: (0..NRUN)
                .map(|_| vec![0u32; 1usize << if big { 20 } else { 18 }])
                .collect(),
            runbits: if big { 20 } else { 18 },
            runmask: (1u32 << if big { 20 } else { 18 }) - 1,
            run_sm: [[1u32 << 31; 128]; NRUN], // prob22 = 0.5, count = 0
            run_pbyte: [-1; NRUN],
            run_cnt: [0; NRUN],
            run_used: [false; NRUN],
            run_idx: [0; NRUN],
            c0: 1,
            bitcount: 0,
            hard: 0,
            hard_fast: 0,
            hard_slow: 0,
            c4: 0,
            wordhash: 0,
            prevword: 0,
            prevword2: 0,
            prevword3: 0,
            c1: 0,
            run_len: 1,
            nest_stack: [0u8; 64],
            nest_depth: 0,
            col: 0,
            line_start: 0,
            prev_line_start: 0,
            prev2_line_start: 0,
            rpos: [0; 256],
            rlen: 0,
            rcount: 0,
            above_byte: 256,
            ind_pred: 0,
            follow1: vec![0u32; 256],
            follow2: vec![0u32; 65536],
            follow3: vec![0u32; FSIZE],
            follow4: vec![0u32; FSIZE],
            follow5: vec![0u32; FSIZE],
            follow6: vec![0u32; FSIZE],
            followhn: vec![0u32; FSIZE],
            followd: vec![0u32; FSIZE],
            followc: vec![0u32; 256],
            followln: vec![0u32; FSIZE],
            followg: vec![0u32; 65536],
            follows2: vec![0u32; 65536],
            follows3: vec![0u32; 65536],
            followg2: vec![0u32; 65536],
            followhn8: vec![0u32; FSIZE],
            follows4: vec![0u32; 65536],
            followln8: vec![0u32; FSIZE],
            followg14: vec![0u32; 65536],
            followcc8: vec![0u32; 65536],
            follows5: vec![0u32; 65536],
            followg16: vec![0u32; 65536],
            followw: vec![0u32; 65536],
        }
    }

    #[inline]
    fn b(&self, p: u32) -> u8 {
        // buf.len() == bufmask + 1 (a power of two), so `p & bufmask` is always a
        // valid index; skip the bounds check on this very hot byte accessor.
        unsafe { *self.buf.get_unchecked((p & self.bufmask) as usize) }
    }

    pub fn byte_start(&mut self) {
        let c4 = self.c4;
        self.ctxhash[0] = 0;
        self.ctxhash[1] = hashk(0x100, c4 & 0xff);
        self.ctxhash[2] = hashk(0x200, c4 & 0xffff);
        self.ctxhash[3] = hashk(0x300, c4 & 0xffffff);
        self.ctxhash[4] = hashk(0x400, c4);
        self.ctxhash[5] = hashk(
            0x500,
            c4.wrapping_mul(0x9E37_79B1) ^ ((self.c1 as u32) << 3),
        );
        self.ctxhash[6] = if self.pos >= 6 {
            hashk(
                0x600,
                c4.wrapping_mul(2654435761)
                    ^ ((self.b(self.pos - 5) as u32) << 7)
                    ^ ((self.b(self.pos - 6) as u32) << 15),
            )
        } else {
            hashk(0x600, c4)
        };
        self.ctxhash[7] = if self.wordhash != 0 {
            hashk(0x700, self.wordhash)
        } else {
            0
        };
        self.ctxhash[8] = hashk(0x800, ((c4 >> 8) & 0xff) | (((c4 >> 16) & 0xff) << 8));
        self.ctxhash[9] = if self.pos >= 7 {
            hashk(
                0x900,
                c4.wrapping_mul(0x9E37_79B1)
                    ^ ((self.b(self.pos - 5) as u32).wrapping_mul(0x85eb_ca6b))
                    ^ ((self.b(self.pos - 6) as u32).wrapping_mul(0xc2b2_ae35))
                    ^ ((self.b(self.pos - 7) as u32).wrapping_mul(0x27d4_eb2f)),
            )
        } else {
            hashk(0x900, c4)
        };
        self.ctxhash[10] = if self.pos >= 8 {
            hashk(
                0xA00,
                c4.wrapping_mul(0x85eb_ca6b)
                    ^ ((self.b(self.pos - 5) as u32).wrapping_mul(0xc2b2_ae35))
                    ^ ((self.b(self.pos - 6) as u32).wrapping_mul(0x27d4_eb2f))
                    ^ ((self.b(self.pos - 7) as u32).wrapping_mul(0x1656_67b1))
                    ^ ((self.b(self.pos - 8) as u32).wrapping_mul(0x9E37_79B1)),
            )
        } else {
            hashk(0xA00, c4)
        };
        self.ctxhash[11] = if self.pos >= 9 {
            hashk(
                0xB00,
                c4.wrapping_mul(0xc2b2_ae35)
                    ^ ((self.b(self.pos - 5) as u32).wrapping_mul(0x27d4_eb2f))
                    ^ ((self.b(self.pos - 6) as u32).wrapping_mul(0x1656_67b1))
                    ^ ((self.b(self.pos - 7) as u32).wrapping_mul(0x85eb_ca6b))
                    ^ ((self.b(self.pos - 8) as u32).wrapping_mul(0x9E37_79B1))
                    ^ ((self.b(self.pos - 9) as u32).wrapping_mul(0xff51_afd7)),
            )
        } else {
            hashk(0xB00, c4)
        };
        self.ctxhash[12] = if self.wordhash != 0 {
            hashk(0xC00, self.wordhash ^ ((self.c1 as u32) << 8))
        } else {
            let b1 = c4 & 0xff;
            let b2 = (c4 >> 8) & 0xff;
            let b3 = (c4 >> 16) & 0xff;
            hashk(0xC00, b1 | (b2 << 8) | ((b3 & 0x1f) << 16))
        };
        self.ctxhash[13] = if self.wordhash != 0 {
            hashk(
                0xD00,
                self.wordhash.wrapping_mul(0x85eb_ca6b) ^ (c4 & 0xffff),
            )
        } else {
            let mut h = 0u32;
            let mut x = c4;
            for _ in 0..4 {
                let b = (x & 0xff) as u8;
                let class = if (b >= b'a' && b <= b'z') || (b >= b'A' && b <= b'Z') {
                    1
                } else if b >= b'0' && b <= b'9' {
                    2
                } else if b == b' ' || b == b'\n' || b == b'\t' || b == b'\r' {
                    3
                } else {
                    4
                };
                h = (h << 3) | class;
                x >>= 8;
            }
            hashk(0xD00, h ^ (c4 & 0xff))
        };
        self.ctxhash[14] = if self.wordhash != 0 {
            let folded = (c4 & 0xdfdf_dfdf).wrapping_mul(0x27d4_eb2f);
            hashk(0xE00, self.wordhash.wrapping_mul(0xc2b2_ae35) ^ folded)
        } else {
            let b1 = c4 & 0xff;
            let b2 = (c4 >> 8) & 0xff;
            let b3 = (c4 >> 16) & 0xff;
            let b4 = (c4 >> 24) & 0xff;
            hashk(
                0xE00,
                b1.wrapping_mul(3) ^ b2.wrapping_mul(5) ^ b3.wrapping_mul(7) ^ b4.wrapping_mul(11),
            )
        };
        self.ctxhash[15] = hashk(0xF00, (self.col.min(255) << 16) ^ (c4 & 0xffff));
        let b1 = (c4 & 0xff) as u8;
        let class = if (b1 >= b'a' && b1 <= b'z') || (b1 >= b'A' && b1 <= b'Z') {
            1
        } else if b1 >= b'0' && b1 <= b'9' {
            2
        } else if b1 == b' ' || b1 == b'\n' || b1 == b'\t' || b1 == b'\r' {
            3
        } else {
            4
        };
        self.ctxhash[16] = hashk(
            0x1000,
            ((self.col & 63) << 8) ^ class ^ self.wordhash.wrapping_mul(0x9e37_79b1),
        );
        // order-5: the four bytes in c4 plus the byte at pos-5.
        self.ctxhash[17] = if self.pos >= 5 {
            hashk(
                0x1100,
                c4.wrapping_mul(0x2545_f491)
                    ^ ((self.b(self.pos - 5) as u32).wrapping_mul(0x9e37_79b1)),
            )
        } else {
            hashk(0x1100, c4)
        };
        // order-11: c4 plus bytes pos-5..pos-11.
        self.ctxhash[18] = if self.pos >= 11 {
            hashk(
                0x1200,
                c4.wrapping_mul(0x9e37_79b1)
                    ^ ((self.b(self.pos - 5) as u32).wrapping_mul(0x85eb_ca6b))
                    ^ ((self.b(self.pos - 6) as u32).wrapping_mul(0xc2b2_ae35))
                    ^ ((self.b(self.pos - 7) as u32).wrapping_mul(0x27d4_eb2f))
                    ^ ((self.b(self.pos - 8) as u32).wrapping_mul(0x1656_67b1))
                    ^ ((self.b(self.pos - 9) as u32).wrapping_mul(0xff51_afd7))
                    ^ ((self.b(self.pos - 10) as u32).wrapping_mul(0xc4ce_b9fe))
                    ^ ((self.b(self.pos - 11) as u32).wrapping_mul(0x2545_f491)),
            )
        } else {
            hashk(0x1200, c4)
        };
        // stride-2 sparse: bytes at pos-2, pos-4, pos-6 (skips every other byte).
        self.ctxhash[19] = if self.pos >= 6 {
            hashk(
                0x1300,
                (self.b(self.pos - 2) as u32)
                    | ((self.b(self.pos - 4) as u32) << 8)
                    | ((self.b(self.pos - 6) as u32) << 16),
            )
        } else {
            hashk(0x1300, c4)
        };
        // gap bigram: bytes at pos-1 and pos-3 (skips pos-2).
        self.ctxhash[20] = if self.pos >= 3 {
            hashk(0x1400, (c4 & 0xff) | ((self.b(self.pos - 3) as u32) << 8))
        } else {
            hashk(0x1400, c4)
        };
        // gap bigram: bytes at pos-1 and pos-4 (skips pos-2, pos-3).
        self.ctxhash[21] = if self.pos >= 4 {
            hashk(0x1500, (c4 & 0xff) | ((self.b(self.pos - 4) as u32) << 8))
        } else {
            hashk(0x1500, c4)
        };
        // gap bigram: bytes at pos-1 and pos-5.
        self.ctxhash[22] = if self.pos >= 5 {
            hashk(0x1600, (c4 & 0xff) | ((self.b(self.pos - 5) as u32) << 8))
        } else {
            hashk(0x1600, c4)
        };
        // word bigram: previous completed word + the word currently being typed.
        self.ctxhash[23] = if self.prevword != 0 {
            hashk(
                0x1700,
                self.prevword
                    .wrapping_mul(0x9e37_79b1)
                    .wrapping_add(self.wordhash.wrapping_mul(0x85eb_ca6b)),
            )
        } else {
            0
        };
        // previous word + recent literal bytes: models the gap/punctuation that
        // follows a word and the run-up into the next one.
        self.ctxhash[24] = if self.prevword != 0 {
            hashk(
                0x1800,
                self.prevword.wrapping_mul(0xc2b2_ae35) ^ (c4 & 0xffff),
            )
        } else {
            0
        };
        // word trigram: the two preceding words plus the word being typed.
        self.ctxhash[25] = if self.prevword2 != 0 {
            hashk(
                0x1900,
                self.prevword2
                    .wrapping_mul(0x27d4_eb2f)
                    .wrapping_add(self.prevword.wrapping_mul(0x9e37_79b1))
                    .wrapping_add(self.wordhash.wrapping_mul(0x85eb_ca6b)),
            )
        } else {
            0
        };
        // order-10: c4 plus bytes pos-5..pos-10 (fills the gap below order-11).
        self.ctxhash[26] = if self.pos >= 10 {
            hashk(
                0x1A00,
                c4.wrapping_mul(0x2545_f491)
                    ^ ((self.b(self.pos - 5) as u32).wrapping_mul(0x85eb_ca6b))
                    ^ ((self.b(self.pos - 6) as u32).wrapping_mul(0xc2b2_ae35))
                    ^ ((self.b(self.pos - 7) as u32).wrapping_mul(0x27d4_eb2f))
                    ^ ((self.b(self.pos - 8) as u32).wrapping_mul(0x1656_67b1))
                    ^ ((self.b(self.pos - 9) as u32).wrapping_mul(0xff51_afd7))
                    ^ ((self.b(self.pos - 10) as u32).wrapping_mul(0xc4ce_b9fe)),
            )
        } else {
            hashk(0x1A00, c4)
        };
        // order-13: extends the direct-context ladder past order-11.
        self.ctxhash[27] = if self.pos >= 13 {
            hashk(
                0x1B00,
                c4.wrapping_mul(0xc2b2_ae35)
                    ^ ((self.b(self.pos - 5) as u32).wrapping_mul(0x85eb_ca6b))
                    ^ ((self.b(self.pos - 6) as u32).wrapping_mul(0xc2b2_ae35))
                    ^ ((self.b(self.pos - 7) as u32).wrapping_mul(0x27d4_eb2f))
                    ^ ((self.b(self.pos - 8) as u32).wrapping_mul(0x1656_67b1))
                    ^ ((self.b(self.pos - 9) as u32).wrapping_mul(0xff51_afd7))
                    ^ ((self.b(self.pos - 10) as u32).wrapping_mul(0xc4ce_b9fe))
                    ^ ((self.b(self.pos - 11) as u32).wrapping_mul(0x2545_f491))
                    ^ ((self.b(self.pos - 12) as u32).wrapping_mul(0x9e37_79b9))
                    ^ ((self.b(self.pos - 13) as u32).wrapping_mul(0x7f4a_7c15)),
            )
        } else {
            hashk(0x1B00, c4)
        };
        // order-16: a very long deterministic context for structured / repetitive
        // data, beyond what the match models alone cover.
        self.ctxhash[28] = if self.pos >= 16 {
            let mut h = c4.wrapping_mul(0x9e37_79b1);
            let mut k: u32 = 5;
            let mults: [u32; 12] = [
                0x85eb_ca6b,
                0xc2b2_ae35,
                0x27d4_eb2f,
                0x1656_67b1,
                0xff51_afd7,
                0xc4ce_b9fe,
                0x2545_f491,
                0x9e37_79b9,
                0x7f4a_7c15,
                0x94d0_49bb,
                0xd6e8_feb8,
                0xa548_1ad7,
            ];
            while k <= 16 {
                h ^= (self.b(self.pos - k) as u32).wrapping_mul(mults[(k - 5) as usize]);
                k += 1;
            }
            hashk(0x1C00, h)
        } else {
            hashk(0x1C00, c4)
        };
        // stride-3 sparse: bytes at pos-3, pos-6, pos-9 (columnar / record-aligned).
        self.ctxhash[29] = if self.pos >= 9 {
            hashk(
                0x1D00,
                (self.b(self.pos - 3) as u32)
                    | ((self.b(self.pos - 6) as u32) << 8)
                    | ((self.b(self.pos - 9) as u32) << 16),
            )
        } else {
            hashk(0x1D00, c4)
        };
        // stride-4 sparse: bytes at pos-4, pos-8, pos-12 (wider record alignment).
        self.ctxhash[30] = if self.pos >= 12 {
            hashk(
                0x1E00,
                (self.b(self.pos - 4) as u32)
                    | ((self.b(self.pos - 8) as u32) << 8)
                    | ((self.b(self.pos - 12) as u32) << 16),
            )
        } else {
            hashk(0x1E00, c4)
        };
        // stride-5 sparse: bytes at pos-5, pos-10, pos-15.
        self.ctxhash[31] = if self.pos >= 15 {
            hashk(
                0x1F00,
                (self.b(self.pos - 5) as u32)
                    | ((self.b(self.pos - 10) as u32) << 8)
                    | ((self.b(self.pos - 15) as u32) << 16),
            )
        } else {
            hashk(0x1F00, c4)
        };
        // stride-6 sparse: bytes at pos-6, pos-12, pos-18.
        self.ctxhash[32] = if self.pos >= 18 {
            hashk(
                0x2000,
                (self.b(self.pos - 6) as u32)
                    | ((self.b(self.pos - 12) as u32) << 8)
                    | ((self.b(self.pos - 18) as u32) << 16),
            )
        } else {
            hashk(0x2000, c4)
        };
        // stride-7 sparse: bytes at pos-7, pos-14, pos-21.
        self.ctxhash[33] = if self.pos >= 21 {
            hashk(
                0x2100,
                (self.b(self.pos - 7) as u32)
                    | ((self.b(self.pos - 14) as u32) << 8)
                    | ((self.b(self.pos - 21) as u32) << 16),
            )
        } else {
            hashk(0x2100, c4)
        };
        // stride-8 sparse: bytes at pos-8, pos-16, pos-24.
        self.ctxhash[34] = if self.pos >= 24 {
            hashk(
                0x2200,
                (self.b(self.pos - 8) as u32)
                    | ((self.b(self.pos - 16) as u32) << 8)
                    | ((self.b(self.pos - 24) as u32) << 16),
            )
        } else {
            hashk(0x2200, c4)
        };
        // stride-9..16 sparse: three samples at each stride (record alignments).
        for (slot, stride, tag) in [
            (35usize, 9u32, 0x2300u32),
            (36, 10, 0x2400),
            (37, 11, 0x2500),
            (38, 12, 0x2600),
            (39, 13, 0x2700),
            (40, 14, 0x2800),
            (41, 15, 0x2900),
            (42, 16, 0x2A00),
            (43, 17, 0x2B00),
            (44, 18, 0x2C00),
            (45, 19, 0x2D00),
            (46, 20, 0x2E00),
        ] {
            self.ctxhash[slot] = if self.pos >= stride * 3 {
                hashk(
                    tag,
                    (self.b(self.pos - stride) as u32)
                        | ((self.b(self.pos - stride * 2) as u32) << 8)
                        | ((self.b(self.pos - stride * 3) as u32) << 16),
                )
            } else {
                hashk(tag, c4)
            };
        }
        // gap bigrams: last byte paired with one byte at distance k = 6..13.
        for (slot, k, tag) in [
            (47usize, 6u32, 0x3000u32),
            (48, 7, 0x3100),
            (49, 8, 0x3200),
            (50, 9, 0x3300),
            (51, 10, 0x3400),
            (52, 11, 0x3500),
            (53, 12, 0x3600),
            (54, 13, 0x3700),
        ] {
            self.ctxhash[slot] = if self.pos >= k {
                hashk(tag, (c4 & 0xff) | ((self.b(self.pos - k) as u32) << 8))
            } else {
                hashk(tag, c4)
            };
        }
        // 4-sample strided contexts: bytes at pos-k,2k,3k,4k for k=2..8
        // (longer periodic / record context than the 3-sample strides).
        for (slot, k, tag) in [
            (55usize, 2u32, 0x4000u32),
            (56, 3, 0x4100),
            (57, 4, 0x4200),
            (58, 5, 0x4300),
            (59, 6, 0x4400),
            (60, 7, 0x4500),
            (61, 8, 0x4600),
            (62, 9, 0x4700),
            (63, 10, 0x4800),
            (64, 11, 0x4900),
            (65, 12, 0x4A00),
        ] {
            self.ctxhash[slot] = if self.pos >= k * 4 {
                hashk(
                    tag,
                    (self.b(self.pos - k) as u32)
                        ^ (self.b(self.pos - k * 2) as u32).wrapping_mul(0x85eb_ca6b)
                        ^ (self.b(self.pos - k * 3) as u32).wrapping_mul(0xc2b2_ae35)
                        ^ (self.b(self.pos - k * 4) as u32).wrapping_mul(0x27d4_eb2f),
                )
            } else {
                hashk(tag, c4)
            };
        }
        // word 4-gram: three preceding words plus the word currently being typed.
        self.ctxhash[66] = if self.prevword3 != 0 {
            hashk(
                0x6000,
                self.prevword3
                    .wrapping_mul(0x1656_67b1)
                    .wrapping_add(self.prevword2.wrapping_mul(0x27d4_eb2f))
                    .wrapping_add(self.prevword.wrapping_mul(0x9e37_79b1))
                    .wrapping_add(self.wordhash.wrapping_mul(0x85eb_ca6b)),
            )
        } else {
            0
        };
        // word skip-gram: the word two back paired with the word being typed
        // (skips the immediately preceding word).
        self.ctxhash[67] = if self.prevword2 != 0 {
            hashk(
                0x6100,
                self.prevword2
                    .wrapping_mul(0xc2b2_ae35)
                    .wrapping_add(self.wordhash.wrapping_mul(0x9e37_79b1)),
            )
        } else {
            0
        };
        // word skip-gram: the word three back paired with the word being typed.
        self.ctxhash[68] = if self.prevword3 != 0 {
            hashk(
                0x6200,
                self.prevword3
                    .wrapping_mul(0x1656_67b1)
                    .wrapping_add(self.wordhash.wrapping_mul(0x9e37_79b1)),
            )
        } else {
            0
        };
        // 2D / "byte above" model: the byte at the same column in the previous
        // line. Powerful for aligned source code, text and tabular structure.
        let col2d = self.pos.wrapping_sub(self.line_start);
        let above_pos = self.prev_line_start.wrapping_add(col2d);
        let have_above = self.prev_line_start != 0 && above_pos < self.line_start;
        let above = if have_above {
            self.b(above_pos) as u32
        } else {
            0
        };
        self.above_byte = if have_above { above } else { 256 };
        // byte above + current column
        self.ctxhash[69] = if have_above {
            hashk(0x6300, above | (col2d.min(1023) << 9))
        } else {
            0
        };
        // byte above + byte to the left (2D neighbourhood)
        self.ctxhash[70] = if have_above {
            hashk(0x6400, above | ((self.c1 as u32) << 8))
        } else {
            0
        };
        // byte above + the byte above-and-left (diagonal), captures 2D runs
        self.ctxhash[71] = if have_above && above_pos > self.prev_line_start {
            let above_left = self.b(above_pos - 1) as u32;
            hashk(0x6500, above | (above_left << 8))
        } else {
            0
        };
        // byte two lines up (same column) + byte above: a vertical bigram that
        // captures repeated/aligned blocks spanning multiple lines.
        let above2_pos = self.prev2_line_start.wrapping_add(col2d);
        self.ctxhash[74] =
            if self.prev2_line_start != 0 && above2_pos < self.prev_line_start && have_above {
                let above2 = self.b(above2_pos) as u32;
                hashk(0x6800, above | (above2 << 8))
            } else {
                0
            };
        // upper-forward: byte above + the byte above-right (the char that came
        // next on the previous line) — strong when the current line copies it.
        self.ctxhash[79] = if have_above && above_pos + 1 < self.line_start {
            let above_r = self.b(above_pos + 1) as u32;
            hashk(0x6D00, above | (above_r << 8))
        } else {
            0
        };
        // 3-wide horizontal window from the previous line (above-left/above/right)
        self.ctxhash[80] =
            if have_above && above_pos > self.prev_line_start && above_pos + 1 < self.line_start {
                let al = self.b(above_pos - 1) as u32;
                let ar = self.b(above_pos + 1) as u32;
                hashk(0x6E00, above ^ (al << 8) ^ (ar << 16))
            } else {
                0
            };
        // Record model: the byte one detected-period back (the "byte above" for
        // newline-free periodic data such as executables and tables).
        let r = self.rlen;
        let rec_ok = self.rcount > 8 && r >= 2 && r < self.pos;
        let recb = if rec_ok {
            self.b(self.pos - r) as u32
        } else {
            0
        };
        self.ctxhash[72] = if rec_ok {
            hashk(0x6600, recb | ((self.c1 as u32) << 8))
        } else {
            0
        };
        // record byte + the byte just before it one period back (2-gram above)
        self.ctxhash[73] = if rec_ok && self.pos > r + 1 {
            let recb1 = self.b(self.pos - r - 1) as u32;
            hashk(0x6700, recb | (recb1 << 8))
        } else {
            0
        };
        // Indirect models: the current order-1 / order-2 context combined with
        // the recent history of bytes that have followed it. Captures higher-order
        // regularity ("what usually comes next here") that direct contexts miss.
        let f1 = self.follow1[(c4 & 0xff) as usize];
        self.ctxhash[75] = hashk(0x6900, (c4 & 0xff) ^ f1.wrapping_mul(0x9e37_79b1));
        let f2 = self.follow2[(c4 & 0xffff) as usize];
        self.ctxhash[76] = hashk(0x6A00, (c4 & 0xffff) ^ f2.wrapping_mul(0x85eb_ca6b));
        self.ind_pred = f2 & 0xff;
        let j3 = ((c4 & 0x00ff_ffff).wrapping_mul(0x9e37_79b1) >> (32 - FBITS)) as usize;
        self.ctxhash[77] = hashk(
            0x6B00,
            (c4 & 0x00ff_ffff) ^ self.follow3[j3].wrapping_mul(0xc2b2_ae35),
        );
        let j4 = (c4.wrapping_mul(0x85eb_ca6b) >> (32 - FBITS)) as usize;
        self.ctxhash[78] = hashk(0x6C00, c4 ^ self.follow4[j4].wrapping_mul(0x27d4_eb2f));
        // Word-indirect: current word prefix + the bytes that have followed it.
        self.ctxhash[81] = if self.wordhash != 0 {
            let wk = (self.wordhash.wrapping_mul(0x9e37_79b1) >> 16) as usize;
            hashk(
                0x7000,
                self.wordhash ^ self.followw[wk].wrapping_mul(0xc2b2_ae35),
            )
        } else {
            0
        };
        // Run model: last byte + the length of its current run (capped). Models
        // run continuation/termination (zero-runs in binary, repeated chars).
        self.ctxhash[82] = hashk(0x7100, (c4 & 0xff) | (self.run_len.min(255) << 8));
        // Nesting model: predict from bracket-nesting depth and the enclosing
        // bracket — captures the ()[]{} structure pervasive in source code.
        let last_open = if self.nest_depth > 0 {
            self.nest_stack[self.nest_depth - 1] as u32
        } else {
            0
        };
        self.ctxhash[83] = hashk(0x7200, (self.nest_depth as u32 & 31) | ((c4 & 0xff) << 5));
        self.ctxhash[84] = hashk(0x7300, last_open | ((c4 & 0xff) << 8));
        // enclosing bracket + nesting depth + order-2 context (finer structure)
        self.ctxhash[85] = hashk(
            0x7400,
            last_open.wrapping_mul(0x9e37_79b1)
                ^ ((self.nest_depth as u32 & 31) << 16)
                ^ (c4 & 0xffff),
        );
        // High-nibble (opcode-class) context: the top nibble of the last 5 bytes,
        // ignoring low-bit operand noise — targets executable/binary structure.
        let hn = (c4 & 0xf0f0_f0f0)
            ^ if self.pos >= 5 {
                ((self.b(self.pos - 5) as u32) & 0xf0) << 24
            } else {
                0
            };
        self.ctxhash[86] = hashk(0x7700, hn);
        // longer high-nibble context (last 8 bytes' top nibbles), order-8-coarse.
        self.ctxhash[87] = if self.pos >= 8 {
            hashk(
                0x7800,
                hn.wrapping_mul(0x9e37_79b1)
                    ^ ((self.b(self.pos - 6) as u32 & 0xf0) << 4)
                    ^ ((self.b(self.pos - 7) as u32 & 0xf0) << 12)
                    ^ ((self.b(self.pos - 8) as u32 & 0xf0) << 20),
            )
        } else {
            hashk(0x7800, hn)
        };
        // byte-delta context: differences between consecutive recent bytes —
        // captures gradients/patterns in numeric and tabular data.
        let d1 = (c4 & 0xff).wrapping_sub((c4 >> 8) & 0xff) & 0xff;
        let d2 = ((c4 >> 8) & 0xff).wrapping_sub((c4 >> 16) & 0xff) & 0xff;
        let d3 = ((c4 >> 16) & 0xff).wrapping_sub((c4 >> 24) & 0xff) & 0xff;
        self.ctxhash[88] = hashk(0x7900, d1 | (d2 << 8) | (d3 << 16));
        // Indirect order-5 / order-6 models: the longer base context combined
        // with the recent history of bytes that have followed it. Extends the
        // order-1..4 indirect family to deterministic longer-range structure
        // (helps executable/source repeats the direct long orders miss).
        if self.pos >= 5 {
            let m5 = c4.wrapping_mul(0x9e37_79b1)
                ^ (self.b(self.pos - 5) as u32).wrapping_mul(0x85eb_ca6b);
            let k5 = (m5 >> (32 - FBITS)) as usize;
            self.ctxhash[89] = hashk(0x7A00, m5 ^ self.follow5[k5].wrapping_mul(0xc2b2_ae35));
        } else {
            self.ctxhash[89] = 0;
        }
        if self.pos >= 6 {
            let m6 = c4.wrapping_mul(0x85eb_ca6b)
                ^ (self.b(self.pos - 5) as u32).wrapping_mul(0xc2b2_ae35)
                ^ (self.b(self.pos - 6) as u32).wrapping_mul(0x27d4_eb2f);
            let k6 = (m6 >> (32 - FBITS)) as usize;
            self.ctxhash[90] = hashk(0x7B00, m6 ^ self.follow6[k6].wrapping_mul(0x9e37_79b1));
        } else {
            self.ctxhash[90] = 0;
        }
        // High-nibble indirect: the opcode-class pattern (high nibble of the last
        // four bytes) combined with the recent history of bytes that followed it.
        // Merges the high-nibble and indirect families to capture "what operand
        // byte usually follows this instruction-class pattern" in executables.
        let hnm = (c4 & 0xf0f0_f0f0).wrapping_mul(0x9e37_79b1);
        let khn = (hnm >> (32 - FBITS)) as usize;
        self.ctxhash[91] = hashk(0x7C00, hnm ^ self.followhn[khn].wrapping_mul(0xc2b2_ae35));
        // Byte-delta indirect: the consecutive-difference pattern combined with
        // the recent history of bytes that followed it (numeric/tabular regimes).
        let dm = (d1 | (d2 << 8) | (d3 << 16)).wrapping_mul(0x85eb_ca6b);
        let kd = (dm >> (32 - FBITS)) as usize;
        self.ctxhash[92] = hashk(0x7D00, dm ^ self.followd[kd].wrapping_mul(0x27d4_eb2f));
        // Char-class indirect: the letter/digit/space/other pattern of the last
        // four bytes combined with the bytes that have followed it (text regimes).
        let cck = cls4(c4);
        self.ctxhash[93] = hashk(
            0x7E00,
            cck ^ self.followc[cck as usize].wrapping_mul(0x9e37_79b1),
        );
        // Low-nibble indirect: the low nibbles of the last four bytes (operand /
        // register pattern in code) combined with the bytes that followed it.
        let lnm = (c4 & 0x0f0f_0f0f).wrapping_mul(0xc2b2_ae35);
        let kln = (lnm >> (32 - FBITS)) as usize;
        self.ctxhash[94] = hashk(0x7F00, lnm ^ self.followln[kln].wrapping_mul(0x85eb_ca6b));
        // Gap-bigram indirect: the (last byte, byte three back) sparse pair plus
        // the bytes that followed it.
        let gk = if self.pos >= 3 {
            (c4 & 0xff) | ((self.b(self.pos - 3) as u32) << 8)
        } else {
            c4 & 0xffff
        };
        self.ctxhash[95] = hashk(
            0x8000,
            gk ^ self.followg[gk as usize].wrapping_mul(0x27d4_eb2f),
        );
        // Stride-2 indirect: the (pos-2, pos-4) interleaved pair plus its history.
        let sk = if self.pos >= 4 {
            (self.b(self.pos - 2) as u32) | ((self.b(self.pos - 4) as u32) << 8)
        } else {
            c4 & 0xffff
        };
        self.ctxhash[96] = hashk(
            0x8100,
            sk ^ self.follows2[sk as usize].wrapping_mul(0x9e37_79b1),
        );
        // Stride-3 indirect: the (pos-3, pos-6) pair plus its follow history.
        let s3k = if self.pos >= 6 {
            (self.b(self.pos - 3) as u32) | ((self.b(self.pos - 6) as u32) << 8)
        } else {
            c4 & 0xffff
        };
        self.ctxhash[97] = hashk(
            0x8200,
            s3k ^ self.follows3[s3k as usize].wrapping_mul(0x85eb_ca6b),
        );
        // Wide-gap indirect: the (last byte, byte five back) sparse pair plus its
        // follow history (longer-range sparse structure).
        let g2k = if self.pos >= 5 {
            (c4 & 0xff) | ((self.b(self.pos - 5) as u32) << 8)
        } else {
            c4 & 0xffff
        };
        self.ctxhash[98] = hashk(
            0x8300,
            g2k ^ self.followg2[g2k as usize].wrapping_mul(0xc2b2_ae35),
        );
        // 8-byte high-nibble indirect: extends the (winning) 4-byte high-nibble
        // indirect to an 8-byte opcode-class pattern — longer instruction-class
        // context for executables.
        let hn8 = if self.pos >= 8 {
            hnm ^ ((self.b(self.pos - 5) as u32 & 0xf0) << 4)
                ^ ((self.b(self.pos - 6) as u32 & 0xf0) << 12)
                ^ ((self.b(self.pos - 7) as u32 & 0xf0) << 20)
                ^ ((self.b(self.pos - 8) as u32 & 0xf0).wrapping_mul(0x85eb_ca6b))
        } else {
            hnm
        };
        let khn8 = (hn8 >> (32 - FBITS)) as usize;
        self.ctxhash[99] = hashk(0x8500, hn8 ^ self.followhn8[khn8].wrapping_mul(0x27d4_eb2f));
        // Stride-4 indirect: the (pos-4, pos-8) pair plus its follow history —
        // dword-aligned periodic structure (operand/address tables in code).
        let s4k = if self.pos >= 8 {
            (self.b(self.pos - 4) as u32) | ((self.b(self.pos - 8) as u32) << 8)
        } else {
            c4 & 0xffff
        };
        self.ctxhash[100] = hashk(
            0x8600,
            s4k ^ self.follows4[s4k as usize].wrapping_mul(0x9e37_79b1),
        );
        // 8-byte low-nibble indirect: operand/register-pattern analog of hn8.
        let ln8 = if self.pos >= 8 {
            (c4 & 0x0f0f_0f0f).wrapping_mul(0xc2b2_ae35)
                ^ ((self.b(self.pos - 5) as u32 & 0x0f) << 4)
                ^ ((self.b(self.pos - 6) as u32 & 0x0f) << 12)
                ^ ((self.b(self.pos - 7) as u32 & 0x0f) << 20)
                ^ ((self.b(self.pos - 8) as u32 & 0x0f).wrapping_mul(0x9e37_79b1))
        } else {
            (c4 & 0x0f0f_0f0f).wrapping_mul(0xc2b2_ae35)
        };
        let kln8 = (ln8 >> (32 - FBITS)) as usize;
        self.ctxhash[101] = hashk(0x8700, ln8 ^ self.followln8[kln8].wrapping_mul(0x27d4_eb2f));
        // gap(1,4) indirect: the (last byte, byte four back) sparse pair + history.
        let g14k = if self.pos >= 4 {
            (c4 & 0xff) | ((self.b(self.pos - 4) as u32) << 8)
        } else {
            c4 & 0xffff
        };
        self.ctxhash[102] = hashk(
            0x8800,
            g14k ^ self.followg14[g14k as usize].wrapping_mul(0x85eb_ca6b),
        );
        // 8-byte char-class indirect: extends the char-class indirect to an
        // 8-byte letter/digit/space/other pattern (text / source structure).
        let cc8 = if self.pos >= 8 {
            cls4(c4)
                | (cls4(
                    (self.b(self.pos - 5) as u32)
                        | ((self.b(self.pos - 6) as u32) << 8)
                        | ((self.b(self.pos - 7) as u32) << 16)
                        | ((self.b(self.pos - 8) as u32) << 24),
                ) << 8)
        } else {
            cls4(c4)
        };
        self.ctxhash[103] = hashk(
            0x8900,
            cc8 ^ self.followcc8[cc8 as usize].wrapping_mul(0xc2b2_ae35),
        );
        // Stride-5 indirect: the (pos-5, pos-10) pair plus its follow history.
        let s5k = if self.pos >= 10 {
            (self.b(self.pos - 5) as u32) | ((self.b(self.pos - 10) as u32) << 8)
        } else {
            c4 & 0xffff
        };
        self.ctxhash[104] = hashk(
            0x8A00,
            s5k ^ self.follows5[s5k as usize].wrapping_mul(0x27d4_eb2f),
        );
        // gap(1,6) indirect: the (last byte, byte six back) sparse pair + history.
        let g16k = if self.pos >= 6 {
            (c4 & 0xff) | ((self.b(self.pos - 6) as u32) << 8)
        } else {
            c4 & 0xffff
        };
        self.ctxhash[105] = hashk(
            0x8B00,
            g16k ^ self.followg16[g16k as usize].wrapping_mul(0x9e37_79b1),
        );
    }

    #[inline]
    pub fn predict(&mut self) -> i32 {
        for i in 0..NCTX {
            let h = self.ctxhash[i]
                .wrapping_mul(769)
                .wrapping_add(self.c0 as u32);
            // i < NCTX, so the per-model table index is always valid.
            let row = unsafe { self.tab.get_unchecked_mut(i) };
            let ix = if self.assoc[i] {
                // N-way set-associative: a context maps to a bucket of WAYS slots;
                // pick the way whose checksum matches, else evict the lowest-count
                // way. The whole bucket lives in ~one cache line (array-of-structs).
                let base = ((h & self.bmask[i]) << WAYS_LOG) as usize;
                let chk = (h >> self.cshift[i]) as u8;
                // Slice the WAYS-slot bucket once; indexing a fixed-length slice
                // is bounds-check-free, vs. one check per probe on the full row.
                let bucket = &mut row[base..base + WAYS];
                let mut sel = usize::MAX;
                let mut wl = 0usize;
                let mut lo = bucket[0].cn;
                for k in 0..WAYS {
                    if bucket[k].ck == chk {
                        sel = k;
                        break;
                    }
                    if bucket[k].cn < lo {
                        lo = bucket[k].cn;
                        wl = k;
                    }
                }
                if sel != usize::MAX {
                    base + sel
                } else {
                    bucket[wl] = Slot {
                        cp: 0,
                        cn: 0,
                        st: 0,
                        ck: chk,
                    };
                    base + wl
                }
            } else {
                (h & self.tmask[i]) as usize
            };
            self.idx[i] = ix;
            // ix is always a valid slot index: non-assoc uses `h & tmask` (= len-1)
            // and the assoc branch returns base+way within the 2^tb table.
            let slot = unsafe { *row.get_unchecked(ix) };
            // stretch has 4096 entries; cp+2048 is in [0,4095] (the counter is a
            // contraction toward [0,4095]) and sm>>20 is a 12-bit value, so both
            // indices are always valid — skip the bounds check.
            self.mix_in[i] = unsafe {
                *self
                    .stretch16
                    .get_unchecked((slot.cp as i32 + 32768) as usize)
            };
            let mi = (slot.st as usize) | ((self.bitcount as usize) << 8);
            self.sm_idx[i] = mi;
            // mi <= 2047 by construction (st <= 255 | bitcount<3-bit> << 8).
            let smp = (unsafe { self.sm.get_unchecked(i) }[mi & 2047] >> 16) as usize;
            self.mix_in[SM_BASE + i] = unsafe { *self.stretch16.get_unchecked(smp) };
        }
        self.mm_used = false;
        self.mix_in[MM_BASE] = 0;
        if self.matchlen > 0 && self.predicted_byte >= 0 {
            let sofar = self.c0 - (1 << self.bitcount);
            if sofar == (self.predicted_byte >> (8 - self.bitcount)) {
                let expected_bit = (self.predicted_byte >> (7 - self.bitcount)) & 1;
                let li = if self.matchlen > 32 {
                    32
                } else {
                    self.matchlen
                };
                self.mm_idx = ((li << 1) | expected_bit) as usize;
                self.mix_in[MM_BASE] = unsafe {
                    *self
                        .stretch16
                        .get_unchecked((*self.mm_sm.get_unchecked(self.mm_idx) >> 16) as usize)
                };
                self.mm_used = true;
            } else {
                self.matchlen = 0;
            }
        }
        self.mm_used2 = false;
        self.mix_in[MM_BASE + 1] = 0;
        if self.matchlen2 > 0 && self.predicted_byte2 >= 0 {
            let sofar = self.c0 - (1 << self.bitcount);
            if sofar == (self.predicted_byte2 >> (8 - self.bitcount)) {
                let expected_bit = (self.predicted_byte2 >> (7 - self.bitcount)) & 1;
                let li = if self.matchlen2 > 32 {
                    32
                } else {
                    self.matchlen2
                };
                self.mm_idx2 = ((li << 1) | expected_bit) as usize;
                self.mix_in[MM_BASE + 1] = unsafe {
                    *self
                        .stretch16
                        .get_unchecked((*self.mm_sm2.get_unchecked(self.mm_idx2) >> 16) as usize)
                };
                self.mm_used2 = true;
            } else {
                self.matchlen2 = 0;
            }
        }
        self.mm_used3 = false;
        self.mix_in[MM_BASE + 2] = 0;
        if self.matchlen3 > 0 && self.predicted_byte3 >= 0 {
            let sofar = self.c0 - (1 << self.bitcount);
            if sofar == (self.predicted_byte3 >> (8 - self.bitcount)) {
                let expected_bit = (self.predicted_byte3 >> (7 - self.bitcount)) & 1;
                let li = if self.matchlen3 > 84 {
                    84
                } else {
                    self.matchlen3
                };
                self.mm_idx3 = ((li << 1) | expected_bit) as usize;
                self.mix_in[MM_BASE + 2] = unsafe {
                    *self
                        .stretch16
                        .get_unchecked((*self.mm_sm3.get_unchecked(self.mm_idx3) >> 16) as usize)
                };
                self.mm_used3 = true;
            } else {
                self.matchlen3 = 0;
            }
        }
        self.mm_used4 = false;
        self.mix_in[MM_BASE + 3] = 0;
        if self.matchlen4 > 0 && self.predicted_byte4 >= 0 {
            let sofar = self.c0 - (1 << self.bitcount);
            if sofar == (self.predicted_byte4 >> (8 - self.bitcount)) {
                let expected_bit = (self.predicted_byte4 >> (7 - self.bitcount)) & 1;
                let li = if self.matchlen4 > 72 {
                    72
                } else {
                    self.matchlen4
                };
                self.mm_idx4 = ((li << 1) | expected_bit) as usize;
                self.mix_in[MM_BASE + 3] = unsafe {
                    *self
                        .stretch16
                        .get_unchecked((*self.mm_sm4.get_unchecked(self.mm_idx4) >> 16) as usize)
                };
                self.mm_used4 = true;
            } else {
                self.matchlen4 = 0;
            }
        }
        self.mm_used5 = false;
        self.mix_in[MM_BASE + 4] = 0;
        if self.matchlen5 > 0 && self.predicted_byte5 >= 0 {
            let sofar = self.c0 - (1 << self.bitcount);
            if sofar == (self.predicted_byte5 >> (8 - self.bitcount)) {
                let expected_bit = (self.predicted_byte5 >> (7 - self.bitcount)) & 1;
                let li = if self.matchlen5 > 72 {
                    72
                } else {
                    self.matchlen5
                };
                self.mm_idx5 = ((li << 1) | expected_bit) as usize;
                self.mix_in[MM_BASE + 4] = unsafe {
                    *self
                        .stretch16
                        .get_unchecked((*self.mm_sm5.get_unchecked(self.mm_idx5) >> 16) as usize)
                };
                self.mm_used5 = true;
            } else {
                self.matchlen5 = 0;
            }
        }
        self.mm_used6 = false;
        self.mix_in[MM_BASE + 5] = 0;
        if self.matchlen6 > 0 && self.predicted_byte6 >= 0 {
            let sofar = self.c0 - (1 << self.bitcount);
            if sofar == (self.predicted_byte6 >> (8 - self.bitcount)) {
                let expected_bit = (self.predicted_byte6 >> (7 - self.bitcount)) & 1;
                let li = if self.matchlen6 > 32 {
                    32
                } else {
                    self.matchlen6
                };
                self.mm_idx6 = ((li << 1) | expected_bit) as usize;
                self.mix_in[MM_BASE + 5] = unsafe {
                    *self
                        .stretch16
                        .get_unchecked((*self.mm_sm6.get_unchecked(self.mm_idx6) >> 16) as usize)
                };
                self.mm_used6 = true;
            } else {
                self.matchlen6 = 0;
            }
        }
        // DMC variable-order Markov prediction — one extra mixer input.
        self.mix_in[DMC_IN] = self.dmc.predict(&self.stretch);
        self.mix_in[DMC2_IN] = self.dmc2.predict(&self.stretch);
        self.mix_in[DMC3_IN] = self.dmc3.predict(&self.stretch);
        self.mix_in[DMC4_IN] = self.dmc4.predict(&self.stretch);
        self.mix_in[DMC5_IN] = self.dmc5.predict(&self.stretch);
        self.mix_in[CTW_IN] = self.ctw.predict(&self.stretch);
        // RunContextMap bank. At the first bit of a byte, read each run map's slot
        // (its context hash is fixed for the whole byte); for every bit, if the
        // partial byte still matches the remembered byte, emit that byte's next bit
        // at a confidence the per-map StateMap has learned for this run length.
        if self.bitcount == 0 {
            for j in 0..NRUN {
                let h = self.ctxhash[RUN_CTX[j]];
                let slot = self.runtab[j][(h & self.runmask) as usize];
                if (slot >> 16) as u8 == (h >> self.runbits) as u8 && ((slot >> 8) & 0xff) != 0 {
                    self.run_pbyte[j] = (slot & 0xff) as i32;
                    self.run_cnt[j] = ((slot >> 8) & 0xff) as i32;
                } else {
                    self.run_pbyte[j] = -1;
                    self.run_cnt[j] = 0;
                }
            }
        }
        // Loop invariants (same for all run maps at this bit) hoisted out; and once
        // a run map's partial byte diverges from its remembered byte it can never
        // re-match within this byte, so prune it (pbyte=-1) and later bits skip it.
        // Both are output-neutral: a non-matching map contributes 0 either way.
        let sofar = self.c0 - (1 << self.bitcount);
        let sh8 = 8 - self.bitcount;
        let sh7 = 7 - self.bitcount;
        for j in 0..NRUN {
            self.run_used[j] = false;
            self.mix_in[RUN_BASE + j] = 0;
            let pb = self.run_pbyte[j];
            if pb >= 0 {
                if sofar == (pb >> sh8) {
                    let expected_bit = (pb >> sh7) & 1;
                    let cnt = self.run_cnt[j].min(63);
                    let sidx = ((cnt << 1) | expected_bit) as usize;
                    self.run_idx[j] = sidx;
                    // prob is the top 16 bits of the 22-bit fixed point (>>10 then >>6).
                    let smp = (self.run_sm[j][sidx] >> 16) as usize;
                    self.mix_in[RUN_BASE + j] =
                        unsafe { *self.stretch16.get_unchecked(smp) };
                    self.run_used[j] = true;
                } else {
                    self.run_pbyte[j] = -1; // diverged — skip for the rest of this byte
                }
            }
        }
        // Pre-widen the full input vector to i64 once; all 27 layer-1 mixers dot
        // the same vector, so this lifts the per-element sign-extend out of the
        // hot loop (run 27x per bit) into a single pass.
        for i in 0..NINPUT {
            self.mix_in64[i] = self.mix_in[i] as i64;
        }
        // Layer-1 specialist mixers, each selected by a different context:
        //   m0 — the proven last-byte + match-activity context (full resolution)
        //   m1 — the within-byte partial-byte context (order-0 bit position)
        //   m2 — the second-to-last byte (an order-2-distance specialist)
        let ctx0 = (((if self.matchlen4 > 0 { 1 } else { 0 }) << 13)
            | ((if self.matchlen3 > 72 { 1 } else { 0 }) << 12)
            | ((if self.matchlen3 > 52 { 1 } else { 0 }) << 11)
            | ((if self.matchlen3 > 0 { 1 } else { 0 }) << 10)
            | ((if self.matchlen2 > 0 { 1 } else { 0 }) << 9)
            | ((if self.matchlen > 0 { 1 } else { 0 }) << 8)
            | self.c1) as usize;
        let ctx1 = self.c0 as usize;
        let ctx2 = ((self.c4 >> 8) & 0xff) as usize;
        let ctx3 = (self.c4 & 0xffff) as usize;
        let ctx4 = ((self.c4 & 0xffffff).wrapping_mul(0x9e37_79b1) >> 13) as usize;
        self.l1[0].ctx = (ctx0) & (self.l1[0].nctx - 1);
        self.l1[1].ctx = (ctx1) & (self.l1[1].nctx - 1);
        self.l1[2].ctx = (ctx2) & (self.l1[2].nctx - 1);
        self.l1[3].ctx = (ctx3) & (self.l1[3].nctx - 1);
        self.l1[4].ctx = (ctx4) & (self.l1[4].nctx - 1);
        let ctx5 = ((self.matchlen.min(15) as usize) << 2)
            | (if self.matchlen3 > 0 { 2 } else { 0 })
            | (if self.matchlen4 > 0 { 1 } else { 0 });
        self.l1[5].ctx = (ctx5) & (self.l1[5].nctx - 1);
        let ctx6 = (((self.col & 63) << 6) | (self.c1 as u32 & 63)) as usize;
        self.l1[6].ctx = (ctx6) & (self.l1[6].nctx - 1);
        let ctx7 = (self.c4.wrapping_mul(0x9e37_79b1) >> 19) as usize;
        self.l1[7].ctx = (ctx7) & (self.l1[7].nctx - 1);
        let ctx8 = if self.pos >= 6 {
            (self.c4.wrapping_mul(0x85eb_ca6b)
                ^ (self.b(self.pos - 5) as u32).wrapping_mul(0xc2b2_ae35)
                ^ (self.b(self.pos - 6) as u32).wrapping_mul(0x27d4_eb2f)) as usize
        } else {
            self.c4 as usize
        };
        self.l1[8].ctx = (ctx8) & (self.l1[8].nctx - 1);
        // stride-2 sparse selector: bytes at pos-2 and pos-4 (interleaved structure).
        let ctx9 = if self.pos >= 4 {
            (self.b(self.pos - 2) as usize) | ((self.b(self.pos - 4) as usize) << 8)
        } else {
            self.c1 as usize
        };
        self.l1[9].ctx = (ctx9) & (self.l1[9].nctx - 1);
        // stride-3 sparse selector: bytes at pos-3 and pos-6.
        let ctx10 = if self.pos >= 6 {
            (self.b(self.pos - 3) as usize) | ((self.b(self.pos - 6) as usize) << 8)
        } else {
            self.c1 as usize
        };
        self.l1[10].ctx = (ctx10) & (self.l1[10].nctx - 1);
        // byte-above selector (2D structure): specialise on the char one line up.
        let ctx11 = (self.above_byte as usize) | ((self.c1 as usize & 1) << 9);
        self.l1[11].ctx = (ctx11) & (self.l1[11].nctx - 1);
        // specialise on the order-2 indirect prediction (the byte that most
        // recently followed this 2-byte context).
        self.l1[12].ctx = (self.ind_pred as usize) & (self.l1[12].nctx - 1);
        // nest-state selector: specialise on the enclosing bracket + nesting depth.
        let nestsel = if self.nest_depth > 0 {
            (self.nest_stack[self.nest_depth - 1] as usize) | ((self.nest_depth & 3) << 8)
        } else {
            0
        };
        self.l1[13].ctx = (nestsel) & (self.l1[13].nctx - 1);
        // high-nibble (opcode-class) selector.
        let hnsel = ((self.c4 & 0xf0f0_f0f0).wrapping_mul(0x9e37_79b1) >> 20) as usize;
        self.l1[14].ctx = (hnsel) & (self.l1[14].nctx - 1);
        // character-class selector (letter/digit/space/other of last 4 bytes) —
        // a coarse semantic text-mode grouping (analogous to the high-nibble one).
        let cls = |b: u32| -> usize {
            let b = b & 0xff;
            if (b >= 97 && b <= 122) || (b >= 65 && b <= 90) {
                1
            } else if b >= 48 && b <= 57 {
                2
            } else if b == 32 || b == 9 || b == 10 || b == 13 {
                3
            } else {
                0
            }
        };
        let ccsel = cls(self.c4)
            | (cls(self.c4 >> 8) << 2)
            | (cls(self.c4 >> 16) << 4)
            | (cls(self.c4 >> 24) << 6);
        self.l1[15].ctx = (ccsel) & (self.l1[15].nctx - 1);
        // combined mode selector: last byte's high nibble + char-class of the
        // last two bytes (a richer visual+semantic mode than either alone).
        let modesel =
            ((self.c4 & 0xf0) >> 4) as usize | (cls(self.c4) << 4) | (cls(self.c4 >> 8) << 6);
        self.l1[16].ctx = (modesel) & (self.l1[16].nctx - 1);
        // run-length regime selector: bucket the current run length with the
        // class of the last byte — distinguishes "in a long run" from "varying".
        let runb = {
            let r = self.run_len;
            if r <= 1 {
                0
            } else if r == 2 {
                1
            } else if r <= 4 {
                2
            } else if r <= 8 {
                3
            } else if r <= 16 {
                4
            } else if r <= 64 {
                5
            } else if r <= 256 {
                6
            } else {
                7
            }
        };
        let runsel = runb | (cls(self.c4) << 3);
        self.l1[17].ctx = (runsel) & (self.l1[17].nctx - 1);
        // gradient / delta-sign selector: coarse sign (zero/up/down) of the last
        // three consecutive byte differences — a "numeric trend" mode.
        let dsign = |a: u32, b: u32| -> usize {
            let d = (a & 0xff).wrapping_sub(b & 0xff) & 0xff;
            if d == 0 {
                0
            } else if d < 128 {
                1
            } else {
                2
            }
        };
        let gradsel = dsign(self.c4, self.c4 >> 8)
            + 3 * dsign(self.c4 >> 8, self.c4 >> 16)
            + 9 * dsign(self.c4 >> 16, self.c4 >> 24);
        self.l1[18].ctx = (gradsel) & (self.l1[18].nctx - 1);
        // periodic / record selector: when the period detector is confident,
        // specialise on the coarse value of the byte one period back.
        let rgl = self.rlen;
        let rec_ok_l = self.rcount > 8 && rgl >= 2 && rgl < self.pos;
        let recsel = if rec_ok_l {
            1 + ((self.b(self.pos - rgl) as usize) >> 5)
        } else {
            0
        };
        self.l1[19].ctx = (recsel) & (self.l1[19].nctx - 1);
        // above-char-class + nesting selector: a 2D / structural mode keyed on
        // the class of the char one line up and the current bracket depth.
        let aboveclass = if self.above_byte > 255 {
            4
        } else {
            cls(self.above_byte)
        };
        let abovesel = aboveclass | ((self.nest_depth & 7) << 3);
        self.l1[20].ctx = (abovesel) & (self.l1[20].nctx - 1);
        // gradient-magnitude selector: bucket the magnitude of the last byte
        // difference (flat / small / medium / large) with the last-byte class —
        // a smooth-vs-noisy numeric mode, distinct from the delta-sign selector.
        let dmag = {
            let d = (self.c4 & 0xff).wrapping_sub((self.c4 >> 8) & 0xff) & 0xff;
            let m = if d >= 128 { 256 - d } else { d };
            if m == 0 {
                0
            } else if m <= 2 {
                1
            } else if m <= 8 {
                2
            } else if m <= 32 {
                3
            } else {
                4
            }
        };
        let gmagsel = dmag | (cls(self.c4) << 3);
        self.l1[21].ctx = (gmagsel) & (self.l1[21].nctx - 1);
        // GLN-style halfspace gate: the sign-agreement pattern of GLN_BITS base
        // predictions (order-0..6 direct counters + their bit-history StateMaps).
        // Unlike the byte-context gates above, this partitions the input space by
        // *which models lean toward 1* — the GLN's data-dependent gating.
        // True-halfspace GLN specialists: each gate bit = sign(<hyperplane, preds>).
        for s in 0..NGLN_HS {
            let mut glngate = 0usize;
            for k in 0..GLN_BITS as usize {
                let hp = &self.gln_hp[s][k];
                let mut proj = 0i64;
                for i in 0..GLN_NSEL {
                    proj += hp[i] as i64 * self.mix_in[GLN_SEL[i]] as i64;
                }
                glngate |= ((proj > 0) as usize) << k;
            }
            self.l1[22 + s].ctx = glngate & (self.l1[22 + s].nctx - 1);
        }
        // Axis-aligned GLN specialist: sign of each base prediction (complementary
        // partition to the weighted-halfspace gates above).
        let mut glngate2 = 0usize;
        for k in 0..(GLN_BITS as usize / 2) {
            glngate2 |= ((self.mix_in[k] > 0) as usize) << k;
            glngate2 |= ((self.mix_in[SM_BASE + k] > 0) as usize) << (GLN_BITS as usize / 2 + k);
        }
        self.l1[22 + NGLN_HS].ctx = glngate2 & (self.l1[22 + NGLN_HS].nctx - 1);
        // High-level predictor sign-agreement gate: complementary axis partition
        // over the 6 match models + DMC + CTW (the long-range/structural predictors
        // the order-0..6 axis gate above does not cover). 8 signs -> 256 rows.
        let mut glngate3 = 0usize;
        for k in 0..6 {
            glngate3 |= ((self.mix_in[MM_BASE + k] > 0) as usize) << k;
        }
        glngate3 |= ((self.mix_in[DMC_IN] > 0) as usize) << 6;
        glngate3 |= ((self.mix_in[CTW_IN] > 0) as usize) << 7;
        self.l1[22 + NGLN_HS + 1].ctx = glngate3 & (self.l1[22 + NGLN_HS + 1].nctx - 1);
        // High-level predictor confidence gate: how *strongly* (not just which way)
        // the long-range predictors lean. Buckets |logit| of CTW, DMC, and the two
        // primary match models into 2 bits each (0=uncertain..3=near-certain).
        // Orthogonal to the sign-agreement gate: the ideal blend of the local
        // context models differs when the structural predictors are confident.
        let cbucket = |v: i32| -> usize { ((v.unsigned_abs() >> 9).min(3)) as usize };
        let glngate5 = cbucket(self.mix_in[CTW_IN])
            | (cbucket(self.mix_in[DMC_IN]) << 2)
            | (cbucket(self.mix_in[MM_BASE]) << 4)
            | (cbucket(self.mix_in[MM_BASE + 1]) << 6);
        self.l1[22 + NGLN_HS + 2].ctx = (glngate5 | ((self.bitcount as usize) << 8))
            & (self.l1[22 + NGLN_HS + 2].nctx - 1);
        // Local-context confidence gate: the confidence of the mid-order direct
        // counters (order-2/3/4/6). Complements the high-level confidence gate —
        // when the local context models are sharp the mixer should weight them
        // over the structural predictors, and vice-versa.
        let glngate6 = cbucket(self.mix_in[2])
            | (cbucket(self.mix_in[3]) << 2)
            | (cbucket(self.mix_in[4]) << 4)
            | (cbucket(self.mix_in[6]) << 6);
        self.l1[22 + NGLN_HS + 3].ctx = glngate6 & (self.l1[22 + NGLN_HS + 3].nctx - 1);
        // Word-model confidence gate: how sharply the word / n-gram context models
        // (word, word-bigram, word-trigram, word-4gram) are predicting. Targets the
        // natural-language files, where the mixer should defer to the word bank when
        // it locks onto a known word and fall back to bytes when it does not.
        let glngate7 = cbucket(self.mix_in[7])
            | (cbucket(self.mix_in[23]) << 2)
            | (cbucket(self.mix_in[25]) << 4)
            | (cbucket(self.mix_in[66]) << 6);
        self.l1[22 + NGLN_HS + 4].ctx = glngate7 & (self.l1[22 + NGLN_HS + 4].nctx - 1);
        // Bit-history StateMap confidence gate: the sharpness of the order-1/5/7
        // StateMaps — a different predictor family (nonstationary bit-history) than
        // the direct counters of the local-confidence gate, spread across orders.
        let glngate10 = cbucket(self.mix_in[SM_BASE + 1])
            | (cbucket(self.mix_in[SM_BASE + 5]) << 2)
            | (cbucket(self.mix_in[SM_BASE + 7]) << 4)
            | (cbucket(self.mix_in[SM_BASE + 11]) << 6);
        self.l1[22 + NGLN_HS + 5].ctx = (glngate10 | ((self.bitcount as usize) << 8))
            & (self.l1[22 + NGLN_HS + 5].nctx - 1);
        // Word/mid-order StateMap + long-match confidence gate: sharpness of the
        // word-context StateMaps (indices 9/12/13) plus the order-8 match model —
        // a distinct set of contexts from the low-order StateMap gate above.
        let glngate11 = cbucket(self.mix_in[SM_BASE + 9])
            | (cbucket(self.mix_in[SM_BASE + 12]) << 2)
            | (cbucket(self.mix_in[MM_BASE + 2]) << 4)
            | (cbucket(self.mix_in[SM_BASE + 13]) << 6);
        self.l1[22 + NGLN_HS + 6].ctx = glngate11 & (self.l1[22 + NGLN_HS + 6].nctx - 1);
        // Fine dual-confidence gate: 3-bit confidence buckets over the two most
        // informative predictors (CTW and the order-9 word StateMap) plus the match
        // sign — a finer partition of the two dominant signals than the coarse gates.
        let fbucket = |v: i32| -> usize { ((v.unsigned_abs() >> 8).min(7)) as usize };
        let glngate12 = fbucket(self.mix_in[CTW_IN])
            | (fbucket(self.mix_in[SM_BASE + 9]) << 3)
            | (((self.mix_in[MM_BASE] > 0) as usize) << 6)
            | (((self.mix_in[MM_BASE + 1] > 0) as usize) << 7);
        self.l1[22 + NGLN_HS + 7].ctx = glngate12 & (self.l1[22 + NGLN_HS + 7].nctx - 1);
        // Difficulty-regime specialist: selected by an EMA of recent per-bit coding
        // surprise (how well the whole ensemble has been predicting lately). A novel
        // meta-signal — no existing selector sees the aggregate recent error, only
        // individual predictor confidence. 16 difficulty levels.
        self.l1[22 + NGLN_HS + 8].ctx = (((self.hard >> 11).min(31) as usize)
            | ((self.bitcount as usize) << 5))
            & (self.l1[22 + NGLN_HS + 8].nctx - 1);
        // Difficulty-trend specialist: fast (~8-bit window) vs slow (~128-bit) EMA
        // of coding surprise — whether the data is getting harder or easier right
        // now, and by how much. Captures regime transitions (a new block/format
        // starting) that the level-only gate cannot distinguish from steady-state.
        let trend = ((self.hard_fast - self.hard_slow) >> 12).clamp(-4, 3) + 4; // 0..7
        self.l1[22 + NGLN_HS + 9].ctx = ((((self.hard_slow >> 13).min(3) as usize) << 3)
            | trend as usize
            | ((self.bitcount as usize) << 5))
            & (self.l1[22 + NGLN_HS + 9].nctx - 1);
        // delta sign+magnitude selector: the last byte difference bucketed by
        // both sign and coarse magnitude (numeric trend, finer than sign alone).
        let dsm = {
            let d = (self.c4 & 0xff).wrapping_sub((self.c4 >> 8) & 0xff) & 0xff;
            let neg = d >= 128;
            let m = if neg { 256 - d } else { d };
            let mb = if m == 0 {
                0
            } else if m <= 4 {
                1
            } else if m <= 32 {
                2
            } else {
                3
            };
            (mb | (if neg { 4 } else { 0 })) as usize
        };
        let dsmsel = dsm | (cls(self.c4) << 3);
        // Fused layer-1 dot products. All 27 specialists dot the *same* input
        // vector under their own (already-selected) weight row, so iterate the
        // inputs once — loading each mix_in64[i] a single time — and accumulate
        // into the 27 dot products in parallel, instead of re-reading the whole
        // input vector inside 27 separate loops. Per-row order is unchanged, so
        // each dot (and thus the output) is identical.
        {
            let n = NINPUT;
            let mut rows: [&[i32]; NL1] = [&[][..]; NL1];
            for k in 0..NL1 {
                let base = self.l1[k].ctx * n;
                rows[k] = unsafe { self.l1[k].w.get_unchecked(base..base + n) };
            }
            let mut dot = [0i64; NL1];
            for i in 0..n {
                let xi = self.mix_in64[i];
                // Skip zero inputs: a 0 contributes 0 to every row's dot product, so
                // skipping the NL1 mul-adds is output-neutral. Match-model and run-map
                // inputs are exactly 0 whenever unused (common), so this is a real
                // operator-count (WORK) reduction at byte-identical output.
                if xi != 0 {
                    for k in 0..NL1 {
                        dot[k] += unsafe { *rows[k].get_unchecked(i) } as i64 * xi;
                    }
                }
            }
            for k in 0..NL1 {
                let mut d = (dot[k] >> 16) as i32;
                if d > 2047 {
                    d = 2047;
                }
                if d < -2047 {
                    d = -2047;
                }
                let mut p = squash_d(&self.squash, d);
                if p < 1 {
                    p = 1;
                }
                if p > 4094 {
                    p = 4094;
                }
                self.l1[k].pr = p;
                self.l2_in[k] = d;
            }
        }
        // Two layer-2 combiners over the layer-1 logits — one keyed on the last
        // byte, one on the within-byte bit position — averaged in the logit domain.
        // Pre-widen the 27 layer-1 logits once for the 10 layer-2 combiners.
        for k in 0..NL1 {
            self.l2_in64[k] = self.l2_in[k] as i64;
        }
        // Fused layer-2 dot products: the 10 combiners all dot the same 27-wide
        // logit vector, so load each input once and accumulate into all 10 dots.
        let l2cctx = ((self.matchlen.min(15) as usize) << 2)
            | (if self.matchlen3 > 0 { 2 } else { 0 })
            | (if self.matchlen4 > 0 { 1 } else { 0 });
        let l2ectx = if self.wordhash != 0 {
            (self.wordhash.wrapping_mul(0x9e37_79b1) >> 24) as usize
        } else {
            self.c1 as usize
        };
        let l2fctx = ((self.c4 & 0xf0f0_f0f0).wrapping_mul(0x9e37_79b1) >> 24) as usize;
        let l2gctx = cls(self.c4)
            | (cls(self.c4 >> 8) << 2)
            | (cls(self.c4 >> 16) << 4)
            | (cls(self.c4 >> 24) << 6);
        let l2hctx = if self.nest_depth > 0 {
            (self.nest_stack[self.nest_depth - 1] as usize) | ((self.nest_depth & 3) << 8)
        } else {
            0
        };
        let l2ictx = (self.above_byte as usize) | ((self.c1 as usize & 1) << 9);
        // numeric-regime combiner: keyed on the byte-delta pattern of the last three.
        let l2jctx = (dsmsel & 0xff) | ((dsign(self.c4 >> 8, self.c4 >> 16)) << 6);
        self.l2.ctx = (self.c1 as usize) & (self.l2.nctx - 1);
        self.l2b.ctx = (self.c0 as usize) & (self.l2b.nctx - 1);
        self.l2c.ctx = l2cctx & (self.l2c.nctx - 1);
        self.l2d.ctx = (((self.c4 >> 8) & 0xff) as usize) & (self.l2d.nctx - 1);
        self.l2e.ctx = l2ectx & (self.l2e.nctx - 1);
        self.l2f.ctx = l2fctx & (self.l2f.nctx - 1);
        self.l2g.ctx = l2gctx & (self.l2g.nctx - 1);
        self.l2h.ctx = l2hctx & (self.l2h.nctx - 1);
        self.l2i.ctx = l2ictx & (self.l2i.nctx - 1);
        self.l2j.ctx = (l2jctx & 0xff) & (self.l2j.nctx - 1);
        // difficulty-regime combiner: reweight the specialists by how hard the
        // data has been lately (same 32-level signal as the difficulty specialist,
        // but applied one layer up, over the specialist logits).
        self.l2k.ctx = (((self.hard >> 11).min(31) as usize)
            | ((self.bitcount as usize) << 5))
            & (self.l2k.nctx - 1);
        // high-level-confidence combiner: reweight the specialists by how strongly
        // the long-range predictors (CTW/DMC/two primary matches) commit — the
        // same 256-row signal as the confidence gate specialist, one layer up.
        self.l2l.ctx = {
            let cb2 = |v: i32| -> usize { ((v.unsigned_abs() >> 9).min(3)) as usize };
            (cb2(self.mix_in[CTW_IN])
                | (cb2(self.mix_in[DMC_IN]) << 2)
                | (cb2(self.mix_in[MM_BASE]) << 4)
                | (cb2(self.mix_in[MM_BASE + 1]) << 6)
                | ((self.bitcount as usize) << 8))
                & (self.l2l.nctx - 1)
        };
        // StateMap-confidence combiner: reweight the specialists by how sharply the
        // order-1/5/7/11 bit-history StateMaps predict (PR #131's -52 gate signal,
        // one layer up over the specialist logits).
        self.l2m.ctx = {
            let cb3 = |v: i32| -> usize { ((v.unsigned_abs() >> 9).min(3)) as usize };
            (cb3(self.mix_in[SM_BASE + 1])
                | (cb3(self.mix_in[SM_BASE + 5]) << 2)
                | (cb3(self.mix_in[SM_BASE + 7]) << 4)
                | (cb3(self.mix_in[SM_BASE + 11]) << 6)
                | ((self.bitcount as usize) << 8))
                & (self.l2m.nctx - 1)
        };
        // local-counter-confidence combiner: reweight the specialists by how sharp
        // the mid-order direct counters (order-2/3/4/6) are (PR #128's -67 gate
        // signal, one layer up).
        self.l2n.ctx = {
            let cb4 = |v: i32| -> usize { ((v.unsigned_abs() >> 9).min(3)) as usize };
            (cb4(self.mix_in[2])
                | (cb4(self.mix_in[3]) << 2)
                | (cb4(self.mix_in[4]) << 4)
                | (cb4(self.mix_in[6]) << 6))
                & (self.l2n.nctx - 1)
        };
        // word-model-confidence combiner: reweight the specialists by how sharply
        // the word / n-gram models (word, bigram, trigram, 4-gram) predict (PR
        // #129's word gate signal, one layer up). Targets natural-language text.
        self.l2o.ctx = {
            let cb5 = |v: i32| -> usize { ((v.unsigned_abs() >> 9).min(3)) as usize };
            (cb5(self.mix_in[7])
                | (cb5(self.mix_in[23]) << 2)
                | (cb5(self.mix_in[25]) << 4)
                | (cb5(self.mix_in[66]) << 6))
                & (self.l2o.nctx - 1)
        };
        // word/mid-order StateMap-confidence combiner: sharpness of the word-context
        // StateMaps (9/12/13) + the order-8 match model (PR #132's gate signal, one
        // layer up).
        self.l2p.ctx = {
            let cb6 = |v: i32| -> usize { ((v.unsigned_abs() >> 9).min(3)) as usize };
            (cb6(self.mix_in[SM_BASE + 9])
                | (cb6(self.mix_in[SM_BASE + 12]) << 2)
                | (cb6(self.mix_in[MM_BASE + 2]) << 4)
                | (cb6(self.mix_in[SM_BASE + 13]) << 6))
                & (self.l2p.nctx - 1)
        };
        let mut dd = [0i64; 16];
        {
            let rows: [&[i32]; 16] = [
                {
                    let b = self.l2.ctx * NL1;
                    unsafe { self.l2.w.get_unchecked(b..b + NL1) }
                },
                {
                    let b = self.l2b.ctx * NL1;
                    unsafe { self.l2b.w.get_unchecked(b..b + NL1) }
                },
                {
                    let b = self.l2c.ctx * NL1;
                    unsafe { self.l2c.w.get_unchecked(b..b + NL1) }
                },
                {
                    let b = self.l2d.ctx * NL1;
                    unsafe { self.l2d.w.get_unchecked(b..b + NL1) }
                },
                {
                    let b = self.l2e.ctx * NL1;
                    unsafe { self.l2e.w.get_unchecked(b..b + NL1) }
                },
                {
                    let b = self.l2f.ctx * NL1;
                    unsafe { self.l2f.w.get_unchecked(b..b + NL1) }
                },
                {
                    let b = self.l2g.ctx * NL1;
                    unsafe { self.l2g.w.get_unchecked(b..b + NL1) }
                },
                {
                    let b = self.l2h.ctx * NL1;
                    unsafe { self.l2h.w.get_unchecked(b..b + NL1) }
                },
                {
                    let b = self.l2i.ctx * NL1;
                    unsafe { self.l2i.w.get_unchecked(b..b + NL1) }
                },
                {
                    let b = self.l2j.ctx * NL1;
                    unsafe { self.l2j.w.get_unchecked(b..b + NL1) }
                },
                {
                    let b = self.l2k.ctx * NL1;
                    unsafe { self.l2k.w.get_unchecked(b..b + NL1) }
                },
                {
                    let b = self.l2l.ctx * NL1;
                    unsafe { self.l2l.w.get_unchecked(b..b + NL1) }
                },
                {
                    let b = self.l2m.ctx * NL1;
                    unsafe { self.l2m.w.get_unchecked(b..b + NL1) }
                },
                {
                    let b = self.l2n.ctx * NL1;
                    unsafe { self.l2n.w.get_unchecked(b..b + NL1) }
                },
                {
                    let b = self.l2o.ctx * NL1;
                    unsafe { self.l2o.w.get_unchecked(b..b + NL1) }
                },
                {
                    let b = self.l2p.ctx * NL1;
                    unsafe { self.l2p.w.get_unchecked(b..b + NL1) }
                },
            ];
            for i in 0..NL1 {
                let xi = self.l2_in64[i];
                for j in 0..16 {
                    dd[j] += unsafe { *rows[j].get_unchecked(i) } as i64 * xi;
                }
            }
        }
        let mut dsum = 0i32;
        let mut dv = [0i32; 16];
        for j in 0..16 {
            let mut v = (dd[j] >> 16) as i32;
            if v > 2047 {
                v = 2047;
            }
            if v < -2047 {
                v = -2047;
            }
            dv[j] = v;
            dsum += v;
        }
        let mut prs = [0i32; 16];
        for j in 0..16 {
            let mut pp = squash_d(&self.squash, dv[j]);
            if pp < 1 {
                pp = 1;
            }
            if pp > 4094 {
                pp = 4094;
            }
            prs[j] = pp;
        }
        self.l2.pr = prs[0];
        self.l2b.pr = prs[1];
        self.l2c.pr = prs[2];
        self.l2d.pr = prs[3];
        self.l2e.pr = prs[4];
        self.l2f.pr = prs[5];
        self.l2g.pr = prs[6];
        self.l2h.pr = prs[7];
        self.l2i.pr = prs[8];
        self.l2j.pr = prs[9];
        self.l2k.pr = prs[10];
        self.l2l.pr = prs[11];
        self.l2m.pr = prs[12];
        self.l2n.pr = prs[13];
        self.l2o.pr = prs[14];
        self.l2p.pr = prs[15];
        // Squash the combined logit straight to 16-bit and run the whole SSE/APM
        // chain at 16-bit precision (the calibration tables are ~16-bit), so no
        // stage re-quantizes the probability to the 12-bit 1/4096 grid.
        let mut p = squash16_d(&self.squash16, dsum / 16);
        if p < 1 {
            p = 1;
        }
        if p > 65534 {
            p = 65534;
        }

        let a1ctx = ((self.c1 | (if self.matchlen > 0 { 256 } else { 0 })) as usize) & 1023;
        let a1 = self.apm1.apply16(&self.stretch16, a1ctx, p);
        p = (p + a1) >> 1;
        if p < 1 {
            p = 1;
        }
        if p > 65534 {
            p = 65534;
        }
        let a2 = self
            .apm2
            .apply16(&self.stretch16, (self.c4 & 0x3fff) as usize, p);
        p = (p + a2) >> 1;
        if p < 1 {
            p = 1;
        }
        if p > 65534 {
            p = 65534;
        }
        let a3ctx = (self.c0 as usize)
            | (if self.matchlen > 0 { 256 } else { 0 })
            | (if self.matchlen3 > 0 { 512 } else { 0 });
        let a3 = self.apm3.apply16(&self.stretch16, a3ctx, p);
        p = (p + a3) >> 1;
        if p < 1 {
            p = 1;
        }
        if p > 65534 {
            p = 65534;
        }
        // Match-length SSE: calibrate by how long the current order-6 match runs.
        let a4ctx = ((self.matchlen as usize) & 0xff)
            | (if self.matchlen3 > 0 { 256 } else { 0 })
            | (if self.matchlen4 > 0 { 512 } else { 0 });
        let a4 = self.apm4.apply16(&self.stretch16, a4ctx, p);
        p = (3 * p + a4) >> 2;
        if p < 1 {
            p = 1;
        }
        if p > 65534 {
            p = 65534;
        }
        p
    }

    #[inline]
    pub fn update(&mut self, bit: i32, _p: i32) {
        let t = if bit != 0 { 65535 } else { 0 };
        // Update the recent-difficulty EMA: surprise = distance of the final
        // prediction from the observed bit (0 = perfectly confident+correct, large
        // = confidently wrong). Window ~32 bits. Read by the difficulty specialist.
        let surprise = (t - _p).abs();
        self.hard += (surprise - self.hard) >> 5;
        self.hard_fast += (surprise - self.hard_fast) >> 3;
        self.hard_slow += (surprise - self.hard_slow) >> 7;
        self.apm1.update(bit);
        self.apm2.update(bit);
        self.apm3.update(bit);
        self.apm4.update(bit);
        self.dmc.update(bit);
        self.dmc2.update(bit);
        self.dmc3.update(bit);
        self.dmc4.update(bit);
        self.dmc5.update(bit);
        self.ctw.update(bit);
        // Match-model StateMaps, count-based 1/(cnt+K) adaptation (cells pack
        // prob22<<10 | count, like the run maps and the main context StateMap).
        macro_rules! mm_sm_update {
            ($arr:ident, $idx:ident) => {{
                let entry = self.$arr[self.$idx];
                let cnt = (entry & 1023) as i32;
                let p22 = (entry >> 10) as i32;
                let newp = p22 + (((bit << 22) - p22) / (cnt + MM_SM_K));
                let newcnt = if cnt < MM_SM_CAP { cnt + 1 } else { MM_SM_CAP };
                self.$arr[self.$idx] = ((newp as u32) << 10) | (newcnt as u32);
            }};
        }
        if self.mm_used {
            mm_sm_update!(mm_sm, mm_idx);
        }
        if self.mm_used2 {
            mm_sm_update!(mm_sm2, mm_idx2);
        }
        if self.mm_used3 {
            mm_sm_update!(mm_sm3, mm_idx3);
        }
        if self.mm_used4 {
            mm_sm_update!(mm_sm4, mm_idx4);
        }
        if self.mm_used5 {
            mm_sm_update!(mm_sm5, mm_idx5);
        }
        if self.mm_used6 {
            mm_sm_update!(mm_sm6, mm_idx6);
        }
        // Run-map StateMaps: adapt the (run-length, expected-bit) cell each run map
        // read this bit toward the observed bit, learning how reliable each run is.
        for j in 0..NRUN {
            if self.run_used[j] {
                let entry = self.run_sm[j][self.run_idx[j]];
                let cnt = (entry & 1023) as i32;
                let p22 = (entry >> 10) as i32;
                let newp = p22 + (((bit << 22) - p22) / (cnt + RUN_SM_K));
                let newcnt = if cnt < RUN_SM_CAP { cnt + 1 } else { RUN_SM_CAP };
                self.run_sm[j][self.run_idx[j]] = ((newp as u32) << 10) | (newcnt as u32);
            }
        }
        // Fused layer-1 weight update (mirror of the fused dot in `predict`): all
        // 27 specialists train on the same input vector, so load each mix_in[i]
        // once and apply it to all 27 weight rows in one pass. Each row's update
        // is identical to the per-mixer loop: w[i] += (x[i] * err_k * lr_k) >> 16,
        // with err_k*lr_k folded once (both fit i32, so the value is unchanged).
        {
            let n = NINPUT;
            let mut errlr = [0i32; NL1];
            let mut rowp: [*mut i32; NL1] = [core::ptr::null_mut::<i32>(); NL1];
            for k in 0..NL1 {
                let m = &mut self.l1[k];
                errlr[k] = ((bit << 12) - m.pr) * m.lr;
                let base = m.ctx * n;
                // SAFETY: base + n <= w.len() (ctx < nctx), and the 27 rows live in
                // distinct, non-overlapping weight buffers that are never resized
                // here, so the pointers stay valid and non-aliasing for the loop.
                rowp[k] = unsafe { m.w.as_mut_ptr().add(base) };
            }
            for i in 0..n {
                let xi = self.mix_in[i];
                // Same zero-skip as the dot: a 0 input yields (0*errlr)>>16 == 0 for
                // every row, so its weights don't change — skipping is output-neutral
                // and drops the NL1 updates for each unused match-model / run-map input.
                if xi != 0 {
                    for k in 0..NL1 {
                        unsafe {
                            let p = rowp[k].add(i);
                            *p = (*p).wrapping_add((xi * errlr[k]) >> 16);
                        }
                    }
                }
            }
        }
        // Fused layer-2 weight update (same idea as layer-1): the 11 combiners all
        // train on the same specialist-logit vector, so load each l2_in[i] once and
        // apply it to all 11 weight rows in one pass. Identical per-row arithmetic.
        {
            let mut errlr = [0i32; 16];
            let mut rowp: [*mut i32; 16] = [core::ptr::null_mut::<i32>(); 16];
            {
                let ms: [&mut Mixer; 16] = [
                    &mut self.l2,
                    &mut self.l2b,
                    &mut self.l2c,
                    &mut self.l2d,
                    &mut self.l2e,
                    &mut self.l2f,
                    &mut self.l2g,
                    &mut self.l2h,
                    &mut self.l2i,
                    &mut self.l2j,
                    &mut self.l2k,
                    &mut self.l2l,
                    &mut self.l2m,
                    &mut self.l2n,
                    &mut self.l2o,
                    &mut self.l2p,
                ];
                for (j, m) in ms.into_iter().enumerate() {
                    errlr[j] = ((bit << 12) - m.pr) * m.lr;
                    let base = m.ctx * NL1;
                    // SAFETY: base + NL1 <= w.len(); the 11 rows are in distinct,
                    // non-overlapping, never-resized weight buffers.
                    rowp[j] = unsafe { m.w.as_mut_ptr().add(base) };
                }
            }
            for i in 0..NL1 {
                let xi = self.l2_in[i];
                for j in 0..16 {
                    unsafe {
                        let p = rowp[j].add(i);
                        *p = (*p).wrapping_add((xi * errlr[j]) >> 16);
                    }
                }
            }
        }
        for i in 0..NCTX {
            let ix = self.idx[i];
            let s = self.sm_idx[i];
            // Borrow the counter slot once and reuse it for every field, so the
            // table is indexed a single time instead of five (no behaviour change).
            // i < NCTX and ix is the in-range slot index chosen in `predict`.
            let slot = unsafe { self.tab.get_unchecked_mut(i).get_unchecked_mut(ix) };
            let n = slot.cn as i32;
            let pr = slot.cp as i32 + 32768;
            slot.cp = ((pr + (((t - pr) * self.rate_tab[n as usize]) >> 12)) - 32768) as i16;
            if n < CNT_LIMIT {
                slot.cn = (n + 1) as u8;
            }
            // StateMap: adapt prob for the observed bit-history state, then
            // advance that state. prob is 22-bit fixed point in the high bits,
            // an adaptation count (capped at 255) in the low 10 bits.
            let smcell = &mut unsafe { self.sm.get_unchecked_mut(i) }[s & 2047];
            let entry = *smcell;
            let cnt = (entry & 1023) as i32;
            let p22 = (entry >> 10) as i32;
            let newp = p22 + (((bit << 22) - p22) / (cnt + 2));
            let newcnt = if cnt < 255 { cnt + 1 } else { 255 };
            *smcell = ((newp as u32) << 10) | (newcnt as u32);
            slot.st = next_state(s as u8, bit);
        }
        self.c0 = (self.c0 << 1) | bit;
        self.bitcount += 1;
        if self.bitcount == 8 {
            let byte = (self.c0 & 0xff) as u8;
            if self.matchlen > 0 {
                if (self.predicted_byte & 0xff) as u8 == byte {
                    self.matchptr += 1;
                    if self.matchlen < 0x3ff {
                        self.matchlen += 1;
                    }
                } else {
                    self.matchlen = 0;
                }
            }
            if self.matchlen2 > 0 {
                if (self.predicted_byte2 & 0xff) as u8 == byte {
                    self.matchptr2 += 1;
                    if self.matchlen2 < 0x3ff {
                        self.matchlen2 += 1;
                    }
                } else {
                    self.matchlen2 = 0;
                }
            }
            if self.matchlen3 > 0 {
                if (self.predicted_byte3 & 0xff) as u8 == byte {
                    self.matchptr3 += 1;
                    if self.matchlen3 < 0x3ff {
                        self.matchlen3 += 1;
                    }
                } else {
                    self.matchlen3 = 0;
                }
            }
            if self.matchlen4 > 0 {
                if (self.predicted_byte4 & 0xff) as u8 == byte {
                    self.matchptr4 += 1;
                    if self.matchlen4 < 0x3ff {
                        self.matchlen4 += 1;
                    }
                } else {
                    self.matchlen4 = 0;
                }
            }
            if self.matchlen5 > 0 {
                if (self.predicted_byte5 & 0xff) as u8 == byte {
                    self.matchptr5 += 1;
                    if self.matchlen5 < 0x3ff {
                        self.matchlen5 += 1;
                    }
                } else {
                    self.matchlen5 = 0;
                }
            }
            if self.matchlen6 > 0 {
                if (self.predicted_byte6 & 0xff) as u8 == byte {
                    self.matchptr6 += 1;
                    if self.matchlen6 < 0x3ff {
                        self.matchlen6 += 1;
                    }
                } else {
                    self.matchlen6 = 0;
                }
            }
            // Indirect model: record that `byte` followed the order-1 / order-2
            // context that preceded it (c4 still holds the pre-`byte` history).
            let ic1 = (self.c4 & 0xff) as usize;
            self.follow1[ic1] = (self.follow1[ic1] << 8) | byte as u32;
            let ic2 = (self.c4 & 0xffff) as usize;
            self.follow2[ic2] = (self.follow2[ic2] << 8) | byte as u32;
            let ic3 = ((self.c4 & 0x00ff_ffff).wrapping_mul(0x9e37_79b1) >> (32 - FBITS)) as usize;
            self.follow3[ic3] = (self.follow3[ic3] << 8) | byte as u32;
            let ic4 = (self.c4.wrapping_mul(0x85eb_ca6b) >> (32 - FBITS)) as usize;
            self.follow4[ic4] = (self.follow4[ic4] << 8) | byte as u32;
            if self.pos >= 5 {
                let m5 = self.c4.wrapping_mul(0x9e37_79b1)
                    ^ (self.b(self.pos - 5) as u32).wrapping_mul(0x85eb_ca6b);
                let k5 = (m5 >> (32 - FBITS)) as usize;
                self.follow5[k5] = (self.follow5[k5] << 8) | byte as u32;
            }
            if self.pos >= 6 {
                let m6 = self.c4.wrapping_mul(0x85eb_ca6b)
                    ^ (self.b(self.pos - 5) as u32).wrapping_mul(0xc2b2_ae35)
                    ^ (self.b(self.pos - 6) as u32).wrapping_mul(0x27d4_eb2f);
                let k6 = (m6 >> (32 - FBITS)) as usize;
                self.follow6[k6] = (self.follow6[k6] << 8) | byte as u32;
            }
            {
                let hnm = (self.c4 & 0xf0f0_f0f0).wrapping_mul(0x9e37_79b1);
                let khn = (hnm >> (32 - FBITS)) as usize;
                self.followhn[khn] = (self.followhn[khn] << 8) | byte as u32;
                let dd1 = (self.c4 & 0xff).wrapping_sub((self.c4 >> 8) & 0xff) & 0xff;
                let dd2 = ((self.c4 >> 8) & 0xff).wrapping_sub((self.c4 >> 16) & 0xff) & 0xff;
                let dd3 = ((self.c4 >> 16) & 0xff).wrapping_sub((self.c4 >> 24) & 0xff) & 0xff;
                let dm = (dd1 | (dd2 << 8) | (dd3 << 16)).wrapping_mul(0x85eb_ca6b);
                let kd = (dm >> (32 - FBITS)) as usize;
                self.followd[kd] = (self.followd[kd] << 8) | byte as u32;
                let cck = cls4(self.c4) as usize;
                self.followc[cck] = (self.followc[cck] << 8) | byte as u32;
                let lnm = (self.c4 & 0x0f0f_0f0f).wrapping_mul(0xc2b2_ae35);
                let kln = (lnm >> (32 - FBITS)) as usize;
                self.followln[kln] = (self.followln[kln] << 8) | byte as u32;
                let gk = if self.pos >= 3 {
                    (self.c4 & 0xff) | ((self.b(self.pos - 3) as u32) << 8)
                } else {
                    self.c4 & 0xffff
                } as usize;
                self.followg[gk] = (self.followg[gk] << 8) | byte as u32;
                let sk = if self.pos >= 4 {
                    (self.b(self.pos - 2) as u32) | ((self.b(self.pos - 4) as u32) << 8)
                } else {
                    self.c4 & 0xffff
                } as usize;
                self.follows2[sk] = (self.follows2[sk] << 8) | byte as u32;
                let s3k = if self.pos >= 6 {
                    (self.b(self.pos - 3) as u32) | ((self.b(self.pos - 6) as u32) << 8)
                } else {
                    self.c4 & 0xffff
                } as usize;
                self.follows3[s3k] = (self.follows3[s3k] << 8) | byte as u32;
                let g2k = if self.pos >= 5 {
                    (self.c4 & 0xff) | ((self.b(self.pos - 5) as u32) << 8)
                } else {
                    self.c4 & 0xffff
                } as usize;
                self.followg2[g2k] = (self.followg2[g2k] << 8) | byte as u32;
                let hn8 = if self.pos >= 8 {
                    hnm ^ ((self.b(self.pos - 5) as u32 & 0xf0) << 4)
                        ^ ((self.b(self.pos - 6) as u32 & 0xf0) << 12)
                        ^ ((self.b(self.pos - 7) as u32 & 0xf0) << 20)
                        ^ ((self.b(self.pos - 8) as u32 & 0xf0).wrapping_mul(0x85eb_ca6b))
                } else {
                    hnm
                };
                let khn8 = (hn8 >> (32 - FBITS)) as usize;
                self.followhn8[khn8] = (self.followhn8[khn8] << 8) | byte as u32;
                let s4k = if self.pos >= 8 {
                    (self.b(self.pos - 4) as u32) | ((self.b(self.pos - 8) as u32) << 8)
                } else {
                    self.c4 & 0xffff
                } as usize;
                self.follows4[s4k] = (self.follows4[s4k] << 8) | byte as u32;
                let ln8 = if self.pos >= 8 {
                    (self.c4 & 0x0f0f_0f0f).wrapping_mul(0xc2b2_ae35)
                        ^ ((self.b(self.pos - 5) as u32 & 0x0f) << 4)
                        ^ ((self.b(self.pos - 6) as u32 & 0x0f) << 12)
                        ^ ((self.b(self.pos - 7) as u32 & 0x0f) << 20)
                        ^ ((self.b(self.pos - 8) as u32 & 0x0f).wrapping_mul(0x9e37_79b1))
                } else {
                    (self.c4 & 0x0f0f_0f0f).wrapping_mul(0xc2b2_ae35)
                };
                let kln8 = (ln8 >> (32 - FBITS)) as usize;
                self.followln8[kln8] = (self.followln8[kln8] << 8) | byte as u32;
                let g14k = if self.pos >= 4 {
                    (self.c4 & 0xff) | ((self.b(self.pos - 4) as u32) << 8)
                } else {
                    self.c4 & 0xffff
                } as usize;
                self.followg14[g14k] = (self.followg14[g14k] << 8) | byte as u32;
                let cc8 = if self.pos >= 8 {
                    cls4(self.c4)
                        | (cls4(
                            (self.b(self.pos - 5) as u32)
                                | ((self.b(self.pos - 6) as u32) << 8)
                                | ((self.b(self.pos - 7) as u32) << 16)
                                | ((self.b(self.pos - 8) as u32) << 24),
                        ) << 8)
                } else {
                    cls4(self.c4)
                } as usize;
                self.followcc8[cc8] = (self.followcc8[cc8] << 8) | byte as u32;
                let s5k = if self.pos >= 10 {
                    (self.b(self.pos - 5) as u32) | ((self.b(self.pos - 10) as u32) << 8)
                } else {
                    self.c4 & 0xffff
                } as usize;
                self.follows5[s5k] = (self.follows5[s5k] << 8) | byte as u32;
                let g16k = if self.pos >= 6 {
                    (self.c4 & 0xff) | ((self.b(self.pos - 6) as u32) << 8)
                } else {
                    self.c4 & 0xffff
                } as usize;
                self.followg16[g16k] = (self.followg16[g16k] << 8) | byte as u32;
            }
            let bp = (self.pos & self.bufmask) as usize;
            self.buf[bp] = byte;
            self.pos += 1;
            self.c4 = (self.c4 << 8) | byte as u32;
            if byte as i32 == self.c1 {
                if self.run_len < 65535 {
                    self.run_len += 1;
                }
            } else {
                self.run_len = 1;
            }
            self.c1 = byte as i32;
            // Nesting model: track the stack of open brackets (source structure).
            match byte {
                b'(' | b'[' | b'{' => {
                    if self.nest_depth < 64 {
                        self.nest_stack[self.nest_depth] = byte;
                        self.nest_depth += 1;
                    }
                }
                b')' | b']' | b'}' => {
                    if self.nest_depth > 0 {
                        self.nest_depth -= 1;
                    }
                }
                _ => {}
            }
            if byte == b'\n' || byte == b'\r' {
                self.col = 0;
            } else if self.col < 255 {
                self.col += 1;
            }
            if byte == b'\n' {
                self.prev2_line_start = self.prev_line_start;
                self.prev_line_start = self.line_start;
                self.line_start = self.pos;
            }
            // Record-length detector: majority-vote the distance between repeats
            // of each byte value, yielding the dominant period for data (binary /
            // tabular) that has no newline structure.
            let bi = byte as usize;
            let d = self.pos - self.rpos[bi];
            self.rpos[bi] = self.pos;
            if d == self.rlen {
                if self.rcount < 1024 {
                    self.rcount += 1;
                }
            } else if self.rcount > 0 {
                self.rcount -= 1;
            } else {
                self.rlen = d;
                self.rcount = 1;
            }
            // Word-indirect: record that `byte` followed the current word prefix.
            let wk = (self.wordhash.wrapping_mul(0x9e37_79b1) >> 16) as usize;
            self.followw[wk] = (self.followw[wk] << 8) | byte as u32;
            if (byte >= b'a' && byte <= b'z')
                || (byte >= b'A' && byte <= b'Z')
                || (byte >= b'0' && byte <= b'9')
            {
                self.wordhash = hashk(self.wordhash, (byte | 0x20) as u32);
            } else {
                // Word boundary: shift the just-finished word into the word history.
                if self.wordhash != 0 {
                    self.prevword3 = self.prevword2;
                    self.prevword2 = self.prevword;
                    self.prevword = self.wordhash;
                }
                self.wordhash = 0;
            }
            if self.pos >= 6 {
                let h = (self
                    .c4
                    .wrapping_mul(2654435761)
                    .wrapping_add((self.b(self.pos - 5) as u32).wrapping_mul(0x85eb_ca6b))
                    .wrapping_add((self.b(self.pos - 6) as u32).wrapping_mul(0xc2b2_ae35)))
                    >> (32 - MMBITS);
                let cand = self.mmtab[h as usize];
                self.mmtab[h as usize] = self.pos;
                if self.matchlen == 0 && cand > 0 && cand < self.pos {
                    self.matchptr = cand;
                    let mut l: i32 = 0;
                    while l < 0x3ff
                        && cand > l as u32
                        && self.pos > (l as u32 + 1)
                        && self.b(cand - 1 - l as u32) == self.b(self.pos - 1 - l as u32)
                    {
                        l += 1;
                    }
                    self.matchlen = if l > 0 { l } else { 1 };
                }
            }
            if self.pos >= 8 {
                let h2 = (self
                    .c4
                    .wrapping_mul(2654435761)
                    .wrapping_add((self.b(self.pos - 5) as u32).wrapping_mul(0x85eb_ca6b))
                    .wrapping_add((self.b(self.pos - 6) as u32).wrapping_mul(0xc2b2_ae35))
                    .wrapping_add((self.b(self.pos - 7) as u32).wrapping_mul(0x27d4_eb2f))
                    .wrapping_add((self.b(self.pos - 8) as u32).wrapping_mul(0x1656_67b1)))
                    >> (32 - MMBITS2);
                let cand = self.mmtab2[h2 as usize];
                self.mmtab2[h2 as usize] = self.pos;
                if self.matchlen2 == 0 && cand > 0 && cand < self.pos {
                    self.matchptr2 = cand;
                    let mut l: i32 = 0;
                    while l < 0x3ff
                        && cand > l as u32
                        && self.pos > (l as u32 + 1)
                        && self.b(cand - 1 - l as u32) == self.b(self.pos - 1 - l as u32)
                    {
                        l += 1;
                    }
                    // Long-match specialist: only engage on genuinely long repeats.
                    self.matchlen2 = if l >= 8 { l } else { 0 };
                }
            }
            if self.pos >= 10 {
                let h3 = (self
                    .c4
                    .wrapping_mul(2654435761)
                    .wrapping_add((self.b(self.pos - 5) as u32).wrapping_mul(0x85eb_ca6b))
                    .wrapping_add((self.b(self.pos - 6) as u32).wrapping_mul(0xc2b2_ae35))
                    .wrapping_add((self.b(self.pos - 7) as u32).wrapping_mul(0x27d4_eb2f))
                    .wrapping_add((self.b(self.pos - 8) as u32).wrapping_mul(0x1656_67b1))
                    .wrapping_add((self.b(self.pos - 9) as u32).wrapping_mul(0xff51_afd7))
                    .wrapping_add((self.b(self.pos - 10) as u32).wrapping_mul(0xc4ce_b9fe)))
                    >> (32 - MMBITS3);
                let cand = self.mmtab3[h3 as usize];
                self.mmtab3[h3 as usize] = self.pos;
                if self.matchlen3 == 0 && cand > 0 && cand < self.pos {
                    self.matchptr3 = cand;
                    let mut l: i32 = 0;
                    while l < 0x3ff
                        && cand > l as u32
                        && self.pos > (l as u32 + 1)
                        && self.b(cand - 1 - l as u32) == self.b(self.pos - 1 - l as u32)
                    {
                        l += 1;
                    }
                    self.matchlen3 = if l >= 10 { l } else { 0 };
                }
            }
            if self.pos >= 12 {
                let h4 = (self
                    .c4
                    .wrapping_mul(2654435761)
                    .wrapping_add((self.b(self.pos - 5) as u32).wrapping_mul(0x85eb_ca6b))
                    .wrapping_add((self.b(self.pos - 6) as u32).wrapping_mul(0xc2b2_ae35))
                    .wrapping_add((self.b(self.pos - 7) as u32).wrapping_mul(0x27d4_eb2f))
                    .wrapping_add((self.b(self.pos - 8) as u32).wrapping_mul(0x1656_67b1))
                    .wrapping_add((self.b(self.pos - 9) as u32).wrapping_mul(0xff51_afd7))
                    .wrapping_add((self.b(self.pos - 10) as u32).wrapping_mul(0xc4ce_b9fe))
                    .wrapping_add((self.b(self.pos - 11) as u32).wrapping_mul(0x52dc_e729))
                    .wrapping_add((self.b(self.pos - 12) as u32).wrapping_mul(0x9e37_79b9)))
                    >> (32 - MMBITS4);
                let cand = self.mmtab4[h4 as usize];
                self.mmtab4[h4 as usize] = self.pos;
                if self.matchlen4 == 0 && cand > 0 && cand < self.pos {
                    self.matchptr4 = cand;
                    let mut l: i32 = 0;
                    while l < 0x3ff
                        && cand > l as u32
                        && self.pos > (l as u32 + 1)
                        && self.b(cand - 1 - l as u32) == self.b(self.pos - 1 - l as u32)
                    {
                        l += 1;
                    }
                    self.matchlen4 = if l >= 12 { l } else { 0 };
                }
            }
            if self.pos >= 14 {
                let h5 = (self
                    .c4
                    .wrapping_mul(2654435761)
                    .wrapping_add((self.b(self.pos - 5) as u32).wrapping_mul(0x85eb_ca6b))
                    .wrapping_add((self.b(self.pos - 6) as u32).wrapping_mul(0xc2b2_ae35))
                    .wrapping_add((self.b(self.pos - 7) as u32).wrapping_mul(0x27d4_eb2f))
                    .wrapping_add((self.b(self.pos - 8) as u32).wrapping_mul(0x1656_67b1))
                    .wrapping_add((self.b(self.pos - 9) as u32).wrapping_mul(0xff51_afd7))
                    .wrapping_add((self.b(self.pos - 10) as u32).wrapping_mul(0xc4ce_b9fe))
                    .wrapping_add((self.b(self.pos - 11) as u32).wrapping_mul(0x52dc_e729))
                    .wrapping_add((self.b(self.pos - 12) as u32).wrapping_mul(0x9e37_79b9))
                    .wrapping_add((self.b(self.pos - 13) as u32).wrapping_mul(0x7f4a_7c15))
                    .wrapping_add((self.b(self.pos - 14) as u32).wrapping_mul(0x94d0_49bb)))
                    >> (32 - MMBITS5);
                let cand = self.mmtab5[h5 as usize];
                self.mmtab5[h5 as usize] = self.pos;
                if self.matchlen5 == 0 && cand > 0 && cand < self.pos {
                    self.matchptr5 = cand;
                    let mut l: i32 = 0;
                    while l < 0x3ff
                        && cand > l as u32
                        && self.pos > (l as u32 + 1)
                        && self.b(cand - 1 - l as u32) == self.b(self.pos - 1 - l as u32)
                    {
                        l += 1;
                    }
                    self.matchlen5 = if l >= 14 { l } else { 0 };
                }
            }
            // order-2 (short) match model: anchored on just the last 2 bytes,
            // catches short repeats far earlier than the order-6+ models.
            if self.pos >= 2 {
                let h6 = ((self.c4 & 0x0000_ffff).wrapping_mul(2654435761)) >> (32 - MMBITS6);
                let cand = self.mmtab6[h6 as usize];
                self.mmtab6[h6 as usize] = self.pos;
                if self.matchlen6 == 0 && cand > 0 && cand < self.pos {
                    self.matchptr6 = cand;
                    let mut l: i32 = 0;
                    while l < 0x3ff
                        && cand > l as u32
                        && self.pos > (l as u32 + 1)
                        && self.b(cand - 1 - l as u32) == self.b(self.pos - 1 - l as u32)
                    {
                        l += 1;
                    }
                    self.matchlen6 = if l >= 2 { l } else { 0 };
                }
            }
            self.predicted_byte = if self.matchlen > 0 && self.matchptr < self.pos {
                self.b(self.matchptr) as i32
            } else {
                -1
            };
            self.predicted_byte2 = if self.matchlen2 > 0 && self.matchptr2 < self.pos {
                self.b(self.matchptr2) as i32
            } else {
                -1
            };
            self.predicted_byte3 = if self.matchlen3 > 0 && self.matchptr3 < self.pos {
                self.b(self.matchptr3) as i32
            } else {
                -1
            };
            self.predicted_byte4 = if self.matchlen4 > 0 && self.matchptr4 < self.pos {
                self.b(self.matchptr4) as i32
            } else {
                -1
            };
            self.predicted_byte5 = if self.matchlen5 > 0 && self.matchptr5 < self.pos {
                self.b(self.matchptr5) as i32
            } else {
                -1
            };
            self.predicted_byte6 = if self.matchlen6 > 0 && self.matchptr6 < self.pos {
                self.b(self.matchptr6) as i32
            } else {
                -1
            };
            // Update each run map: ctxhash[] still holds the context that PRECEDED
            // this byte (byte_start recomputes them just below), so record that
            // `byte` followed it — extending the run if the byte repeats, else
            // resetting to a fresh length-1 run for the new byte.
            for j in 0..NRUN {
                let h = self.ctxhash[RUN_CTX[j]];
                let idx = (h & self.runmask) as usize;
                let chk = (h >> self.runbits) as u8;
                let slot = self.runtab[j][idx];
                let newslot = if (slot >> 16) as u8 == chk
                    && (slot & 0xff) as u8 == byte
                    && ((slot >> 8) & 0xff) != 0
                {
                    let nc = ((slot >> 8) & 0xff).min(254) + 1;
                    ((chk as u32) << 16) | (nc << 8) | byte as u32
                } else {
                    ((chk as u32) << 16) | (1u32 << 8) | byte as u32
                };
                self.runtab[j][idx] = newslot;
            }
            self.c0 = 1;
            self.bitcount = 0;
            self.byte_start();
        }
    }
}
