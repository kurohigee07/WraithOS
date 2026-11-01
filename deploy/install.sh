#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."

echo "[install] building release binary (x86_64 musl)..."
cargo zigbuild --release --target x86_64-unknown-linux-musl

echo "[install] installing systemd unit..."
mkdir -p ~/.config/systemd/user
cp deploy/orchestrator.service ~/.config/systemd/user/orchestrator.service

echo "[install] daemon-reload + enable..."
systemctl --user daemon-reload
systemctl --user enable orchestrator.service

echo "[install] (re)starting service..."
systemctl --user restart orchestrator.service
sleep 3

echo "[install] waiting for healthz..."
for i in $(seq 1 10); do
    if curl -sf -m 2 http://127.0.0.1:9090/healthz >/dev/null 2>&1; then
        echo "[install] OK — service active, healthz reachable"
        systemctl --user status orchestrator.service --no-pager | head -3
        exit 0
    fi
    sleep 1
done
echo "[install] FAILED — healthz unreachable after 10s"
systemctl --user status orchestrator.service --no-pager | head -15
exit 1
