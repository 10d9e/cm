#!/usr/bin/env bash
# Evaluate one candidate: boundary guard -> correctness gate -> score.
# FROZEN — do not edit as part of autoresearch.
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$PATH:/usr/bin"

echo "== boundary guard =="
bash scripts/guard.sh

echo "== correctness gate (round-trip tests) =="
if ! cargo test --release >/tmp/cm_test.log 2>&1; then
  echo "TESTS FAILED — candidate is INVALID:"
  tail -n 30 /tmp/cm_test.log
  exit 1
fi
grep -E "test result" /tmp/cm_test.log

echo "== build =="
cargo build --release --quiet

echo "== score =="
./target/release/cm eval corpus
