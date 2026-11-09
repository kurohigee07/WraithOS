#!/bin/bash
# Build RAUC bundle from current rootfs.squashfs for x86 or ARM.
# Usage: make-bundle.sh <x86|arm> <version>
#
# Production CA is REQUIRED at $CA_PROD (default: ~/backups-b/rauc-ca-prod).
# No throwaway cert generation — bundle must be signed with production CA.
set -euo pipefail

ARCH="${1:-x86}"
VERSION="${2:-v0.29.0}"
REPO="$(cd "$(dirname "$0")/../.." && pwd)"
BUILD="$HOME/buildroot-2024.02.9"
CA_PROD="${CA_PROD:-$HOME/backups-b/rauc-ca-prod}"

case "$ARCH" in
    x86) COMPAT="wraithos-x86_64"; IMG="$BUILD/output/images/rootfs.squashfs" ;;
    arm) COMPAT="wraithos-aarch64"; IMG="$BUILD/output-arm-m31/images/rootfs.squashfs" ;;
    *) echo "usage: $0 <x86|arm> <version>"; exit 1 ;;
esac

[ -f "$IMG" ] || { echo "FATAL: rootfs.squashfs tidak ada: $IMG"; exit 1; }

# ── Production CA — MUST exist, no fallback ─────────────────────────────
CERT="$CA_PROD/dev.cert.pem"
KEY="$CA_PROD/dev.key"
CA_CERT="$CA_PROD/ca.cert.pem"
[ -f "$CA_CERT" ] || { echo "FATAL: CA cert tidak ada: $CA_CERT"; exit 1; }
[ -f "$CERT"    ] || { echo "FATAL: signing cert tidak ada: $CERT"; exit 1; }
[ -f "$KEY"     ] || { echo "FATAL: signing key tidak ada: $KEY"; exit 1; }

# ── Verify CA fingerprint matches expected production CA ───────────────
EXPECTED_FP="5C:75:AC:3B:03:12:B1:6D:15:AF:30:0D:98:1B:6E:55:F6:D0:BC:CC:16:BE:40:D9:A7:FF:97:48:EB:98:FD:38"
ACTUAL_FP=$(openssl x509 -in "$CA_CERT" -noout -fingerprint -sha256 | sed 's/^.*=//')
if [ "$ACTUAL_FP" != "$EXPECTED_FP" ]; then
    echo "FATAL: CA fingerprint mismatch"
    echo "  expected: $EXPECTED_FP"
    echo "  actual:   $ACTUAL_FP"
    exit 1
fi
echo "[bundle] CA fingerprint OK: $ACTUAL_FP"

WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
mkdir -p "$WORK/content" "$WORK/bundle"
cp "$IMG" "$WORK/content/rootfs.squashfs"
cat > "$WORK/content/manifest.raucm" <<EOF
[update]
compatible=$COMPAT
version=$VERSION
description=WraithOS $ARCH $VERSION

[bundle]
format=verity

[image.rootfs]
filename=rootfs.squashfs
EOF

cp "$CERT" "$WORK/dev.cert.pem"
cp "$KEY" "$WORK/dev.key"

sudo docker run --rm --network host -v "$WORK:/work:Z" -w /work --security-opt label=disable ubuntu:24.04 \
  bash -c "apt-get -o Acquire::ForceIPv4=true update -qq >/dev/null 2>&1 && \
           DEBIAN_FRONTEND=noninteractive apt-get -o Acquire::ForceIPv4=true install -y -qq rauc squashfs-tools >/dev/null 2>&1 && \
           rauc bundle --cert=/work/dev.cert.pem --key=/work/dev.key \
                       /work/content /work/bundle/$ARCH-$VERSION.raucb"

OUT="$REPO/dist/wraithos-bundles/$ARCH-$VERSION.raucb"
mkdir -p "$(dirname "$OUT")"
sudo cp "$WORK/bundle/$ARCH-$VERSION.raucb" "$OUT"
sudo chown "$(id -u):$(id -g)" "$OUT"
ls -lh "$OUT"
echo "[bundle] done: $OUT"
