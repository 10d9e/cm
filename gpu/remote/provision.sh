#!/usr/bin/env bash
# Provision a vast.ai 1x RTX 4090, build + benchmark the codec, fetch results,
# and ALWAYS destroy the instance on exit. Cost-guarded (rejects offers above
# MAX_DPH). Run from anywhere; paths are resolved from this script's location.
#
#   MAX_DPH=0.50 remote/provision.sh
#
# API key: read from ~/.config/vastai/vast_api_key, else .notes, else $VAST_API_KEY.
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
MAX_DPH="${MAX_DPH:-0.50}"
# Toolkit must be <= the host driver's max CUDA or the runtime tries (and fails)
# "forward compatibility". 12.2 is broadly supported on 4090 hosts; the search
# below also requires the host to advertise CUDA >= this.
IMAGE="${IMAGE:-nvidia/cuda:12.2.2-devel-ubuntu22.04}"
DISK="${DISK:-24}"

# ---- locate the vast CLI ----------------------------------------------------
# pip --user installs the console script under python's user-base bin.
PATH="$(python3 -m site --user-base 2>/dev/null)/bin:$PATH"
VAST=""
for c in vastai vast; do
  if command -v "$c" >/dev/null 2>&1 && "$c" --version >/dev/null 2>&1; then VAST="$c"; break; fi
done
if [ -z "$VAST" ]; then
  echo "vast CLI not found; installing via pip --user ..."
  python3 -m pip install --user --quiet vastai
  PATH="$(python3 -m site --user-base 2>/dev/null)/bin:$PATH"
  command -v vastai >/dev/null 2>&1 && VAST="vastai"
fi
[ -z "$VAST" ] && { echo "could not locate or install the vast CLI"; exit 1; }
echo "using vast CLI: $VAST"

# ---- api key ----------------------------------------------------------------
KEY=""
for p in "$HOME/.config/vastai/vast_api_key" "$ROOT/.notes"; do
  if [ -f "$p" ]; then KEY="$(tr -d ' \t\n\r' < "$p")"; [ -n "$KEY" ] && break; fi
done
[ -z "$KEY" ] && KEY="${VAST_API_KEY:-}"
[ -z "$KEY" ] && { echo "no API key found"; exit 1; }
$VAST set api-key "$KEY" >/dev/null

# ---- pick the cheapest qualifying offer -------------------------------------
QUERY="gpu_name=RTX_4090 num_gpus=1 rentable=true cuda_vers>=12.2 dph<$MAX_DPH"
echo "searching offers: $QUERY"
OFFER_JSON="$($VAST search offers "$QUERY" -o 'dph' --raw)"
ASK_ID="$(OFFER_JSON="$OFFER_JSON" MAXD="$MAX_DPH" python3 <<'PY'
import json,os
maxd=float(os.environ["MAXD"]); data=json.loads(os.environ["OFFER_JSON"])
ok=[o for o in data if o.get("dph_total",1e9)<=maxd]
print(ok[0]["id"] if ok else "")
PY
)"
[ -z "$ASK_ID" ] && { echo "no offer <= \$$MAX_DPH/hr"; exit 1; }
echo "selected offer $ASK_ID"

# ---- create instance --------------------------------------------------------
CREATE="$($VAST create instance "$ASK_ID" --image "$IMAGE" --disk "$DISK" --ssh --direct --raw)"
IID="$(CREATE="$CREATE" python3 <<'PY'
import json,os
try: print(json.loads(os.environ["CREATE"]).get("new_contract",""))
except Exception: print("")
PY
)"
[ -z "$IID" ] && { echo "create failed: $CREATE"; exit 1; }

# Attach our SSH key to the instance up front (belt-and-suspenders; the proxy
# host can otherwise reject the key for a while).
for k in "$HOME/.ssh/id_ed25519.pub" "$HOME/.ssh/id_rsa.pub"; do
  [ -f "$k" ] && $VAST attach ssh "$IID" "$(cat "$k")" >/dev/null 2>&1 && break
done
echo "instance $IID created"

# NOTE: -y is required — `destroy instance` prompts for confirmation and would
# otherwise read EOF and ABORT in this non-interactive context, leaking the
# (billing) instance.
cleanup() { echo "destroying instance $IID"; $VAST destroy instance "$IID" -y >/dev/null 2>&1 || true; }
trap cleanup EXIT

# ---- wait until running + ssh reachable -------------------------------------
HOST=""; PORT=""
for i in $(seq 1 60); do
  SH="$($VAST show instances --raw)"
  # Prefer the DIRECT endpoint (public IP + host-mapped port for container 22).
  # The proxy host (ssh_host/ssh_port) can reject the key for a long time; the
  # direct IP comes up cleanly. Fall back to the proxy if no direct mapping yet.
  read -r HOST PORT STATUS < <(SH="$SH" IID="$IID" python3 <<'PY'
import json,os
iid=int(os.environ["IID"]); data=json.loads(os.environ["SH"])
data=data if isinstance(data,list) else data.get("instances",[])
for d in data:
    if d.get("id")==iid:
        host=d.get("public_ipaddr") or ""; port=""
        ports=d.get("ports") or {}
        m=ports.get("22/tcp")
        if m and isinstance(m,list) and m: port=m[0].get("HostPort","") or ""
        if not host or not port:
            host=d.get("ssh_host","") or ""; port=str(d.get("ssh_port","") or "")
        print(host.strip(), str(port).strip(), d.get("actual_status","") or "")
        break
PY
)
  echo "  [$i] status=$STATUS host=$HOST port=$PORT"
  if [ "$STATUS" = "running" ] && [ -n "$HOST" ] && [ -n "$PORT" ]; then
    if ssh -p "$PORT" -o StrictHostKeyChecking=accept-new -o ConnectTimeout=8 \
        "root@$HOST" true 2>/dev/null; then break; fi
  fi
  sleep 10
done
[ -n "$HOST" ] && [ -n "$PORT" ] || { echo "instance never became reachable"; exit 1; }

# ---- remote deps, sync, build, benchmark ------------------------------------
SSH="ssh -p $PORT -o StrictHostKeyChecking=accept-new"
$SSH "root@$HOST" "apt-get update -qq && apt-get install -y -qq rsync make g++ >/dev/null"
bash "$HERE/sync.sh" "$HOST" "$PORT"

$SSH "root@$HOST" "set -e; cd /root/cm/gpu; nvidia-smi -L; \
  make gpu CUDA_ARCH=${CUDA_ARCH:-sm_89}; \
  echo '== GPU roundtrip selftest =='; \
  for f in ../corpus/*.bin; do ./gpu selftest \$f 65536; done; \
  echo '== benchmark =='; bash bench/run.sh ../corpus bench/results"

# ---- fetch results ----------------------------------------------------------
rsync -az -e "$SSH" "root@$HOST:/root/cm/gpu/bench/results.gpu.csv" "$ROOT/gpu/bench/" || true
rsync -az -e "$SSH" "root@$HOST:/root/cm/gpu/bench/results.cpu.csv" "$ROOT/gpu/bench/" || true
echo "done — results in gpu/bench/. Instance will be destroyed now."
