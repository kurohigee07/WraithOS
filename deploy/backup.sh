#!/bin/bash
set -euo pipefail

REMOTE="oci:wraith-bucket/daily"
STAMP=$(date +%Y%m%d-%H%M%S)
FILENAME="orchestrator-$STAMP.tar.gz"
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

if command -v sqlite3 >/dev/null 2>&1 && [ -f orchestrator.db ]; then
    sqlite3 orchestrator.db ".backup '$TMP/orchestrator.db'"
else
    cp -f orchestrator.db* "$TMP/" 2>/dev/null || true
fi

cp -f orchestrator.toml "$TMP/" 2>/dev/null || true
cp -a modules "$TMP/modules" 2>/dev/null || mkdir -p "$TMP/modules"
cp -f "$HOME/.config/systemd/user/orchestrator.service" "$TMP/" 2>/dev/null || true
cp -a deploy "$TMP/deploy" 2>/dev/null || true

tar -czf - -C "$TMP" . | rclone rcat "$REMOTE/$FILENAME"
echo "[backup] OK -> $REMOTE/$FILENAME"

rclone lsf "$REMOTE" --files-only | sort | head -n -7 | while read -r f; do
    echo "[backup] pruning old: $f"
    rclone delete "$REMOTE/$f"
done
