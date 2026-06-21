#!/usr/bin/env bash
# rsync the gpu/ subproject + corpus to a remote host (used by provision.sh).
#   sync.sh <ssh_host> <ssh_port>
set -euo pipefail
HOST="$1"; PORT="$2"
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"   # repo root
SSH="ssh -p $PORT -o StrictHostKeyChecking=accept-new"

$SSH "root@$HOST" "mkdir -p /root/cm/gpu /root/cm/corpus"
rsync -az -e "$SSH" --delete \
  --exclude 'cpu_ref' --exclude 'gpu' --exclude 'bench/big.bin' --exclude 'bench/*.csv' \
  "$ROOT/gpu/" "root@$HOST:/root/cm/gpu/"
rsync -az -e "$SSH" "$ROOT/corpus/" "root@$HOST:/root/cm/corpus/"
echo "synced to root@$HOST:/root/cm"
