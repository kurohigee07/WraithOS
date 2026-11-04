#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."

DIST_DIR="${DIST_DIR:-dist}"
PUBKEY="${PUBKEY:-deploy/wraith-release.pub}"

if [ ! -f "$PUBKEY" ]; then
    echo "[verify] FATAL: public key $PUBKEY not found" >&2
    exit 1
fi

if [ ! -f "$DIST_DIR/SHA256SUMS" ] || [ ! -f "$DIST_DIR/SHA256SUMS.asc" ]; then
    echo "[verify] FATAL: SHA256SUMS or .asc missing in $DIST_DIR" >&2
    exit 1
fi

# Create temp keyring, import public key only
GNUPGHOME=$(mktemp -d)
export GNUPGHOME
trap 'rm -rf "$GNUPGHOME"' EXIT

gpg --batch --quiet --import "$PUBKEY" 2>/dev/null

echo "[verify] verifying SHA256SUMS.asc with $PUBKEY"
gpg --batch --verify "$DIST_DIR/SHA256SUMS.asc" "$DIST_DIR/SHA256SUMS"
echo ""

echo "[verify] verifying file checksums"
( cd "$DIST_DIR" && sha256sum -c SHA256SUMS )

echo ""
echo "[verify] all signatures valid"
