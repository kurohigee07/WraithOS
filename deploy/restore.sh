#!/bin/bash
set -euo pipefail
if [ $# -ne 1 ]; then echo "usage: $0 <filename.tar.gz>"; exit 1; fi
FILENAME="$1"
REMOTE="oci:wraith-bucket/daily"
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

echo "[restore] downloading $FILENAME from $REMOTE..."
rclone copy "$REMOTE/$FILENAME" "$TMP/"

echo "[restore] stopping service..."
systemctl --user stop orchestrator.service || true

tar -xzf "$TMP/$FILENAME" -C "$TMP"

[ -f "$TMP/orchestrator.db" ] && cp -f "$TMP/orchestrator.db" ./
[ -f "$TMP/orchestrator.toml" ] && cp -f "$TMP/orchestrator.toml" ./
[ -d "$TMP/modules" ] && rm -rf modules && cp -a "$TMP/modules" ./
[ -f "$TMP/orchestrator.service" ] && cp -f "$TMP/orchestrator.service" "$HOME/.config/systemd/user/"
[ -d "$TMP/deploy" ] && rm -rf deploy && cp -a "$TMP/deploy" ./

systemctl --user daemon-reload
systemctl --user start orchestrator.service
sleep 3

if curl -sf -m 2 http://127.0.0.1:9090/healthz >/dev/null; then
    echo "[restore] OK - service healthy"
else
    echo "[restore] WARN - service started but healthz failed"
    systemctl --user status orchestrator.service --no-pager | head -10
    exit 1
fi
