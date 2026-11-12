#!/bin/bash
# WraithOS x86_64 QEMU launcher.
# -no-reboot + restart loop: guest `reboot` = QEMU exit = wrapper restart.
# Avoids QEMU TCG soft-reset tty corruption, user sees normal reboot.
# SSH backup: host port 2222 -> guest 22.
set -u
IMG="${1:-wraithos.img}"
[ -f "$IMG" ] || { echo "Image not found: $IMG"; exit 1; }
IMG_DIR="$(cd "$(dirname "$IMG")" && pwd)"
IMG_NAME="$(basename "$IMG")"

while true; do
    echo ""
    echo "═══════════════════════════════════════════════════════"
    echo " WraithOS x86_64  —  $IMG_NAME"
    echo " Serial:  root / wraithos-admin-change-me (primary)"
    echo " SSH:     ssh -p 2222 root@127.0.0.1 (host-forwarded, backup)"
    echo " Reboot:  guest reboot = QEMU restart (auto)"
    echo " Exit:    Ctrl-C  (stops wrapper)"
    echo "═══════════════════════════════════════════════════════"

    if [ -w /dev/kvm ]; then
        qemu-system-x86_64 -machine q35 -enable-kvm -cpu host \
            -m 768 -smp 1 \
            -drive file="$IMG_DIR/$IMG_NAME",format=raw,if=virtio,cache=writeback \
            -netdev user,id=net0,hostfwd=tcp::2222-:22 \
            -device virtio-net-pci,netdev=net0 \
            -nographic -no-reboot
    else
        qemu-system-x86_64 -machine q35 -accel tcg,thread=multi -cpu max \
            -m 512 -smp 1 \
            -drive file="$IMG_DIR/$IMG_NAME",format=raw,if=virtio,cache=writeback \
            -netdev user,id=net0,hostfwd=tcp::2222-:22 \
            -device virtio-net-pci,netdev=net0 \
            -nographic -no-reboot
    fi

    EXIT=$?
    echo ""
    echo "════ QEMU exit: $EXIT — restart 2s (guest reboot / shutdown) ════"
    sleep 2
done
