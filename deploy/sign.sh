#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."

KEY_EMAIL="${WRAITH_SIGNING_KEY:-release@wraith.local}"
DIST_DIR="${DIST_DIR:-dist}"

if ! gpg --list-secret-keys "$KEY_EMAIL" >/dev/null 2>&1; then
    echo "[sign] FATAL: signing key '$KEY_EMAIL' not found in keyring" >&2
    echo "[sign] import first: gpg --import wraith-release-secret.asc" >&2
    exit 1
fi

if [ ! -d "$DIST_DIR" ]; then
    echo "[sign] FATAL: $DIST_DIR not found" >&2
    exit 1
fi

TARBALLS=$(find "$DIST_DIR" -maxdepth 1 -name '*.tar.gz' -type f | sort)
if [ -z "$TARBALLS" ]; then
    echo "[sign] FATAL: no *.tar.gz in $DIST_DIR" >&2
    exit 1
fi

echo "[sign] signing with key $KEY_EMAIL"
echo ""

# Regenerate SHA256SUMS from current tarballs
( cd "$DIST_DIR" && sha256sum *.tar.gz > SHA256SUMS )

# Sign SHA256SUMS (detached, armored)
gpg --batch --yes --armor --detach-sign \
    --local-user "$KEY_EMAIL" \
    --output "$DIST_DIR/SHA256SUMS.asc" \
    "$DIST_DIR/SHA256SUMS"

# Sign each tarball (detached, binary)
while IFS= read -r tb; do
    [ -z "$tb" ] && continue
    echo "[sign] $(basename "$tb")"
    gpg --batch --yes --detach-sign \
        --local-user "$KEY_EMAIL" \
        --output "${tb}.sig" \
        "$tb"
done <<< "$TARBALLS"

echo ""
echo "[sign] output:"
ls -lh "$DIST_DIR"/SHA256SUMS* "$DIST_DIR"/*.tar.gz.sig
