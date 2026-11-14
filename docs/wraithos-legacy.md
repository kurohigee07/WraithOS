# WraithOS legacy build sources (superseded)

Moved to deploy/wraithos/. Preserved for historical context.

---

# WraithOS Buildroot sources

This directory contains separate x86_64 and AArch64 external trees. They are not interchangeable: boot firmware, partition sizes, init configuration, and recovery scripts differ.

Start with the detailed, source-guided [WraithOS guide](../docs/wraithos.md). It explains the current caveats, persistent mounts, ARM trial/commit behavior, and destructive wizard/update paths.

## Structure

- `br2-ext/board/wraithos/` — x86 overlay, kernel fragment, BIOS GRUB/genimage references, and systemd files.
- `aarch64-br2-ext/board/wraithos/` — ARM overlay, EFI GRUB/genimage references, boot-state/watchdog scripts, and a tracked AArch64 host binary.
- `buildroot-configs/wraithos_x86_64_defconfig` — x86 defconfig; review its systemd/BusyBox and post-image wiring mismatches before building.
- `buildroot-configs/wraithos_aarch64_defconfig` — ARM defconfig with hardcoded external-tree paths that must be adapted.
- `buildroot-configs/wraithos_x86_64_final.config` — **only a mirror directive**, not a complete final Buildroot configuration.
- `deploy/wraithos/termux-package/` — phone-oriented config and helper scripts; supply the binary separately.

## Historical x86 rebuild outline

This is a reference recipe, not a verified one-command pipeline. Resolve the configuration caveats in the guide first.

```bash
# Obtain and verify Buildroot 2024.02.9 through a trusted source, then enter it.
cp /path/to/Orchestrator-/wraithos/buildroot-configs/wraithos_x86_64_defconfig configs/
cp -a /path/to/Orchestrator-/wraithos/br2-ext "$HOME/wraithos-br2-ext"

# From the orchestrator repository, with Rust/Zig and the musl target installed:
cargo zigbuild --locked --release --target x86_64-unknown-linux-musl -p orchestrator
cp target/x86_64-unknown-linux-musl/release/orchestrator \
  "$HOME/wraithos-br2-ext/board/wraithos/rootfs_overlay/sbin/wraith-init"
chmod +x "$HOME/wraithos-br2-ext/board/wraithos/rootfs_overlay/sbin/wraith-init"

# Back in Buildroot, after adapting hardcoded paths and reviewing init/post-image wiring:
export BR2_EXTERNAL="$HOME/wraithos-br2-ext"
make O=output wraithos_x86_64_defconfig
make O=output -j"$(nproc)"
```

Expected artifacts depend on the final configuration and post-image pipeline. `bzImage` and `rootfs.squashfs` are not the same thing as a complete partitioned `wraithos.img`.

## Historical build notes

- Oracle Linux repository timeouts were worked around with an AlmaLinux mirror.
- Legacy Buildroot options required `BR2_SKIP_LEGACY=y` or option renames.
- Slow downloads were redirected through `BR2_PRIMARY_SITE="https://mirrors.kernel.org/sources/"`.

These are historical workarounds, not guarantees for a fresh build. Review credentials, hardcoded paths, generated GRUB environment format, and the actual init system before distributing an image.
