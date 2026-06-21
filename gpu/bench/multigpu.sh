#!/usr/bin/env bash
# Aggregate multi-GPU throughput: run one single-GPU process per device
# (pinned via CUDA_VISIBLE_DEVICES) on an equal shard of a large file,
# concurrently, and report per-GPU + aggregate MB/s. Reuses the verified
# single-GPU `gpu` binary — no kernel changes. Run from gpu/ on the instance.
#
#   bench/multigpu.sh [seg_size] [per_gpu_MB]
set -euo pipefail
cd "$(dirname "$0")/.."
SEG="${1:-16384}"
PER_GPU_MB="${2:-56}"
CORPUS="../corpus"
WORK=/tmp/mg
mkdir -p "$WORK"

N=$(nvidia-smi -L | wc -l)
[ "$N" -lt 1 ] && { echo "no GPUs"; exit 1; }
TOTAL_MB=$(( N * PER_GPU_MB ))
echo "GPUs=$N  seg=$SEG  per-GPU=${PER_GPU_MB}MB  total=${TOTAL_MB}MB"

# Build a large input from the corpus (repeat until big enough).
BIG="$WORK/big.bin"
if [ ! -f "$BIG" ] || [ "$(($(wc -c < "$BIG")/1000000))" -lt "$TOTAL_MB" ]; then
  : > "$BIG"
  while [ "$(($(wc -c < "$BIG")/1000000))" -lt "$TOTAL_MB" ]; do cat "$CORPUS"/*.bin >> "$BIG"; done
fi
SZ=$(wc -c < "$BIG")

# Shard into N equal pieces.
rm -f "$WORK"/shard_*
SHARD=$(( (SZ + N - 1) / N ))
split -b "$SHARD" -d -a 2 "$BIG" "$WORK/shard_"
SHARDS=$(ls "$WORK"/shard_*)

[ -x ./gpu ] || { echo "build ./gpu first"; exit 1; }

run_phase() {  # $1 = c|d ; uses global SHARDS
  local op="$1" i=0
  local t0 t1
  t0=$(date +%s.%N)
  for f in $SHARDS; do
    if [ "$op" = "c" ]; then
      CUDA_VISIBLE_DEVICES=$i ./gpu c "$f" "$f.cm" "$SEG" &
    else
      CUDA_VISIBLE_DEVICES=$i ./gpu d "$f.cm" "$f.out" &
    fi
    i=$((i+1))
  done
  wait
  t1=$(date +%s.%N)
  awk -v sz="$SZ" -v t0="$t0" -v t1="$t1" -v n="$N" -v op="$op" \
    'BEGIN{w=t1-t0; printf "  %s: wall=%.2fs  aggregate=%.1f MB/s  (per-GPU avg=%.1f MB/s)\n", (op=="c"?"compress":"decompress"), w, (sz/1e6)/w, (sz/1e6)/w/n}'
}

echo "== compress =="
run_phase c
echo "== decompress =="
run_phase d

# correctness: every shard must roundtrip, and report total compressed size
ok=1; comp=0
for f in $SHARDS; do
  cmp -s "$f" "$f.out" || { echo "  ROUNDTRIP FAIL on $f"; ok=0; }
  comp=$(( comp + $(wc -c < "$f.cm") ))
done
awk -v sz="$SZ" -v comp="$comp" -v ok="$ok" \
  'BEGIN{printf "ratio=%.4f  roundtrip=%s\n", comp/sz, (ok? "OK":"FAIL")}'
