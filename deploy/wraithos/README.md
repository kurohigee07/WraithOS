# WraithOS build (x86_64 + aarch64)

Buildroot pipeline for the WraithOS appliance image. Produces a
flashable disk image per arch, plus RAUC bundles for A/B updates.

## Layout

- `wraithos_{x86_64,aarch64}_defconfig` — Buildroot defconfigs
- `x86_64/` — BIOS GRUB, dual-slot A/B, kernel fragment, rootfs overlay
- `aarch64/` — ARM64 UEFI GRUB, dual-slot A/B, kernel fragment, rootfs overlay
- `uefi/` — Secure Boot key generation (`gen-sb-keys.sh`, device-specific)
- `grub-patches/` — local GRUB 2.12 patches for shim-less Secure Boot
- `make-bundle.sh` — RAUC bundle builder (production CA required)
- `sync-wraith-init.sh` — copies the orchestrator binary into the overlay

## Prerequisites

- Buildroot 2024.02.9 at `~/buildroot-2024.02.9`
- External trees: `~/wraithos-br2-ext/` (x86_64), `~/wraithos-aarch64-br2-ext/` (aarch64)
- Host tools: `build-essential`, `libncurses-dev`, `bison`, `flex`, `rsync`, `bc`, `cpio`, `python3`, `openssl`. For Secure Boot signing: `sbsigntools`.
- Disk: 8 GB for x86_64, 12 GB for aarch64 (build cache)

## Sync the orchestrator binary (required)

The orchestrator binary is not committed (20-26 MB per arch). Build
and sync into the overlay before each image build:

    cd ~/orchestrator-project
    cargo build --release --target=x86_64-unknown-linux-musl -p orchestrator
    cargo build --release --target=aarch64-unknown-linux-musl -p orchestrator
    bash deploy/wraithos/sync-wraith-init.sh


## Build x86_64 (BIOS-only)

    cd ~/buildroot-2024.02.9
    cp ~/orchestrator-project/deploy/wraithos/wraithos_x86_64_defconfig \
       configs/wraithos_x86_64_defconfig

    make BR2_EXTERNAL=$HOME/wraithos-br2-ext \
         defconfig BR2_DEFCONFIG=$PWD/configs/wraithos_x86_64_defconfig
    make -j$(nproc) 2>&1 | tee /tmp/wraithos-x86.log

    Output: output/images/wraithos.img (~640 MB)

## Build x86_64 with UEFI Secure Boot

Signing is opt-in via `WRAITH_SB_KEYS`. See
`docs/wraithos.md` → "Building a UEFI Secure Boot x86_64 image"
for the full runbook (key generation, OVMF enrollment for testing,
production rollout).

    export WRAITH_SB_KEYS="$HOME/sb-keys"
    export WRAITH_SB_KEYPASS='<your-password>'
    make -j$(nproc) 2>&1 | tee /tmp/wraithos-x86-sb.log

Without `WRAITH_SB_KEYS`, `post-image.sh` builds the BIOS-only image
and prints a loud warning. v0.51+ ship BIOS-only pending the v0.53
per-device SB key rollout; see `CHANGELOG.md` for the current
Secure Boot state.

## Build aarch64

    cd ~/buildroot-2024.02.9
    cp ~/orchestrator-project/deploy/wraithos/wraithos_aarch64_defconfig \
       configs/wraithos_aarch64_defconfig

    make O=output-arm-m31 BR2_EXTERNAL=$HOME/wraithos-aarch64-br2-ext \
         defconfig BR2_DEFCONFIG=$PWD/configs/wraithos_aarch64_defconfig
    make O=output-arm-m31 -j$(nproc) 2>&1 | tee /tmp/wraithos-arm.log

    Output: output-arm-m31/images/wraithos-arm.img (~458 MB)


## Boot in QEMU

x86_64, BIOS path:

    qemu-system-x86_64 -machine pc -cpu max -m 1024 -smp 2 \
      -enable-kvm \
      -drive file=output/images/wraithos.img,format=raw,if=virtio \
      -netdev user,id=net0,hostfwd=tcp::2223-:22 \
      -device virtio-net-pci,netdev=net0 \
      -serial mon:stdio -no-reboot

aarch64, UEFI path:

    qemu-system-aarch64 -machine virt -cpu cortex-a72 -m 1024 -smp 2 \
      -bios /usr/share/qemu-efi-aarch64/QEMU_EFI.fd \
      -drive file=output-arm-m31/images/wraithos-arm.img,format=raw,if=virtio \
      -netdev user,id=net0,hostfwd=tcp::2223-:22 \
      -device virtio-net-pci,netdev=net0 \
      -serial mon:stdio -no-reboot

`-cpu max` is required for x86_64. With `-cpu qemu64` (QEMU default),
the guest kernel panics on init because BusyBox uses SSE that qemu64
does not expose. This is a QEMU gotcha, not an image bug.

On first boot, the wizard listens on `http://127.0.0.1:9090/wizard`.
Serial console login is `root` with the documented default password
(see "Before deployment" below). SSH is closed until the wizard
completes (v0.51+).

## A/B update (RAUC)

Production updates use signed RAUC bundles. Build with the production
CA (see `make-bundle.sh`):

    ./make-bundle.sh x86 v0.52.0-security-hardening
    ./make-bundle.sh arm v0.52.0-security-hardening

Install on a running device:

    wraith-install-bundle /path/to/x86-v0.52.0-security-hardening.raucb

The RAUC health gate polls `/readyz` for up to 60 seconds after the
trial boot. On success, the slot is committed. On timeout, `mark-bad`
+ reboot falls through to the last-known-good slot. Details in
`docs/wraithos.md` → "Health-gated rollback".

Legacy x86 rollback via `/boot/boot_b` is documented in
`docs/wraithos.md` as historical; the current image uses grubenv.

## Before deployment

The image ships with a documented root password. Read
[Security](../../docs/security.md#default-credentials) for policy.
At minimum:

- [ ] Change the root hash in `{x86_64,aarch64}/rootfs_overlay/etc/shadow`
      before building the production image
      (`openssl passwd -6 '<new-password>'`)
- [ ] Set `trusted_signer_keys` in `/etc/wraith/orchestrator.toml` and
      sign every module with a matching `.wasm.sig`
- [ ] Set `require_auth = true` and configure at least one tenant
- [ ] Confirm `metrics_bind` is `127.0.0.1:9090` (the default) or
      behind a reverse proxy
- [ ] Boot the image, verify `wraithos login:` accepts the new
      password, verify `/healthz`, `/readyz`, `/ui`
- [ ] If deploying with Secure Boot: generate device-specific keys
      (`uefi/gen-sb-keys.sh`), sign the image, enroll the cert,
      verify with `sbverify`. See `docs/wraithos.md` →
      "Building a UEFI Secure Boot x86_64 image"
- [ ] Exercise `wraith-install-bundle` and A/B rollback on a scratch
      device

The image is a development default. Production deployments must not
ship the default credential.
