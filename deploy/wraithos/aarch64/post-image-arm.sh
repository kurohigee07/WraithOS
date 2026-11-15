#!/bin/bash
set -e
BOARD_DIR="$(dirname "$0")"
BIN_DIR="${BINARIES_DIR:-${BASE_DIR}/images}"
BUILD_D="${BUILD_DIR:-${BASE_DIR}/build}"
HOST_D="${HOST_DIR:-${BASE_DIR}/host}"
export PATH="${HOST_D}/bin:${PATH}"

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
    printf 'A_OK=1\n'
    printf 'B_OK=1\n'
    printf 'A_TRY=0\n'
    printf 'B_TRY=0\n'
    printf 'ORDER=A B\n'
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

# ─── [1.5] dm-verity combined rootfs + grub.cfg ──────────────────
echo "[1.5] Computing dm-verity tree + combined rootfs"
VERITY_BIN=$(command -v veritysetup || echo "/usr/sbin/veritysetup")
[ -x "$VERITY_BIN" ] || { echo "[err] veritysetup missing"; exit 1; }
SQ="${BIN_DIR}/rootfs.squashfs"
SZ=$(stat -c%s "$SQ")
DB=$(( (SZ + 4095) / 4096 ))
HS=$(( DB + 1 ))
DL=$(( DB * 8 ))
VERITY_OUT="$("$VERITY_BIN" format --hash=sha256 --data-block-size=4096 --hash-block-size=4096 "$SQ" "${BIN_DIR}/rootfs.verity" 2>&1)" || { echo "[err] verity: $VERITY_OUT"; exit 1; }
HASH=$(echo "$VERITY_OUT" | grep '^Root hash:' | awk '{print $NF}')
SALT=$(echo "$VERITY_OUT" | grep '^Salt:' | awk '{print $NF}')
[ -n "$HASH" ] && [ -n "$SALT" ] || { echo "[err] verity parse"; exit 1; }
echo "  data_blocks=$DB hash_start=$HS dev_len=$DL"
COMB="${BIN_DIR}/rootfs-verity.squashfs"
cp "$SQ" "$COMB"
cat "${BIN_DIR}/rootfs.verity" >> "$COMB"
echo "  combined: $(stat -c%s "$COMB") bytes"
sed -e "s|@DATA_BLOCKS@|${DB}|g" -e "s|@HASH_START@|${HS}|g" -e "s|@DEV_LEN@|${DL}|g" \
    -e "s|@HASH_A@|${HASH}|g" -e "s|@SALT_A@|${SALT}|g" -e "s|@HASH_B@|${HASH}|g" -e "s|@SALT_B@|${SALT}|g" \
    "${BOARD_DIR}/grub-arm.cfg" > "${BIN_DIR}/grub/grub.cfg"
echo "  grub.cfg generated"

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
