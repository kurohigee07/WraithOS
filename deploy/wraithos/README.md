# WraithOS build (x86_64 + aarch64)

Reproducible Buildroot pipeline untuk WraithOS appliance OS.

## Struktur

    wraithos/
      wraithos_x86_64_defconfig    Buildroot defconfig (x86_64, BIOS GRUB, musl)
      wraithos_aarch64_defconfig   Buildroot defconfig (ARM64, EFI GRUB, musl)
      x86_64/
        post-image.sh              bzImage + squashfs + dual-slot A/B
        genimage.cfg               4 partitions: boot.vfat + rootfs-a + rootfs-b + data
        grub.cfg                   GRUB menu slot A/B, marker /boot/boot_b
        kernel.fragment            eBPF, cgroups v2, watchdog, panic=5
        rootfs_overlay/            /sbin/wraith-init, /etc/init.d/S*, /root/.bash*, /etc/passwd
      aarch64/
        post-image-arm.sh          ARM64 variant
        post-build-arm.sh          ARM64 rootfs post-build hook
        genimage-arm.cfg           ARM64 partition layout (EFI + dual slot)
        grub-arm.cfg               ARM64 GRUB config

## Prasyarat

    Buildroot 2024.02.9 di ~/buildroot-2024.02.9
    BR2_EXTERNAL di ~/wraithos-br2-ext/ (x86_64) dan ~/wraithos-aarch64-br2-ext/ (ARM64)
    Paket host: build-essential, libncurses-dev, bison, flex, rsync, bc, cpio, python3
    Disk free: 8 GB minimum untuk build x86_64, 12 GB untuk ARM64

## Sync orchestrator binary (WAJIB sebelum build)

Binary orchestrator TIDAK di-commit ke git (24 MB per versi = repo bloat).
Sync dari orchestrator-project build output:

    cd ~/orchestrator-project
    cargo zigbuild --locked --release --target x86_64-unknown-linux-musl -p orchestrator
    bash deploy/wraithos/sync-wraith-init.sh

Script ini copy binary ke:
    ~/wraithos-br2-ext/board/wraithos/rootfs_overlay/sbin/wraith-init         (x86_64)
    ~/wraithos-aarch64-br2-ext/board/wraithos/rootfs_overlay/sbin/wraith-init (aarch64, jika ada)

Setiap rebuild orchestrator (patch, config fix, dll.) — jalankan sync sebelum build WraithOS.

## Build x86_64

    cd ~/buildroot-2024.02.9
    cp ~/orchestrator-project/deploy/wraithos/wraithos_x86_64_defconfig \
       configs/wraithos_x86_64_defconfig

    make BR2_EXTERNAL=$HOME/wraithos-br2-ext \
         defconfig BR2_DEFCONFIG=$PWD/configs/wraithos_x86_64_defconfig

    make -j8 2>&1 | tee /tmp/wraithos-build.log

    Output: output/images/wraithos.img (~577 MB)

## Boot

    cd output/images
    qemu-system-x86_64 \
        -machine q35 -enable-kvm -cpu host -m 1024 -smp 2 \
        -drive file=wraithos.img,format=raw,if=virtio,cache=writeback \
        -nographic -no-reboot

    Login: root (no password)

## A/B update

    # switch to slot B
    touch /boot/boot_b && sync && reboot

    # rollback
    rm /boot/boot_b && sync && reboot

## Catatan

    - bash butuh BR2_PACKAGE_BUSYBOX_SHOW_OTHERS=y (sudah ada di defconfig)
    - tanpanya, bash silently di-reject oleh Buildroot, login loop karena
      /etc/passwd root shell = /bin/bash tapi bash tidak ada di rootfs
    - GRUB symbol yang benar untuk Buildroot 2024.02:
      BR2_TARGET_GRUB2_I386_PC=y (bukan _X86_64_BIOS, deprecated)
    - Kernel fragment harus x86-compatible (CONFIG_WATCHDOG, CONFIG_BPF, dst.)
      — jangan pakai fragment ARM ke build x86

## Before deployment

WraithOS ships with a documented root password. Read
[Security](../../docs/security.md#default-credentials) for the full
policy. At minimum:

- [ ] Change the root hash in `{x86_64,aarch64}/rootfs_overlay/etc/shadow`
      before building the production image
      (`openssl passwd -6 '<new-password>'`)
- [ ] Set `trusted_signer_keys` in `/etc/wraith/orchestrator.toml` and
      sign every module in `modules/{tenant}/` with a matching
      `.wasm.sig`
- [ ] Set `require_auth = true` and configure at least one tenant
- [ ] Confirm `metrics_bind` is `127.0.0.1:9090` (the default) or
      behind a reverse proxy
- [ ] Boot the image with serial console, verify `wraithos login:`
      accepts the new password, verify `/healthz`, `/readyz`,
      `/ui`
- [ ] Exercise `wraith-update` and A/B rollback on a scratch device

The image is a development default. Production deployments must not
ship the default credential.
