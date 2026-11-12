#!/bin/bash
# WraithOS aarch64 QEMU launcher (docker + qemu-system-aarch64).
# -no-reboot + restart loop: guest `reboot` = QEMU exit = wrapper restart.
# SSH backup: host port 2223 -> guest 22.
set -u
IMG="${1:-wraithos-arm.img}"
[ -f "$IMG" ] || { echo "Image not found: $IMG"; exit 1; }
IMG_DIR="$(cd "$(dirname "$IMG")" && pwd)"
IMG_NAME="$(basename "$IMG")"

while true; do
    echo ""
    echo "═══════════════════════════════════════════════════════"
    echo " WraithOS aarch64  —  $IMG_NAME"
    echo " Serial:  root / wraithos-admin-change-me (primary)"
    echo " SSH:     ssh -p 2223 root@127.0.0.1 (host-forwarded, backup)"
    echo " Reboot:  guest reboot = QEMU restart (auto)"
    echo " Exit:    Ctrl-C  (stops wrapper)"
    echo "═══════════════════════════════════════════════════════"

    sudo docker run -it --rm --name wraithos-arm-qemu --network host \
        -v "$IMG_DIR:/work:Z" -w /work --security-opt label=disable \
        ubuntu:24.04 \
        bash -c "apt-get -o Acquire::ForceIPv4=true update -qq >/dev/null 2>&1 && \
                 DEBIAN_FRONTEND=noninteractive apt-get -o Acquire::ForceIPv4=true install -y -qq qemu-system-arm qemu-efi-aarch64 >/dev/null 2>&1 && \
                 exec qemu-system-aarch64 -machine virt -cpu cortex-a72 -m 1024 -smp 2 \
                     -bios /usr/share/qemu-efi-aarch64/QEMU_EFI.fd \
                     -drive file=/work/$IMG_NAME,format=raw,if=virtio \
                     -netdev user,id=net0,hostfwd=tcp::2223-:22 \
                     -device virtio-net-pci,netdev=net0 \
                     -serial mon:stdio -display none -no-reboot"

    EXIT=$?
    echo ""
    echo "════ QEMU exit: $EXIT — restart 2s (guest reboot / shutdown) ════"
    sleep 2
done
