//! Unified frozen metrics runner: WORK and MEMCOST in one process.

use anyhow::{Context, Result};
use cm_metrics_common::{measure_work, MemMeter, FULL, HALF};
use std::fs;

fn main() -> Result<()> {
    let path = std::env::args()
        .nth(1)
        .context("usage: cm-all-meter <module.wasm>")?;
    let wasm = fs::read(&path).with_context(|| format!("read {path}"))?;

    let fuel = measure_work(&wasm);
    println!(
        "full {}B -> {}B (fuel {})",
        FULL, fuel.out_full, fuel.fuel_full
    );
    println!(
        "half {}B -> {}B (fuel {})",
        HALF, fuel.out_half, fuel.fuel_half
    );
    println!(
        "WORK: {} (deterministic, init-free wasm operators for {} bytes; lower is faster)",
        fuel.work,
        FULL - HALF
    );

    let mem = MemMeter::new(&wasm);
    let full = mem.measure(FULL);
    let half = mem.measure(HALF);
    let memcost = full.memcost() as i64 - half.memcost() as i64;
    println!(
        "full {}B: accesses {}, miss L1 {} L2 {} L3/DRAM {}, memcost {}",
        FULL, full.accesses, full.l1m, full.l2m, full.l3m, full.memcost()
    );
    println!(
        "half {}B: accesses {}, miss L1 {} L2 {} L3/DRAM {}, memcost {}",
        HALF, half.accesses, half.l1m, half.l2m, half.l3m, half.memcost()
    );
    println!(
        "MEMCOST: {} (deterministic, init-free weighted cache-miss penalty for {} bytes; lower is friendlier to memory)",
        memcost,
        FULL - HALF
    );

    Ok(())
}
