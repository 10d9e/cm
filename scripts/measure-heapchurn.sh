#!/usr/bin/env bash
# Deterministic init-free heap-churn metric (HEAP_CHURN). Builds the shim WITH
# the heap-tracking feature (a SEPARATE build, so the WORK/MEMCOST shim bytes
# are unaffected) and differences requested heap bytes. Non-scoring. FROZEN.
set -euo pipefail
cd "$(dirname "$0")/.."
rustup target add wasm32-unknown-unknown >/dev/null 2>&1 || true
( cd metrics && RUSTFLAGS="" cargo build --release --quiet -p cm-wasm-meter --target wasm32-unknown-unknown --features heap )
( cd metrics && cargo build --release --quiet -p cm-heapchurn-meter )
WASM=metrics/target/wasm32-unknown-unknown/release/cm_wasm_meter.wasm
./metrics/target/release/cm-heapchurn-meter "$WASM"
