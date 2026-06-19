#!/usr/bin/env bash
# Unified frozen metrics: WORK and MEMCOST in one build + one process.
#
# FROZEN — not part of the editable algorithm surface.
set -euo pipefail
cd "$(dirname "$0")/.."

rustup target add wasm32-unknown-unknown >/dev/null 2>&1 || true
( cd metrics && RUSTFLAGS="" cargo build --release --quiet -p cm-wasm-meter --target wasm32-unknown-unknown )
( cd metrics && cargo build --release --quiet -p cm-all-meter )

WASM=metrics/target/wasm32-unknown-unknown/release/cm_wasm_meter.wasm
./metrics/target/release/cm-all-meter "$WASM"
