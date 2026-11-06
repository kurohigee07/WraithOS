#!/bin/bash
# Copy orchestrator binary ke WraithOS rootfs_overlay sebelum build.
# Binary TIDAK di-commit ke git (24 MB per versi = repo bloat).
# Jalankan setiap kali orchestrator di-rebuild sebelum WraithOS build.
set -euo pipefail

ORCH_REPO="${ORCH_REPO:-$HOME/orchestrator-project}"
X86_OVERLAY="${X86_OVERLAY:-$HOME/wraithos-br2-ext/board/wraithos/rootfs_overlay/sbin}"
ARM_OVERLAY="${ARM_OVERLAY:-$HOME/wraithos-aarch64-br2-ext/board/wraithos/rootfs_overlay/sbin}"

X86_BIN="$ORCH_REPO/target/x86_64-unknown-linux-musl/release/orchestrator"
ARM_BIN="$ORCH_REPO/target/aarch64-unknown-linux-musl/release/orchestrator"

sync_one() {
    local src="$1" dst_dir="$2" label="$3"
    if [ ! -f "$src" ]; then
        echo "[sync] SKIP $label: source not found at $src"
        return 1
    fi
    if [ ! -d "$dst_dir" ]; then
        echo "[sync] SKIP $label: overlay dir not found at $dst_dir"
        return 1
    fi
    local sz=$(stat -c%s "$src")
    local sha=$(sha256sum "$src" | cut -c1-12)
    echo "[sync] $label: $src ($sz bytes, sha=$sha)"
    cp -fv "$src" "$dst_dir/wraith-init"
    chmod +x "$dst_dir/wraith-init"
    echo "[sync] $label: OK -> $dst_dir/wraith-init"
}

echo "=== WraithOS binary sync ==="
echo ""

RC=0
sync_one "$X86_BIN" "$X86_OVERLAY" "x86_64" || RC=$?
sync_one "$ARM_BIN" "$ARM_OVERLAY" "aarch64" || RC=$?

echo ""
echo "=== Verify ==="
for d in "$X86_OVERLAY" "$ARM_OVERLAY"; do
    if [ -f "$d/wraith-init" ]; then
        file "$d/wraith-init" | sed 's/^/    /'
        sha256sum "$d/wraith-init" | sed 's/^/    /'
    fi
done

exit $RC
