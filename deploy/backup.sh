#!/bin/bash
set -euo pipefail

cd "$(dirname "$0")/.."

REMOTE="oci:wraith-bucket/daily"
STAMP=$(date +%Y%m%d-%H%M%S)
FILENAME="orchestrator-$STAMP.tar.gz"
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

# ─── Fail-fast preconditions (no silent empty backup) ──────────────
if [ ! -f orchestrator.db ]; then
    echo "[backup] FATAL: orchestrator.db not found in $(pwd)" >&2
    echo "[backup] cwd=$(pwd) — expected ~/orchestrator-project" >&2
    exit 1
fi

if [ ! -f orchestrator.toml ]; then
    echo "[backup] FATAL: orchestrator.toml not found in $(pwd)" >&2
    exit 1
fi

# ─── DB consistent snapshot (WAL safe via sqlite3 .backup) ────────
if command -v sqlite3 >/dev/null 2>&1; then
    sqlite3 orchestrator.db ".backup '$TMP/orchestrator.db'" || {
        echo "[backup] FATAL: sqlite3 .backup failed" >&2
        exit 1
    }
else
    # Fallback: wal + shm + db (not transactionally consistent, but better than nothing)
    for f in orchestrator.db orchestrator.db-wal orchestrator.db-shm; do
        [ -f "$f" ] && cp -f "$f" "$TMP/" || true
    done
fi

# Verify snapshot is non-empty
if [ ! -s "$TMP/orchestrator.db" ]; then
    echo "[backup] FATAL: backup DB is empty at $TMP/orchestrator.db" >&2
    exit 1
fi

# ─── Other assets ──────────────────────────────────────────────────
cp -f orchestrator.toml "$TMP/"

if [ -d modules ]; then
    cp -a modules "$TMP/modules"
else
    mkdir -p "$TMP/modules"
    echo "[backup] WARN: no modules/ dir — backing up empty" >&2
fi

[ -f "$HOME/.config/systemd/user/orchestrator.service" ] && \
    cp -f "$HOME/.config/systemd/user/orchestrator.service" "$TMP/"

[ -d deploy ] && cp -a deploy "$TMP/deploy"

# ─── Stream to OCI ─────────────────────────────────────────────────
tar -czf - -C "$TMP" . | rclone rcat "$REMOTE/$FILENAME" || {
    echo "[backup] FATAL: rclone rcat failed — backup NOT uploaded" >&2
    exit 1
}

echo "[backup] OK -> $REMOTE/$FILENAME"

# ─── Retention: keep 7 newest ─────────────────────────────────────
rclone lsf "$REMOTE" --files-only | sort | head -n -7 | while read -r f; do
    echo "[backup] pruning old: $f"
    rclone delete "$REMOTE/$f"
done
