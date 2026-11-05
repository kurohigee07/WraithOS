#!/bin/bash
set -e
BOARD_DIR="$(dirname "$0")"
BIN_DIR="${BINARIES_DIR:-${BASE_DIR}/images}"
BUILD_D="${BUILD_DIR:-${BASE_DIR}/build}"
HOST_D="${HOST_DIR:-${BASE_DIR}/host}"

GRUB_CORE_DIR="${BUILD_D}/grub2-2.12/build-arm64-efi/grub-core"
if [ ! -d "$GRUB_CORE_DIR" ]; then
    echo "[err] GRUB arm64-efi not found: $GRUB_CORE_DIR"
    ls -la "${BUILD_D}/grub2-2.12/" 2>/dev/null
    exit 1
fi

GRUB_MKIMAGE="${HOST_D}/bin/grub-mkimage"
GENIMAGE="${HOST_D}/bin/genimage"

rm -rf "${BIN_DIR}/EFI" "${BIN_DIR}/grub" "${BIN_DIR}/Image-a" "${BIN_DIR}/Image-b"
rm -f "${BIN_DIR}/boot.vfat" "${BIN_DIR}/wraithos-arm.img"

cp "${BIN_DIR}/Image" "${BIN_DIR}/Image-a"
cp "${BIN_DIR}/Image" "${BIN_DIR}/Image-b"
mkdir -p "${BIN_DIR}/EFI/BOOT" "${BIN_DIR}/grub/arm64-efi"
cp "${BOARD_DIR}/grub-arm.cfg" "${BIN_DIR}/grub/grub.cfg"
cp "$GRUB_CORE_DIR"/*.mod "${BIN_DIR}/grub/arm64-efi/" 2>/dev/null || true
cp "$GRUB_CORE_DIR"/*.lst "${BIN_DIR}/grub/arm64-efi/" 2>/dev/null || true

ENVFILE="${BIN_DIR}/grub/grubenv"
{
    printf '# GRUB Environment Block\n'
    printf 'slot=A\n'
    printf 'tries=0\n'
    printf 'good=1\n'
    printf 'seq=0\n'
    printf 'booted=A\n'
    printf 'lkg=A\n'
} > "$ENVFILE"
N=$((1024 - $(wc -c < "$ENVFILE")))
printf '%*s' "$N" '' | tr ' ' '#' >> "$ENVFILE"
chmod 0644 "$ENVFILE"

GRUB_EFI="${BIN_DIR}/EFI/BOOT/BOOTAA64.EFI"
"$GRUB_MKIMAGE" \
    -d "$GRUB_CORE_DIR" \
    -O arm64-efi \
    -o "$GRUB_EFI" \
    -p "(hd0,gpt1)/grub" \
    normal configfile linux test part_gpt fat search search_fs_uuid loadenv extcmd

echo "[2] BOOTAA64.EFI: $(stat -c%s "$GRUB_EFI") bytes"

GEN_TMP="${BUILD_D}/genimage-arm.tmp"
rm -rf "$GEN_TMP"

DATA_STAGE="${BUILD_D}/wraithos-arm-data-stage"
rm -rf "$DATA_STAGE"
mkdir -p "$DATA_STAGE"

"$GENIMAGE" \
    --rootpath "$DATA_STAGE" \
    --tmppath "$GEN_TMP" \
    --inputpath "${BIN_DIR}" \
    --outputpath "${BIN_DIR}" \
    --config "${BOARD_DIR}/genimage-arm.cfg"

echo ""
echo "IMAGE: ${BIN_DIR}/wraithos-arm.img"
ls -lh "${BIN_DIR}/wraithos-arm.img"
