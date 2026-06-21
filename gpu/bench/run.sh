#!/usr/bin/env bash
# Benchmark the block-parallel codec: ratio across segment sizes on the real
# corpus (honest ratio), plus throughput on a larger synthetic input (loads the
# GPU enough to show peak MB/s). Run from the gpu/ directory.
#
#   bench/run.sh [corpus_dir] [out_csv]
# Produces two CSVs: <out>.cpu.csv and (if ./gpu exists) <out>.gpu.csv
set -euo pipefail
cd "$(dirname "$0")/.."

CORPUS="${1:-../corpus}"
OUT="${2:-bench/results}"
SEGS="4096,16384,65536,262144,393216"

[ -x ./cpu_ref ] || make cpu_ref

echo "file,seg_size,orig,comp,ratio" > "$OUT.cpu.csv"
for f in "$CORPUS"/*.bin; do
  ./cpu_ref bench "$f" "$SEGS" >> "$OUT.cpu.csv"
done
echo "wrote $OUT.cpu.csv"

# Larger synthetic input: concatenate the corpus ~24x (~57 MB) so the GPU has
# enough segments to saturate. Built from real data so ratios stay meaningful.
BIG=bench/big.bin
if [ ! -f "$BIG" ]; then
  : > "$BIG"
  for i in $(seq 1 24); do cat "$CORPUS"/*.bin >> "$BIG"; done
fi

if [ -x ./gpu ]; then
  echo "file,seg_size,orig,comp,ratio,c_MBps,d_MBps,ok" > "$OUT.gpu.csv"
  for f in "$CORPUS"/*.bin; do
    ./gpu bench "$f" "$SEGS" >> "$OUT.gpu.csv"
  done
  # throughput run on the big file
  ./gpu bench "$BIG" "$SEGS" >> "$OUT.gpu.csv"
  echo "wrote $OUT.gpu.csv"
else
  echo "no ./gpu binary — skipping GPU benchmark (build on the 4090 first)"
fi
