#!/bin/bash
set -e

echo ""
echo "======================================"
echo "WraithOS post-image: building disk"
echo "======================================"

BOARD_DIR="$(dirname "$0")"
BIN_DIR="${BINARIES_DIR:-${BASE_DIR}/images}"
BUILD_D="${BUILD_DIR:-${BASE_DIR}/build}"
HOST_D="${HOST_DIR:-${BASE_DIR}/host}"

# GRUB core dir (build tree → fallback backup)
GRUB_CORE_DIR="${BUILD_D}/grub2-2.12/build-i386-pc/grub-core"
if [ ! -d "$GRUB_CORE_DIR" ]; then
    GRUB_CORE_DIR="$HOME/wraithos-phase2-backup/grub-modules/grub-core"
fi
if [ ! -d "$GRUB_CORE_DIR" ]; then
    echo "[err] GRUB core dir not found"
    exit 1
fi

GRUB_MKIMAGE="${HOST_D}/bin/grub-mkimage"
GRUB_EDITENV="${HOST_D}/bin/grub-editenv"
GENIMAGE="${HOST_D}/bin/genimage"

for t in "$GRUB_MKIMAGE" "$GRUB_EDITENV" "$GENIMAGE"; do
    [ -x "$t" ] || { echo "[err] missing: $t"; exit 1; }
done

# ─── [1/6] Boot partition staging ────────────────────────────────────
echo "[1/6] Preparing boot stage"
BOOT_STAGE="${BUILD_D}/wraithos-boot"
rm -rf "$BOOT_STAGE"
mkdir -p "$BOOT_STAGE/grub/i386-pc"

cp "${BIN_DIR}/bzImage" "$BOOT_STAGE/bzImage"
cp "${BOARD_DIR}/grub.cfg" "$BOOT_STAGE/grub/grub.cfg"

# grubenv with default ORDER=A B
"$GRUB_EDITENV" "$BOOT_STAGE/grub/grubenv" create
"$GRUB_EDITENV" "$BOOT_STAGE/grub/grubenv" set ORDER="A B"
echo "  grubenv: $(stat -c%s "$BOOT_STAGE/grub/grubenv") bytes"

# GRUB modules
cp "$GRUB_CORE_DIR"/*.mod "$BOOT_STAGE/grub/i386-pc/" 2>/dev/null || true
cp "$GRUB_CORE_DIR"/*.lst "$BOOT_STAGE/grub/i386-pc/" 2>/dev/null || true
echo "  modules: $(ls "$BOOT_STAGE/grub/i386-pc" | wc -l)"

# ─── [2/6] Build core.img ────────────────────────────────────────────
echo "[2/6] Building core.img"
CORE_IMG="${BUILD_D}/wraithos-core.img"
rm -f "$CORE_IMG"
"$GRUB_MKIMAGE" \
    -d "$GRUB_CORE_DIR" \
    -O i386-pc \
    -o "$CORE_IMG" \
    -p "(hd0,msdos1)/grub" \
    biosdisk part_msdos fat ext2 squashfs normal configfile linux
echo "  core.img: $(stat -c%s "$CORE_IMG") bytes"

# ─── [3/6] Rootfs slots A/B ─────────────────────────────────────────
echo "[3/6] Preparing rootfs slots"
cp "${BIN_DIR}/rootfs.squashfs" "${BIN_DIR}/rootfs-a.img"
truncate -s 128M "${BIN_DIR}/rootfs-a.img"
cp "${BIN_DIR}/rootfs-a.img" "${BIN_DIR}/rootfs-b.img"
echo "  slots: A+B 128M each"

# ─── [4/6] Run genimage ─────────────────────────────────────────────
echo "[4/6] Running genimage"
GEN_TMP="${BUILD_D}/genimage.tmp"
rm -rf "$GEN_TMP"

"$GENIMAGE" \
    --rootpath "$BOOT_STAGE" \
    --tmppath "$GEN_TMP" \
    --inputpath "${BIN_DIR}" \
    --outputpath "${BIN_DIR}" \
    --config "${BOARD_DIR}/genimage.cfg"

# ─── [5/6] Install GRUB to MBR ──────────────────────────────────────
echo "[5/6] Installing GRUB to MBR"
IMG="${BIN_DIR}/wraithos.img"
[ -f "$IMG" ] || { echo "[err] no wraithos.img"; exit 1; }

# Boot code (446 bytes, preserve partition table)
dd if="${GRUB_CORE_DIR}/boot.img" of="$IMG" bs=446 count=1 conv=notrunc status=none
echo "  MBR boot code: OK"

# core.img at sector 1
dd if="$CORE_IMG" of="$IMG" bs=512 seek=1 conv=notrunc status=none
echo "  core.img: installed"

# ─── [6/6] Cleanup ──────────────────────────────────────────────────
echo "[6/6] Cleanup"
rm -rf "$BOOT_STAGE" "$GEN_TMP" "$CORE_IMG"
rm -f "${BIN_DIR}/rootfs-a.img" "${BIN_DIR}/rootfs-b.img"

# ─── Done ───────────────────────────────────────────────────────────
echo ""
echo "======================================"
echo "IMAGE: $IMG"
ls -lh "$IMG"
echo ""
echo "Partitions:"
fdisk -l "$IMG" 2>/dev/null || true
echo "======================================"
