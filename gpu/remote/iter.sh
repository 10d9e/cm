#!/usr/bin/env bash
# Sync local gpu/ + corpus to the running keep-alive instance, rebuild, and run a
# command. Fast inner loop for debugging — no re-provisioning.
#
#   remote/iter.sh                       # sync + build + GPU selftest on all corpus
#   remote/iter.sh 'cd /root/cm/gpu && ./gpu bench ../corpus/text_dickens.bin 4096,65536'
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
ENVF="/tmp/cmgpu/instance.env"
[ -f "$ENVF" ] || { echo "no $ENVF — run remote/up.sh first"; exit 1; }
. "$ENVF"
SSH="ssh -p $PORT -o StrictHostKeyChecking=accept-new"

# sync code + corpus
$SSH "root@$HOST" "mkdir -p /root/cm/gpu /root/cm/corpus"
rsync -az -e "$SSH" --delete \
  --exclude 'cpu_ref' --exclude 'gpu' --exclude 'bench/big.bin' --exclude 'bench/*.csv' \
  "$ROOT/gpu/" "root@$HOST:/root/cm/gpu/"
rsync -az -e "$SSH" "$ROOT/corpus/" "root@$HOST:/root/cm/corpus/"

CMD="${1:-cd /root/cm/gpu && make gpu CUDA_ARCH=${CUDA_ARCH:-sm_89} && for f in ../corpus/*.bin; do ./gpu selftest \$f 65536; done}"
echo "== remote: $CMD =="
$SSH "root@$HOST" "set -e; $CMD"
