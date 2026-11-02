#!/bin/bash
# Test WebSocket event stream: subscribe, trigger module, verify events.
set -u
LOG=$(mktemp)
RESP=$(mktemp)
trap 'pkill -f "tail -f /dev/null" 2>/dev/null; rm -f "$LOG" "$RESP"' EXIT

echo "[ws-test] subscribing..."
tail -f /dev/null | websocat ws://127.0.0.1:9090/ws/events > "$LOG" 2>&1 &
sleep 2

echo "[ws-test] triggering wraith exec sample..."
wraith exec sample > "$RESP" 2>&1
sleep 2

echo ""
echo "=== captured events ==="
cat "$LOG"
echo ""
echo "=== assertions ==="
PASS=0; FAIL=0
for kind in sample.emitted rhai.processed; do
    if grep -q "$kind" "$LOG"; then
        echo "PASS: $kind"; PASS=$((PASS+1))
    else
        echo "FAIL: $kind"; FAIL=$((FAIL+1))
    fi
done
echo ""
echo "RESULT: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ]
