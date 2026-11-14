#!/usr/bin/env bash
# Wraith Orchestrator — Termux launcher.
# Idempotent: kills any prior instance, waits for health, prints URLs.
set -u

cd "$(dirname "$0")"

# Locate the binary. Same directory as this script.
BIN=./orchestrator
if [ ! -f "$BIN" ]; then
    echo "✗ $BIN not found in $(pwd)"
    echo "  Build it on a desktop, copy to this directory."
    echo "  See README.md → 'Build the binary'."
    exit 1
fi
if [ ! -x "$BIN" ]; then
    echo "✗ $BIN is not executable. Run: chmod +x $BIN"
    exit 1
fi

# Architecture check. Catches the common mistake of pushing a
# x86_64 binary to an aarch64 phone (or vice versa).
ARCH=$(file -b "$BIN" 2>/dev/null | head -1)
case "$ARCH" in
    *aarch64*|*ARM\ aarch64*) ;;
    *x86-64*|*x86_64*) echo "✗ $BIN is x86_64, not aarch64. Android phones are aarch64."; exit 1 ;;
    *) echo "  (arch check skipped: $(file -b "$BIN" 2>/dev/null | head -c 60))" ;;
esac

# Config sanity.
[ -f orchestrator.toml ] || { echo "✗ orchestrator.toml missing in $(pwd)"; exit 1; }

# Runtime dirs.
mkdir -p logs modules data

# Kill any prior instance. Bracket in pattern avoids matching the
# pgrep/pkill command line itself.
if pgrep -f '[.]/orchestrator' >/dev/null 2>&1; then
    echo "  Stopping prior instance..."
    pkill -f '[.]/orchestrator' 2>/dev/null
    sleep 1
fi

# Android OOM-kills background processes when the screen locks.
# Best-effort wake lock. Non-Termux environments ignore this.
command -v termux-wake-lock >/dev/null 2>&1 && termux-wake-lock 2>/dev/null

# Start in background.
nohup "$BIN" --config orchestrator.toml > logs/wraith.log 2>&1 &
PID=$!
echo "Started PID: $PID"
echo "Log: logs/wraith.log"
echo ""

# Wait for health, up to 10 seconds (phone CPU is slower than laptop).
for i in $(seq 1 20); do
    if curl -sf -m 1 http://127.0.0.1:9090/healthz >/dev/null 2>&1; then
        echo "✓ Healthz:   ok"
        echo "✓ Dashboard: http://127.0.0.1:9090/ui"
        echo "✓ Metrics:   http://127.0.0.1:9090/metrics"
        echo ""
        echo "  Stop with: ./stop.sh"
        echo "  If screen locks and process dies, run: termux-wake-lock"
        exit 0
    fi
    sleep 0.5
done

echo "✗ Failed to start after 10s. Last 20 lines of logs/wraith.log:"
tail -20 logs/wraith.log 2>/dev/null
exit 1
