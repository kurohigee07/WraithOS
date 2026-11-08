#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."

VERSION="v0.41.0"
DIST_DIR="dist"
mkdir -p "$DIST_DIR"
rm -rf "$DIST_DIR"/*

# Fungsi buat build + package per target
package_target() {
    local TARGET=$1
    local ARCH_NAME=$2
    local PKG_NAME="wraith-${VERSION}-${ARCH_NAME}-linux-musl"
    local STAGE_DIR="${DIST_DIR}/${PKG_NAME}"

    echo "=== Building for $TARGET ==="
    cargo zigbuild --release --target "$TARGET"

    echo "=== Staging $PKG_NAME ==="
    mkdir -p "${STAGE_DIR}/deploy"
    
    # Copy binary
    cp "target/${TARGET}/release/orchestrator" "${STAGE_DIR}/orchestrator"
    
    # Copy config, docs, licenses
    cp orchestrator.toml "${STAGE_DIR}/"
    cp README.md "${STAGE_DIR}/"
    cp CHANGELOG.md "${STAGE_DIR}/"
    cp LICENSE-MIT LICENSE-APACHE "${STAGE_DIR}/"
    cp deploy/orchestrator.service "${STAGE_DIR}/deploy/"
    cp deploy/logrotate.conf "${STAGE_DIR}/deploy/"
    cp deploy/orchestrator-backup.service "${STAGE_DIR}/deploy/"
    cp deploy/orchestrator-backup.timer "${STAGE_DIR}/deploy/"
    cp deploy/backup.sh "${STAGE_DIR}/deploy/"
    cp deploy/install.sh "${STAGE_DIR}/deploy/"
    
    # Bikin file README khusus distribusi
    cat > "${STAGE_DIR}/INSTALL.txt" << 'EOF'
Wraith Orchestrator - Installation
==================================
1. Extract this tarball.
2. Move 'orchestrator' to /usr/local/bin/ (or keep in directory).
3. Copy deploy/orchestrator.service to ~/.config/systemd/user/
4. Edit orchestrator.toml as needed.
5. systemctl --user enable --now orchestrator.service
EOF

    # Bikin tarball
    echo "=== Packaging ${PKG_NAME}.tar.gz ==="
    tar -czf "${DIST_DIR}/${PKG_NAME}.tar.gz" -C "$DIST_DIR" "$PKG_NAME"
    rm -rf "$STAGE_DIR"
}

# Eksekusi build
package_target "x86_64-unknown-linux-musl" "x86_64"
package_target "aarch64-unknown-linux-musl" "aarch64"

# Generate checksum
echo "=== Generating SHA256 ==="
cd "$DIST_DIR"
sha256sum *.tar.gz > SHA256SUMS
cat SHA256SUMS
cd ..

echo ""
echo "=== DONE ==="
ls -lh "$DIST_DIR"/
