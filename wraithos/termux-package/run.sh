#!/data/data/com.termux/files/usr/bin/bash
# Wraith Orchestrator — Termux launcher
cd "$(dirname "$0")"

# Ensure logs dir
mkdir -p logs modules

# Kill existing instance
pkill -f './orchestrator' 2>/dev/null

# Start in background
nohup ./orchestrator --config orchestrator.toml > logs/wraith.log 2>&1 &
echo "Started PID: $!"
echo "Log: logs/wraith.log"
echo ""

# Wait for health
sleep 3
if curl -sf -m 2 http://127.0.0.1:9090/healthz >/dev/null; then
    echo "✓ Healthz: ok"
    echo "✓ Dashboard: http://127.0.0.1:9090/ui"
    echo "✓ Metrics:   http://127.0.0.1:9090/metrics"
else
    echo "✗ Failed to start. Check logs/wraith.log:"
    tail -20 logs/wraith.log
fi
