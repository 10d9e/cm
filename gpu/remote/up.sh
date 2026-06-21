#!/usr/bin/env bash
# Provision a vast.ai 1x RTX 4090 and KEEP IT RUNNING (no auto-destroy). Saves
# connection info to /tmp/cmgpu/instance.env for iter.sh / down.sh. Tear down
# only with down.sh.
#
#   MAX_DPH=0.50 remote/up.sh
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
MAX_DPH="${MAX_DPH:-0.50}"
GPU_NAME="${GPU_NAME:-RTX_4090}"   # e.g. RTX_4090, RTX_5090
NUM_GPUS="${NUM_GPUS:-1}"
MIN_CUDA="${MIN_CUDA:-12.2}"       # host driver CUDA must be >= this (5090 needs 12.8)
IMAGE="${IMAGE:-nvidia/cuda:12.2.2-devel-ubuntu22.04}"
DISK="${DISK:-24}"
ENVF="/tmp/cmgpu/instance.env"
mkdir -p /tmp/cmgpu

PATH="$(python3 -m site --user-base 2>/dev/null)/bin:$PATH"
VAST=""; for c in vastai vast; do command -v "$c" >/dev/null 2>&1 && "$c" --version >/dev/null 2>&1 && VAST="$c" && break; done
[ -z "$VAST" ] && { echo "vast CLI not found"; exit 1; }

KEY=""
for p in "$HOME/.config/vastai/vast_api_key" "$ROOT/.notes"; do
  [ -f "$p" ] && KEY="$(tr -d ' \t\n\r' < "$p")" && [ -n "$KEY" ] && break
done
[ -z "$KEY" ] && KEY="${VAST_API_KEY:-}"
$VAST set api-key "$KEY" >/dev/null

QUERY="gpu_name=$GPU_NAME num_gpus=$NUM_GPUS rentable=true cuda_vers>=$MIN_CUDA dph<$MAX_DPH"
echo "searching: $QUERY"
OFFER_JSON="$($VAST search offers "$QUERY" -o 'dph' --raw)"
ASK_ID="$(OFFER_JSON="$OFFER_JSON" MAXD="$MAX_DPH" python3 <<'PY'
import json,os
maxd=float(os.environ["MAXD"]); data=json.loads(os.environ["OFFER_JSON"])
ok=[o for o in data if o.get("dph_total",1e9)<=maxd]
print(ok[0]["id"] if ok else "")
PY
)"
[ -z "$ASK_ID" ] && { echo "no offer <= \$$MAX_DPH/hr"; exit 1; }
echo "offer $ASK_ID"

CREATE="$($VAST create instance "$ASK_ID" --image "$IMAGE" --disk "$DISK" --ssh --direct --raw)"
IID="$(CREATE="$CREATE" python3 -c 'import json,os;print(json.loads(os.environ["CREATE"]).get("new_contract",""))')"
[ -z "$IID" ] && { echo "create failed: $CREATE"; exit 1; }
echo "instance $IID created (KEEP-ALIVE — destroy with remote/down.sh)"

for k in "$HOME/.ssh/id_ed25519.pub" "$HOME/.ssh/id_rsa.pub"; do
  [ -f "$k" ] && $VAST attach ssh "$IID" "$(cat "$k")" >/dev/null 2>&1 && break
done

HOST=""; PORT=""
for i in $(seq 1 90); do
  SH="$($VAST show instances --raw)"
  read -r HOST PORT STATUS < <(SH="$SH" IID="$IID" python3 <<'PY'
import json,os
iid=int(os.environ["IID"]); data=json.loads(os.environ["SH"])
data=data if isinstance(data,list) else data.get("instances",[])
for d in data:
    if d.get("id")==iid:
        host=d.get("public_ipaddr") or ""; port=""
        m=(d.get("ports") or {}).get("22/tcp")
        if m: port=m[0].get("HostPort","") or ""
        if not host or not port: host=d.get("ssh_host","") or ""; port=str(d.get("ssh_port","") or "")
        print(host.strip(), str(port).strip(), d.get("actual_status","") or ""); break
PY
)
  echo "  [$i] status=$STATUS host=$HOST port=$PORT"
  if [ "$STATUS" = "running" ] && [ -n "$HOST" ] && [ -n "$PORT" ]; then
    if ssh -p "$PORT" -o StrictHostKeyChecking=accept-new -o ConnectTimeout=8 "root@$HOST" true 2>/dev/null; then break; fi
  fi
  sleep 10
done
[ -n "$HOST" ] && [ -n "$PORT" ] || { echo "never became reachable (instance $IID still up)"; exit 1; }

printf 'IID=%s\nHOST=%s\nPORT=%s\n' "$IID" "$HOST" "$PORT" > "$ENVF"
echo "saved $ENVF:"; cat "$ENVF"
echo "ssh: ssh -p $PORT root@$HOST"
# one-time remote deps
ssh -p "$PORT" -o StrictHostKeyChecking=accept-new "root@$HOST" \
  "apt-get update -qq && apt-get install -y -qq rsync make g++ >/dev/null && echo deps-ok"
