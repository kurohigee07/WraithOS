# WraithOS Buildroot Sources

This directory contains the Buildroot configuration for WraithOS appliance image.

## Structure

- `br2-ext/board/wraithos/` — BR2_EXTERNAL overlay (kernel.fragment, rootfs_overlay/, systemd unit)
- `buildroot-configs/wraithos_x86_64_defconfig` — Buildroot defconfig
- `buildroot-configs/wraithos_x86_64_final.config` — Final .config (after legacy fix)

## How to rebuild from scratch

    # 1. Download Buildroot 2024.02.9
    wget https://buildroot.org/downloads/buildroot-2024.02.9.tar.gz
    tar -xzf buildroot-2024.02.9.tar.gz
    cd buildroot-2024.02.9

    # 2. Copy defconfig
    cp ~/orchestrator-project/wraithos/buildroot-configs/wraithos_x86_64_defconfig \
       configs/

    # 3. Setup BR2_EXTERNAL
    cp -a ~/orchestrator-project/wraithos/br2-ext ~/wraithos-br2-ext

    # 4. Copy orchestrator binary to overlay
    cargo zigbuild --release --target x86_64-unknown-linux-musl
    cp target/x86_64-unknown-linux-musl/release/orchestrator \
       ~/wraithos-br2-ext/board/wraithos/rootfs_overlay/sbin/wraith-init
    chmod +x ~/wraithos-br2-ext/board/wraithos/rootfs_overlay/sbin/wraith-init

    # 5. Build
    export BR2_EXTERNAL=$HOME/wraithos-br2-ext
    make O=output wraithos_x86_64_defconfig
    make O=output -j$(nproc)

    # 6. Output: output/images/bzImage + rootfs.squashfs

## Gotchas

- Oracle Linux repos timeout: pakai AlmaLinux mirror (lihat commit history)
- Buildroot legacy config: BR2_SKIP_LEGACY=y sudah di-set
- Download mirror lemot: BR2_PRIMARY_SITE="https://mirrors.kernel.org/sources/"
