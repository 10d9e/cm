//! Shared constants, wasm instrumentation, and cache tracking for the frozen
//! metrics harness. Lives outside `src/algorithm/`.

use walrus::ir::*;
use walrus::{FunctionId, FunctionKind, LocalFunction, LocalId, ValType};
use wasmtime::{Config, Engine, Instance, Linker, Module, Store};

pub const FULL: u32 = 8192;
pub const HALF: u32 = 4096;
pub const EXPORT_PREFIX: &str = "compress_prefix";
pub const EXPORT_HE: &str = "compress_prefix_he";

const LINE: usize = 64;
const L1_BYTES: usize = 32 * 1024;
const L1_WAYS: usize = 8;
const L2_BYTES: usize = 1024 * 1024;
const L2_WAYS: usize = 8;
const L3_BYTES: usize = 32 * 1024 * 1024;
const L3_WAYS: usize = 16;
const PEN_L1: u64 = 14;
const PEN_L2: u64 = 40;
const PEN_L3: u64 = 200;

struct Cache {
    set_mask: u64,
    ways: usize,
    tags: Vec<u64>,
    age: Vec<u64>,
}

impl Cache {
    fn new(size: usize, ways: usize) -> Self {
        let sets = size / (ways * LINE);
        assert!(sets.is_power_of_two(), "cache sets must be a power of two");
        Cache {
            set_mask: sets as u64 - 1,
            ways,
            tags: vec![u64::MAX; sets * ways],
            age: vec![0; sets * ways],
        }
    }

    #[inline]
    fn hit(&mut self, line: u64, clock: u64) -> bool {
        let base = ((line & self.set_mask) as usize) * self.ways;
        let ways = &mut self.tags[base..base + self.ways];
        let ages = &mut self.age[base..base + self.ways];
        for w in 0..self.ways {
            if ways[w] == line {
                ages[w] = clock;
                return true;
            }
        }
        let mut victim = 0;
        for w in 1..self.ways {
            if ages[w] < ages[victim] {
                victim = w;
            }
        }
        ways[victim] = line;
        ages[victim] = clock;
        false
    }
}

/// MEMCOST cache model state.
pub struct AccessTracker {
    l1: Cache,
    l2: Cache,
    l3: Cache,
    clock: u64,
    pub accesses: u64,
    pub l1m: u64,
    pub l2m: u64,
    pub l3m: u64,
}

impl AccessTracker {
    pub fn new() -> Self {
        AccessTracker {
            l1: Cache::new(L1_BYTES, L1_WAYS),
            l2: Cache::new(L2_BYTES, L2_WAYS),
            l3: Cache::new(L3_BYTES, L3_WAYS),
            clock: 0,
            accesses: 0,
            l1m: 0,
            l2m: 0,
            l3m: 0,
        }
    }

    #[inline]
    fn touch_cache(&mut self, line: u64) {
        self.clock += 1;
        let c = self.clock;
        if self.l1.hit(line, c) {
            return;
        }
        self.l1m += 1;
        if self.l2.hit(line, c) {
            return;
        }
        self.l2m += 1;
        if self.l3.hit(line, c) {
            return;
        }
        self.l3m += 1;
    }

    #[inline]
    pub fn access(&mut self, addr: u32, size: u32) {
        self.accesses += 1;
        let first = (addr as u64) / LINE as u64;
        let last = ((addr as u64) + size.max(1) as u64 - 1) / LINE as u64;
        let mut l = first;
        loop {
            self.touch_cache(l);
            if l == last {
                break;
            }
            l += 1;
        }
    }

    pub fn memcost(&self) -> u64 {
        self.l1m * PEN_L1 + self.l2m * PEN_L2 + self.l3m * PEN_L3
    }
}

fn load_size(k: &LoadKind) -> u32 {
    match k {
        LoadKind::I32 { .. } => 4,
        LoadKind::I64 { .. } => 8,
        LoadKind::F32 => 4,
        LoadKind::F64 => 8,
        LoadKind::V128 => 16,
        LoadKind::I32_8 { .. } | LoadKind::I64_8 { .. } => 1,
        LoadKind::I32_16 { .. } | LoadKind::I64_16 { .. } => 2,
        LoadKind::I64_32 { .. } => 4,
    }
}

fn store_info(k: &StoreKind) -> (u32, ValType) {
    match k {
        StoreKind::I32 { .. } => (4, ValType::I32),
        StoreKind::I64 { .. } => (8, ValType::I64),
        StoreKind::F32 => (4, ValType::F32),
        StoreKind::F64 => (8, ValType::F64),
        StoreKind::V128 => (16, ValType::V128),
        StoreKind::I32_8 { .. } => (1, ValType::I32),
        StoreKind::I32_16 { .. } => (2, ValType::I32),
        StoreKind::I64_8 { .. } => (1, ValType::I64),
        StoreKind::I64_16 { .. } => (2, ValType::I64),
        StoreKind::I64_32 { .. } => (4, ValType::I64),
    }
}

fn collect_seqs(f: &LocalFunction, seq: InstrSeqId, out: &mut Vec<InstrSeqId>) {
    out.push(seq);
    for (instr, _) in f.block(seq).instrs.iter() {
        match instr {
            Instr::Block(b) => collect_seqs(f, b.seq, out),
            Instr::Loop(l) => collect_seqs(f, l.seq, out),
            Instr::IfElse(ie) => {
                collect_seqs(f, ie.consequent, out);
                collect_seqs(f, ie.alternative, out);
            }
            _ => {}
        }
    }
}

struct Tmp {
    track: FunctionId,
    addr: LocalId,
    vi32: LocalId,
    vi64: LocalId,
    vf32: LocalId,
    vf64: LocalId,
}

impl Tmp {
    fn val_local(&self, t: ValType) -> LocalId {
        match t {
            ValType::I64 => self.vi64,
            ValType::F32 => self.vf32,
            ValType::F64 => self.vf64,
            _ => self.vi32,
        }
    }
}

fn loc() -> InstrLocId {
    InstrLocId::default()
}

fn ci32(v: i32) -> Instr {
    Instr::Const(Const {
        value: Value::I32(v),
    })
}

fn instrument(module: &mut walrus::Module) {
    let ty = module
        .types
        .add(&[ValType::I32, ValType::I32, ValType::I32], &[]);
    let (track, _) = module.add_import_func("mem", "track", ty);
    let t = Tmp {
        track,
        addr: module.locals.add(ValType::I32),
        vi32: module.locals.add(ValType::I32),
        vi64: module.locals.add(ValType::I64),
        vf32: module.locals.add(ValType::F32),
        vf64: module.locals.add(ValType::F64),
    };

    let local_ids: Vec<FunctionId> = module
        .funcs
        .iter()
        .filter(|f| matches!(f.kind, FunctionKind::Local(_)))
        .map(|f| f.id())
        .collect();

    for fid in local_ids {
        let f = match &mut module.funcs.get_mut(fid).kind {
            FunctionKind::Local(lf) => lf,
            _ => continue,
        };
        let mut seqs = Vec::new();
        collect_seqs(f, f.entry_block(), &mut seqs);
        for sid in seqs {
            let old = std::mem::take(&mut f.block_mut(sid).instrs);
            let mut new: Vec<(Instr, InstrLocId)> = Vec::with_capacity(old.len() + 8);
            for (instr, il) in old {
                match &instr {
                    Instr::Load(l) => {
                        let sz = load_size(&l.kind) as i32;
                        let off = l.arg.offset as i32;
                        new.push((Instr::LocalTee(LocalTee { local: t.addr }), loc()));
                        new.push((Instr::LocalGet(LocalGet { local: t.addr }), loc()));
                        new.push((ci32(off), loc()));
                        new.push((Instr::Binop(Binop {
                            op: BinaryOp::I32Add,
                        }), loc()));
                        new.push((ci32(sz), loc()));
                        new.push((ci32(0), loc()));
                        new.push((Instr::Call(Call { func: t.track }), loc()));
                        new.push((instr, il));
                    }
                    Instr::Store(s) => {
                        let (sz, vt) = store_info(&s.kind);
                        let off = s.arg.offset as i32;
                        let vl = t.val_local(vt);
                        new.push((Instr::LocalSet(LocalSet { local: vl }), loc()));
                        new.push((Instr::LocalTee(LocalTee { local: t.addr }), loc()));
                        new.push((Instr::LocalGet(LocalGet { local: t.addr }), loc()));
                        new.push((ci32(off), loc()));
                        new.push((Instr::Binop(Binop {
                            op: BinaryOp::I32Add,
                        }), loc()));
                        new.push((ci32(sz as i32), loc()));
                        new.push((ci32(1), loc()));
                        new.push((Instr::Call(Call { func: t.track }), loc()));
                        new.push((Instr::LocalGet(LocalGet { local: vl }), loc()));
                        new.push((instr, il));
                    }
                    _ => new.push((instr, il)),
                }
            }
            f.block_mut(sid).instrs = new;
        }
    }
}

pub fn instrument_wasm(wasm: &[u8]) -> Vec<u8> {
    let mut module = walrus::Module::from_buffer(wasm).expect("parse wasm");
    instrument(&mut module);
    module.emit_wasm()
}

pub struct FuelRun {
    pub out_full: u32,
    pub out_half: u32,
    pub fuel_full: u64,
    pub fuel_half: u64,
    pub work: u64,
}

pub fn measure_work(wasm: &[u8]) -> FuelRun {
    let mut config = Config::new();
    config.consume_fuel(true);
    let engine = Engine::new(&config).expect("engine");
    let module = Module::from_binary(&engine, wasm).expect("parse wasm");

    let mut store = Store::new(&engine, ());
    store
        .set_fuel(1_000_000_000_000_000)
        .expect("set fuel");
    let instance = Instance::new(&mut store, &module, &[]).expect("instantiate");
    let f = instance
        .get_typed_func::<u32, u32>(&mut store, EXPORT_PREFIX)
        .expect("get compress_prefix");

    let b1 = store.get_fuel().unwrap();
    let out_full = f.call(&mut store, FULL).expect("call full");
    let a1 = store.get_fuel().unwrap();
    let fuel_full = b1 - a1;

    let out_half = f.call(&mut store, HALF).expect("call half");
    let a2 = store.get_fuel().unwrap();
    let fuel_half = a1 - a2;

    FuelRun {
        out_full,
        out_half,
        fuel_full,
        fuel_half,
        work: fuel_full - fuel_half,
    }
}

pub struct MemMeter {
    engine: Engine,
    module: Module,
    linker: Linker<AccessTracker>,
}

impl MemMeter {
    pub fn new(wasm: &[u8]) -> Self {
        let instrumented = instrument_wasm(wasm);
        let engine = Engine::new(&Config::new()).expect("engine");
        let module = Module::from_binary(&engine, &instrumented).expect("parse instrumented");
        let mut linker = Linker::new(&engine);
        linker
            .func_wrap(
                "mem",
                "track",
                |mut c: wasmtime::Caller<AccessTracker>, addr: i32, size: i32, _rw: i32| {
                    c.data_mut().access(addr as u32, size as u32);
                },
            )
            .expect("link track");
        MemMeter {
            engine,
            module,
            linker,
        }
    }

    pub fn measure(&self, prefix: u32) -> AccessTracker {
        let mut store = Store::new(&self.engine, AccessTracker::new());
        let inst = self
            .linker
            .instantiate(&mut store, &self.module)
            .expect("instantiate");
        let f = inst
            .get_typed_func::<u32, u32>(&mut store, EXPORT_HE)
            .expect(EXPORT_HE);
        f.call(&mut store, prefix).expect("call");
        store.into_data()
    }
}
