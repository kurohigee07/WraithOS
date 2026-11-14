#!/usr/bin/env bash
# Wraith Orchestrator — status check.
set -u
cd "$(dirname "$0")"

# Bracket in pattern avoids self-match.
PIDS=$(pgrep -f '[.]/orchestrator' 2>/dev/null)
if [ -z "$PIDS" ]; then
    echo "✗ Not running"
    exit 1
fi

echo "✓ Running (PID: $(echo "$PIDS" | head -1))"
if [ "$(echo "$PIDS" | wc -l)" -gt 1 ]; then
    echo "  WARNING: $(( $(echo "$PIDS" | wc -l) - 1 )) extra instance(s) running:"
    echo "$PIDS" | tail -n +2 | sed 's/^/    /'
fi
echo ""
echo "Healthz:  $(curl -s -m 2 http://127.0.0.1:9090/healthz || echo 'unreachable')"
echo "Readyz:   $(curl -s -m 2 http://127.0.0.1:9090/readyz || echo 'unreachable')"
echo "Uptime:   $(ps -o etime= -p "$(echo "$PIDS" | head -1)" 2>/dev/null | tr -d ' ' || echo 'unknown')"
