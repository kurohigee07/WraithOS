#!/bin/bash
set -e
IMG="${1:-wraithos.img}"
[ -f "$IMG" ] || { echo "Image not found: $IMG"; exit 1; }
if [ -w /dev/kvm ]; then
    echo "✓ KVM — boot cepat"
    exec qemu-system-x86_64 -machine q35 -enable-kvm -cpu host \
        -m 768 -smp 1 -drive file="$IMG",format=raw,if=virtio,cache=writeback \
        -nographic -no-reboot
else
    echo "⚠ No KVM — TCG (5-10 menit boot)"
    exec qemu-system-x86_64 -machine q35 -accel tcg,thread=multi -cpu max \
        -m 512 -smp 1 -drive file="$IMG",format=raw,if=virtio,cache=writeback \
        -nographic -no-reboot
fi
