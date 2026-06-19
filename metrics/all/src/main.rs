//! Unified frozen metrics runner: WORK, MEMCOST, LINES, and HEAP_CHURN in one
//! process (single wasm instrumentation pass for MEMCOST+LINES). HEAP_PEAK is
//! measured separately via `cm-heappeak-meter` because it requires a
//! `#[global_allocator]` hook that must not instrument the wasm host.

use anyhow::{Context, Result};
use cm_metrics_common::{measure_heap_churn, measure_work, MemLinesMeter, FULL, HALF};
use std::fs;
use std::process::Command;

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let std_wasm = args
        .next()
        .context("usage: cm-all-meter <meter.wasm> <heap-meter.wasm> [corpus_dir]")?;
    let heap_wasm = args
        .next()
        .context("usage: cm-all-meter <meter.wasm> <heap-meter.wasm> [corpus_dir]")?;
    let corpus = args.next().unwrap_or_else(|| "corpus".to_string());

    let std_bytes = fs::read(&std_wasm).with_context(|| format!("read {std_wasm}"))?;
    let heap_bytes = fs::read(&heap_wasm).with_context(|| format!("read {heap_wasm}"))?;

    let fuel = measure_work(&std_bytes);
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

    let memlines = MemLinesMeter::new(&std_bytes);
    let full = memlines.measure(FULL);
    let half = memlines.measure(HALF);
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

    let lines_full = full.distinct_lines();
    let lines_half = half.distinct_lines();
    assert_eq!(
        lines_full,
        memlines.measure(FULL).distinct_lines(),
        "non-deterministic line count (full)"
    );
    assert_eq!(
        lines_half,
        memlines.measure(HALF).distinct_lines(),
        "non-deterministic line count (half)"
    );
    let lines = lines_full.saturating_sub(lines_half);
    println!("full {}B: {} distinct 64B lines", FULL, lines_full);
    println!("half {}B: {} distinct 64B lines", HALF, lines_half);
    println!(
        "LINES: {} (deterministic, init-free distinct 64B lines touched for {} bytes; counts heap+static+stack; lower is friendlier to memory)",
        lines,
        FULL - HALF
    );

    let heap = measure_heap_churn(&heap_bytes);
    println!("full {}B: heap {} B", FULL, heap.full);
    println!("half {}B: heap {} B", HALF, heap.half);
    println!(
        "HEAP_CHURN: {} (deterministic, init-free heap bytes requested for {} bytes; ~steady-state allocation; lower is leaner)",
        heap.churn,
        FULL - HALF
    );

    let peak = measure_heappeak(&corpus)?;
    println!(
        "HEAP_PEAK: {} (peak live reserved heap bytes over the full corpus; deterministic, heap-only diagnostic; lower is leaner)",
        peak
    );

    Ok(())
}

fn measure_heappeak(corpus: &str) -> Result<u64> {
    let exe = std::env::current_exe().context("current_exe")?;
    let heappeak = exe
        .parent()
        .map(|d| d.join("cm-heappeak-meter"))
        .context("heappeak binary dir")?;
    let out = Command::new(&heappeak)
        .arg(corpus)
        .output()
        .with_context(|| format!("run {}", heappeak.display()))?;
    if !out.status.success() {
        anyhow::bail!(
            "cm-heappeak-meter failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let text = String::from_utf8_lossy(&out.stdout);
    text.lines()
        .find_map(|l| l.strip_prefix("HEAP_PEAK: ").and_then(|v| v.split_whitespace().next()))
        .and_then(|v| v.parse().ok())
        .context("parse HEAP_PEAK from heappeak output")
}
