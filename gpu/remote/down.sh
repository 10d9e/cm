#!/usr/bin/env bash
# Destroy the keep-alive instance recorded in /tmp/cmgpu/instance.env.
# Run ONLY when you actually want to stop paying.
set -euo pipefail
ENVF="/tmp/cmgpu/instance.env"
PATH="$(python3 -m site --user-base 2>/dev/null)/bin:$PATH"
[ -f "$ENVF" ] || { echo "no $ENVF — nothing to destroy"; exit 0; }
. "$ENVF"
echo "destroying instance $IID ($HOST:$PORT)"
vastai destroy instance "$IID" -y
rm -f "$ENVF"
echo "destroyed; verifying none remain:"
vastai show instances --raw 2>/dev/null | python3 -c "import json,sys;d=json.load(sys.stdin);print('  (none)' if not d else [x.get('id') for x in d])"
