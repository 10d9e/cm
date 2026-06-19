#!/usr/bin/env bash
# Deterministic, tamper-proof complexity metric (lower = less compute).
#
# Builds the wasm shim and the wasmtime host meter — both OUTSIDE src/algorithm/,
# so a submission cannot alter the measurement — and prints WORK: the init-free
# executed-operator (fuel) count for compressing a fixed corpus prefix. The wasm
# is built for the fixed wasm32 target (no host-specific codegen), so the number
# is reproducible across machines given a pinned toolchain + wasmtime version.
#
# FROZEN — not part of the editable algorithm surface.
set -euo pipefail
cd "$(dirname "$0")/.."

rustup target add wasm32-unknown-unknown >/dev/null 2>&1 || true
( cd metrics && RUSTFLAGS="" cargo build --release --quiet -p cm-wasm-meter --target wasm32-unknown-unknown )
( cd metrics && cargo build --release --quiet -p cm-fuel-meter )

WASM=metrics/target/wasm32-unknown-unknown/release/cm_wasm_meter.wasm
./metrics/target/release/cm-fuel-meter "$WASM"
