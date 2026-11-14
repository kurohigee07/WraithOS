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
export PATH="${HOST_D}/bin:${PATH}"

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
# ─── [1.5/6] UEFI Secure Boot signing (conditional) ─────────────────
if [ -n "${WRAITH_SB_KEYS:-}" ]; then
    echo "[1.5/6] UEFI Secure Boot signing enabled (keys: $WRAITH_SB_KEYS)"
    [ -f "$WRAITH_SB_KEYS/db.key" ] || { echo "[err] missing db.key"; exit 1; }
    [ -f "$WRAITH_SB_KEYS/db.crt" ] || { echo "[err] missing db.crt"; exit 1; }
    [ -n "${WRAITH_SB_KEYPASS:-}" ] || { echo "[err] WRAITH_SB_KEYPASS not set"; exit 1; }
    command -v sbsign >/dev/null 2>&1 || { echo "[err] sbsign not in PATH"; exit 1; }
    command -v sbverify >/dev/null 2>&1 || { echo "[err] sbverify not in PATH"; exit 1; }
    TMPKEY=$(mktemp /tmp/sb-db-XXXXXX.pem); chmod 600 "$TMPKEY"
    trap "shred -u $TMPKEY 2>/dev/null || rm -f $TMPKEY" EXIT
    openssl rsa -in "$WRAITH_SB_KEYS/db.key" -passin "env:WRAITH_SB_KEYPASS" -out "$TMPKEY" 2>/dev/null || { echo "[err] db.key decrypt"; exit 1; }
    if sbverify --cert "$WRAITH_SB_KEYS/db.crt" "$BOOT_STAGE/bzImage" >/dev/null 2>&1; then
        echo "  bzImage already signed (input was signed), skipping re-sign"
    else
        sbsign --key "$TMPKEY" --cert "$WRAITH_SB_KEYS/db.crt" --output "$BOOT_STAGE/bzImage.signed" "$BOOT_STAGE/bzImage" || exit 1
        mv "$BOOT_STAGE/bzImage.signed" "$BOOT_STAGE/bzImage"
        sbverify --cert "$WRAITH_SB_KEYS/db.crt" "$BOOT_STAGE/bzImage" || exit 1
    fi
    echo "  bzImage signed: $(stat -c%s "$BOOT_STAGE/bzImage") bytes"
    cp "$BOOT_STAGE/bzImage" "${BIN_DIR}/bzImage"
    echo "  staged signed bzImage → BIN_DIR ($(stat -c%s "${BIN_DIR}/bzImage") bytes)"
    EFI_GRUB_CORE="${BUILD_D}/grub2-2.12/build-x86_64-efi/grub-core"
    [ -d "$EFI_GRUB_CORE" ] || { echo "[err] no x86_64-efi GRUB build tree"; exit 1; }
    mkdir -p "$BOOT_STAGE/EFI/BOOT"
    "$GRUB_MKIMAGE" -d "$EFI_GRUB_CORE" -O x86_64-efi -o "$BOOT_STAGE/EFI/BOOT/BOOTX64.EFI.unsigned" -p "/EFI/BOOT" boot linux ext2 fat squash4 part_msdos part_gpt normal efi_gop efi_uga all_video search test loadenv configfile serial terminal echo || exit 1
    sbsign --key "$TMPKEY" --cert "$WRAITH_SB_KEYS/db.crt" --output "$BOOT_STAGE/EFI/BOOT/BOOTX64.EFI" "$BOOT_STAGE/EFI/BOOT/BOOTX64.EFI.unsigned" || exit 1
    rm -f "$BOOT_STAGE/EFI/BOOT/BOOTX64.EFI.unsigned"
    sbverify --cert "$WRAITH_SB_KEYS/db.crt" "$BOOT_STAGE/EFI/BOOT/BOOTX64.EFI" || exit 1
    echo "  grubx64.efi signed: $(stat -c%s "$BOOT_STAGE/EFI/BOOT/BOOTX64.EFI") bytes"
    printf "search --file --set=root /grub/grub.cfg\nconfigfile /grub/grub.cfg\n" > "$BOOT_STAGE/EFI/BOOT/grub.cfg"
    echo "  EFI grub.cfg staged"
    shred -u "$TMPKEY" 2>/dev/null || rm -f "$TMPKEY"; trap - EXIT
else
    echo "[1.5/6] UEFI Secure Boot signing DISABLED (WRAITH_SB_KEYS not set)"
    echo "*** WARNING: image will NOT include EFI/UEFI Secure Boot chain ***"
    echo "*** Production release images MUST set WRAITH_SB_KEYS=</path/to/keys> ***"
    echo "*** to sign GRUB + bzImage and enable UEFI Secure Boot. ***"
    echo "*** Reference: deploy/wraithos/uefi/gen-sb-keys.sh ***"
fi
cp "${BOARD_DIR}/grub.cfg" "$BOOT_STAGE/grub/grub.cfg"

# grubenv: 1024-byte GRUB env block, slot=A starts
GRUBENV="$BOOT_STAGE/grub/grubenv"
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
} > "$GRUBENV"
N=$((1024 - $(wc -c < "$GRUBENV")))
printf '%*s' "$N" '' | tr ' ' '#' >> "$GRUBENV"
chmod 0644 "$GRUBENV"
echo "  grubenv: $(stat -c%s "$GRUBENV") bytes"

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
    biosdisk part_msdos fat normal configfile linux test loadenv extcmd search
echo "  core.img: $(stat -c%s "$CORE_IMG") bytes"

# ─── [3/6] Rootfs slots (direct reference, no cp) ──────────────────
echo "[3/6] Using rootfs.squashfs directly (genimage pads to 128M)"

# ─── [3.5/6] Stage grub/ into inputpath (genimage resolve files from here)
echo "[3.5/6] Staging grub/ to inputpath"
rm -rf "${BIN_DIR}/grub"
cp -r "${BOOT_STAGE}/grub" "${BIN_DIR}/grub"
echo "  staged: $(ls "${BIN_DIR}/grub" | tr '\n' ' ')"

if [ -n "${WRAITH_SB_KEYS:-}" ] && [ -d "${BOOT_STAGE}/EFI" ]; then
    rm -rf "${BIN_DIR}/EFI"
    cp -r "${BOOT_STAGE}/EFI" "${BIN_DIR}/EFI"
    echo "  staged EFI: $(ls -R "${BIN_DIR}/EFI" | head -5 | tr '\n' ' ')"
fi

# ─── [4/6] Run genimage ─────────────────────────────────────────────
echo "[4/6] Running genimage"
GEN_TMP="${BUILD_D}/genimage.tmp"
rm -rf "$GEN_TMP"

if [ -n "${WRAITH_SB_KEYS:-}" ]; then
    GENIMAGE_CFG="${BOARD_DIR}/genimage-efi.cfg"
else
    GENIMAGE_CFG="${BOARD_DIR}/genimage.cfg"
fi
echo "  genimage config: $(basename "$GENIMAGE_CFG")"

"$GENIMAGE" \
    --rootpath "$BOOT_STAGE" \
    --tmppath "$GEN_TMP" \
    --inputpath "${BIN_DIR}" \
    --outputpath "${BIN_DIR}" \
    --config "$GENIMAGE_CFG"

# genimage's DOS partition table stops writing after the last partition
# with an image source. p4 (data) is raw, so the file would end at p3
# content and the kernel would reject p4 with "partition outside the
# disk". Pad to 640 MiB so the whole partition table fits.
IMG_TMP="${BIN_DIR}/wraithos.img"
if [ -f "$IMG_TMP" ]; then
    truncate -s 640M "$IMG_TMP"
    echo "  padded: $(stat -c%s "$IMG_TMP") bytes"
fi

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
rm -rf "$BOOT_STAGE" "$GEN_TMP" "$CORE_IMG" "${BIN_DIR}/grub" "${BIN_DIR}/EFI"

# ─── Done ───────────────────────────────────────────────────────────
echo ""
echo "======================================"
echo "IMAGE: $IMG"
ls -lh "$IMG"
echo ""
echo "Partitions:"
fdisk -l "$IMG" 2>/dev/null || true
echo "======================================"
