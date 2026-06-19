#!/usr/bin/env bash
# Deterministic, tamper-proof memory-traffic metric (lower = friendlier to cache).
#
# Companion to measure-complexity.sh (WORK). Where WORK counts executed wasm
# operators — blind to cache behaviour — this instruments the wasm shim's
# loads/stores and runs the deterministic access trace through a fixed cache
# model, printing MEMCOST: the init-free weighted cache-miss penalty. Both the
# shim and the meter live OUTSIDE src/algorithm/, so a submission cannot alter
# the measurement; the wasm is built for the fixed wasm32 target, so the number
# is reproducible across machines given a pinned toolchain + walrus/wasmtime.
#
# FROZEN — not part of the editable algorithm surface.
set -euo pipefail
cd "$(dirname "$0")/.."

rustup target add wasm32-unknown-unknown >/dev/null 2>&1 || true
( cd metrics && RUSTFLAGS="" cargo build --release --quiet -p cm-wasm-meter --target wasm32-unknown-unknown )
( cd metrics && cargo build --release --quiet -p cm-mem-meter )

WASM=metrics/target/wasm32-unknown-unknown/release/cm_wasm_meter.wasm
./metrics/target/release/cm-mem-meter "$WASM"
