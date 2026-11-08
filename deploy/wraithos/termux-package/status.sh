#!/data/data/com.termux/files/usr/bin/bash
if pgrep -f './orchestrator' >/dev/null; then
    echo "✓ Running (PID: $(pgrep -f './orchestrator' | head -1))"
    echo ""
    echo "Healthz:  $(curl -s -m 2 http://127.0.0.1:9090/healthz || echo 'unreachable')"
    echo "Readyz:   $(curl -s -m 2 http://127.0.0.1:9090/readyz || echo 'unreachable')"
else
    echo "✗ Not running"
fi
