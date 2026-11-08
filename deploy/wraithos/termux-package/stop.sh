#!/data/data/com.termux/files/usr/bin/bash
cd "$(dirname "$0")"
pkill -f './orchestrator' 2>/dev/null && echo "Stopped" || echo "Not running"
