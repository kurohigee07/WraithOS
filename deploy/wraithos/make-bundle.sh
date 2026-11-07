#!/bin/bash
# Build RAUC bundle from current rootfs.squashfs for x86 or ARM.
# Usage: make-bundle.sh <x86|arm> <version>
set -euo pipefail

ARCH="${1:-x86}"
VERSION="${2:-v0.29.0}"
REPO="$(cd "$(dirname "$0")/../.." && pwd)"
BUILD="$HOME/buildroot-2024.02.9"
case "$ARCH" in
    x86) COMPAT="wraithos-x86_64"; IMG="$BUILD/output/images/rootfs.squashfs" ;;
    arm) COMPAT="wraithos-aarch64"; IMG="$BUILD/output-arm-m31/images/rootfs.squashfs" ;;
    *) echo "usage: $0 <x86|arm> <version>"; exit 1 ;;
esac

[ -f "$IMG" ] || { echo "rootfs.squashfs tidak ada: $IMG"; exit 1; }

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

# Certs — default throwaway, ganti dengan production cert untuk release
CA_DIR="$REPO/deploy/wraithos/rauc-ca"
if [ ! -f "$CA_DIR/ca.cert.pem" ]; then
    echo "[bundle] no CA — generating throwaway for dev"
    mkdir -p "$CA_DIR"
    openssl req -x509 -newkey rsa:2048 -nodes -days 30 \
        -keyout "$CA_DIR/ca.key" -out "$CA_DIR/ca.cert.pem" \
        -subj "/CN=WraithOS Dev CA" 2>/dev/null
    openssl req -newkey rsa:2048 -nodes \
        -keyout "$CA_DIR/dev.key" -out "$CA_DIR/dev.csr" \
        -subj "/CN=WraithOS Dev Signing" 2>/dev/null
    openssl x509 -req -days 30 \
        -in "$CA_DIR/dev.csr" -CA "$CA_DIR/ca.cert.pem" -CAkey "$CA_DIR/ca.key" -CAcreateserial \
        -out "$CA_DIR/dev.cert.pem" 2>/dev/null
fi

CERT="$CA_DIR/dev.cert.pem"
KEY="$CA_DIR/dev.key"
[ -f "$CERT" ] && [ -f "$KEY" ] || { echo "certs missing di $CA_DIR"; exit 1; }

sudo docker run --rm --network host -v "$WORK:/work:Z" -w /work --security-opt label=disable ubuntu:24.04 \
  bash -c "apt-get -o Acquire::ForceIPv4=true update -qq >/dev/null 2>&1 && \
           DEBIAN_FRONTEND=noninteractive apt-get -o Acquire::ForceIPv4=true install -y -qq rauc squashfs-tools >/dev/null 2>&1 && \
           rauc bundle --cert=/work/content/../dev.cert.pem --key=/work/content/../dev.key \
                       /work/content /work/bundle/test.raucb 2>&1 | tail -3" \
  || true

# Copy certs into work for docker access
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
