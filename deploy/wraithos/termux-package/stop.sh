#!/usr/bin/env bash
# Wraith Orchestrator — graceful stop.
set -u
cd "$(dirname "$0")"

PIDS=$(pgrep -f '[.]/orchestrator' 2>/dev/null)
if [ -z "$PIDS" ]; then
    echo "Not running"
    exit 0
fi

# SIGTERM first. The orchestrator drains the audit writer on
# shutdown (2 s bound in AppContext::shutdown), so give it time.
pkill -TERM -f '[.]/orchestrator' 2>/dev/null || true
for i in $(seq 1 10); do
    pgrep -f '[.]/orchestrator' >/dev/null 2>&1 || {
        command -v termux-wake-unlock >/dev/null 2>&1 && termux-wake-unlock 2>/dev/null
        echo "Stopped (graceful)"
        exit 0
    }
    sleep 0.3
done

# If still alive after 3 s, SIGKILL.
pkill -KILL -f '[.]/orchestrator' 2>/dev/null || true
command -v termux-wake-unlock >/dev/null 2>&1 && termux-wake-unlock 2>/dev/null
echo "Stopped (forced after timeout)"
