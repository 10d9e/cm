//! Complexity meter. WORK = init-free wasm fuel, differenced (full − half prefix)
//! to cancel one-time setup. Heap bytes requested and peak linear memory are
//! reported as non-ranking diagnostics (heap is ~0.002% of WORK — see
//! docs/proposals/0001 §4.2 — so it is not folded into the ranking number).

use wasmtime::{Config, Engine, Instance, Module, Store};

const FULL: u32 = 8192;
const HALF: u32 = 4096;

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: cm-fuel-meter <module.wasm>");
    let wasm = std::fs::read(&path).expect("read wasm");

    let mut config = Config::new();
    config.consume_fuel(true);
    let engine = Engine::new(&config).expect("engine");
    let module = Module::from_binary(&engine, &wasm).expect("parse wasm");

    let mut store = Store::new(&engine, ());
    store.set_fuel(1_000_000_000_000_000).expect("set fuel");
    let instance = Instance::new(&mut store, &module, &[]).expect("instantiate");
    let compress = instance
        .get_typed_func::<u32, u32>(&mut store, "compress_prefix")
        .expect("get compress_prefix");
    let heap = instance
        .get_typed_func::<(), u64>(&mut store, "cm_heap_bytes")
        .expect("get cm_heap_bytes");
    let mem_pages = instance
        .get_typed_func::<(), u32>(&mut store, "cm_mem_pages")
        .expect("get cm_mem_pages");

    // full prefix
    let f0 = store.get_fuel().unwrap();
    let h0 = heap.call(&mut store, ()).unwrap();
    let out_full = compress.call(&mut store, FULL).expect("call full");
    let f1 = store.get_fuel().unwrap();
    let h1 = heap.call(&mut store, ()).unwrap();
    let fuel_full = f0 - f1;
    let heap_full = h1 - h0;
    let peak_pages = mem_pages.call(&mut store, ()).unwrap();

    // half prefix (same table-size regime, so setup cost cancels)
    let out_half = compress.call(&mut store, HALF).expect("call half");
    let f2 = store.get_fuel().unwrap();
    let h2 = heap.call(&mut store, ()).unwrap();
    let fuel_half = f1 - f2;
    let heap_half = h2 - h1;

    let fuel_work = fuel_full
        .checked_sub(fuel_half)
        .expect("fuel cancellation invariant violated (full < half)");
    let heap_work = heap_full.saturating_sub(heap_half);

    println!("full {}B -> {}B (fuel {}, heap {} B)", FULL, out_full, fuel_full, heap_full);
    println!("half {}B -> {}B (fuel {}, heap {} B)", HALF, out_half, fuel_half, heap_half);
    println!(
        "peak linear memory: {} pages ({} bytes; heap + stack + statics; diagnostic)",
        peak_pages,
        peak_pages as u64 * 65536
    );
    println!(
        "heap: {} B requested (differenced; non-ranking diagnostic, ~0.002% of WORK)",
        heap_work
    );
    println!(
        "WORK: {} (init-free wasm fuel, for {} bytes; lower is faster/leaner)",
        fuel_work,
        FULL - HALF
    );
}
