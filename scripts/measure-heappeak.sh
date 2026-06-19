#!/usr/bin/env bash
# Deterministic full-scale reserved-heap metric (HEAP_PEAK), non-scoring
# diagnostic — peak live reserved heap over the full corpus. FROZEN.
set -euo pipefail
cd "$(dirname "$0")/.."
( cd metrics && cargo build --release --quiet -p cm-heappeak-meter )
./metrics/target/release/cm-heappeak-meter corpus
