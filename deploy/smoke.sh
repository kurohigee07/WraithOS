#!/bin/bash
set -uo pipefail
cd "$(dirname "$0")/.."

HEALTH_URL="http://127.0.0.1:9090/healthz"
METRICS_URL="http://127.0.0.1:9090/metrics"
DEBUG_URL="http://127.0.0.1:9090/debug/state"

echo "[smoke] pre-check healthz..."
curl -sf -m 2 "$HEALTH_URL" >/dev/null || { echo "service not running"; exit 1; }

echo "[smoke] restarting service..."
systemctl --user restart orchestrator.service

echo "[smoke] waiting for healthz..."
READY=0
for i in $(seq 1 10); do
    if curl -sf -m 1 "$HEALTH_URL" >/dev/null 2>&1; then READY=1; break; fi
    sleep 1
done

if [ "$READY" -ne 1 ]; then
    echo "[smoke] FAILED — service not ready in 10s"
    systemctl --user status orchestrator.service --no-pager | head -15
    exit 1
fi

echo "[smoke] post-restart assertions..."
PASS=0; FAIL=0
chk() {
    if eval "$1" >/dev/null 2>&1; then echo "PASS: $2"; PASS=$((PASS+1));
    else echo "FAIL: $2"; FAIL=$((FAIL+1)); fi
}
chk '[ "$(systemctl --user is-active orchestrator.service)" = "active" ]' "service active"
chk 'curl -sf -m 2 "$HEALTH_URL" | grep -q "^ok$"' "healthz ok"
chk 'curl -sf -m 2 "$METRICS_URL" | grep -q "wasm_executions_total"' "wasm counter present"
chk 'curl -sf -m 2 "$METRICS_URL" | grep -q "wasm_pool_in_flight"' "pool gauge present"
chk 'curl -sf -m 2 "$DEBUG_URL" | grep -q "uptime_secs"' "/debug/state reachable"
chk 'curl -sf -m 2 "$DEBUG_URL" | grep -q "sample"' "/debug/state lists modules"

echo ""
echo "[smoke] RESULT: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ]
