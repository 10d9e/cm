#!/usr/bin/env bash
# Unified frozen metrics: WORK, MEMCOST, LINES, HEAP_PEAK, HEAP_CHURN in one
# build + one wasm-instrumentation pass (MEMCOST+LINES share instrumentation).
#
# Builds the wasm shim twice (standard + heap feature), compiles all meter
# binaries once via the metrics workspace, then runs cm-all-meter.
#
# FROZEN — not part of the editable algorithm surface.
set -euo pipefail
cd "$(dirname "$0")/.."

rustup target add wasm32-unknown-unknown >/dev/null 2>&1 || true

WASM_DIR=metrics/target/wasm32-unknown-unknown/release
STD_WASM=/tmp/cm_meter_std.wasm

( cd metrics && RUSTFLAGS="" cargo build --release --quiet -p cm-wasm-meter --target wasm32-unknown-unknown )
cp "$WASM_DIR/cm_wasm_meter.wasm" "$STD_WASM"

( cd metrics && RUSTFLAGS="" cargo build --release --quiet -p cm-wasm-meter --target wasm32-unknown-unknown --features heap )

( cd metrics && cargo build --release --quiet -p cm-all-meter -p cm-heappeak-meter )

./metrics/target/release/cm-all-meter \
  "$STD_WASM" \
  "$WASM_DIR/cm_wasm_meter.wasm" \
  "${1:-corpus}"
