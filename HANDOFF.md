# Wraith-V2 + WraithOS — Historical Handoff

> **Archival document, translated into English.** This file preserves development notes through 2026-11-04. Version labels, completion claims, measurements, and deployment details below are historical reports, not independently verified current status. For the source-guided description of this checkout, start with [README.md](README.md), [docs/architecture.md](docs/architecture.md), and [docs/source-map.md](docs/source-map.md). The original mixed-language file remains available at [baseline commit 20cb378](https://github.com/kurohigee07/Orchestrator-/blob/20cb3785d1f17e4b05759b539ffa016eaea50324/HANDOFF.md).

**Last historical update:** 2026-11-04
**Repository:** https://github.com/kurohigee07/Orchestrator-
**Initial document version:** v0.17.1 (WebSocket, dashboard, audit logs, SDK/CLI)
**Initial WraithOS status:** Buildroot compilation in progress

---

## 1. Project overview

**Wraith-V2** was described as a unified Rust static-musl core for:

- Wasm plugins using the component model and `wasm32-wasip2`.
- Embedded, event-driven Rhai scripting.
- An HTTP API on port 9090.
- Multi-tenant namespaces with Argon2 authentication, quotas, and audit logs.
- Prometheus metrics, a WebSocket event stream, and an HTMX dashboard.

**WraithOS** was described as a Buildroot appliance booting into Wraith-V2:

- Immutable SquashFS root filesystem; the initial notes also mentioned overlayfs.
- A/B atomic updates through RAUC, planned for M27. Later notes describe a custom updater instead.
- Kernel options for eBPF, AF_PACKET, cgroups v2, `panic=5`, and watchdog behavior.
- tmpfs for modules and SQLite in the early MVP. Later integration notes describe persistent `/data` bind mounts.

These statements describe successive historical designs. Do not combine them into one current architecture without checking source.

## 2. Reported milestone history

| Version | Reported feature set |
|---|---|
| v0.1.0-mvp | M1–M7: static musl binary, Wasm bridge, Rhai worker, hardening |
| v0.2.0 | M8–M9: TOML config, CLI, metrics, systemd |
| v0.3.0 | M10-A: bounded Wasm concurrency through a semaphore |
| v0.4.0 | M10-B: per-instance 256 MB memory limit |
| v0.5.0 | M11–M13: tracing, `/debug/state`, install/smoke scripts |
| v0.6.0 | M14: Ed25519 signature verification |
| v0.7.0 | M15: upload/replace/delete/execute lifecycle API |
| v0.8.0 | M16: per-module memory/concurrency/rate quotas |
| v0.9.0 | M17: tenant namespaces and Argon2 authentication |
| v0.10.0 | M18: per-tenant resource caps |
| v0.11.0 | M19: source-build pipeline |
| v0.12.0 | M20: OCI Object Storage backup through rclone and a daily timer |
| v0.13.0 | M21: Rust SDK and `wraith` CLI |
| v0.14.0 | M22: Wasm → Rhai → Rust EventBus |
| v0.15.0 | M23: SQLite `execution_logs` audit table |
| v0.16.0 | M24: embedded HTML/HTMX dashboard at `/ui` |
| v0.17.0 | M25: WebSocket live events at `/ws/events` |
| v0.17.1 | M25: WebSocket test helper |

## 3. Historical deployment details

**Oracle VPS:**

- Hostname: `instance-20261018-1417`.
- OS: Oracle Linux 9, x86_64.
- Reported resources: 48 GB RAM, 8 CPUs.
- User: `opc`.
- Project directory: `~/orchestrator-project`.

**Service:**

- User unit: `~/.config/systemd/user/orchestrator.service`.
- Binary: `~/orchestrator-project/target/x86_64-unknown-linux-musl/release/orchestrator`.
- Config: `~/orchestrator-project/orchestrator.toml`.
- Log: `~/orchestrator-project/logs/orchestrator.log`.
- Bind: `127.0.0.1:9090`.

**Backup:**

- Remote: `oci:wraith-bucket/daily`.
- rclone config: `~/.config/rclone/rclone.conf`, using the native Oracle Object Storage provider.
- OCI config: `~/.oci/config`.
- Script: `deploy/backup.sh`.
- Timer: `orchestrator-backup.timer`, daily at 03:00.
- Retention: seven newest generations.

**CLI:** binary `~/.cargo/bin/wraith`; commands `list`, `deploy`, `exec`, `delete`, `logs`; source in `wraith-cli/` and `wraith-sdk/`.

These are environment-specific historical paths, not credentials or a universal installation recipe.

## 4. Initial WraithOS build notes

- Build directory: `~/buildroot-2024.02.9`.
- External tree: `~/wraithos-br2-ext`.
- Defconfig: `~/buildroot-2024.02.9/configs/wraithos_x86_64_defconfig`.
- Log: `/tmp/wraithos-build.log`.
- tmux session: `wraith-build`.

Historical build command:

```sh
cd ~/buildroot-2024.02.9
make O=output -j8 2>&1 | tee -a /tmp/wraithos-build.log
```

Expected artifacts after completion:

```text
~/buildroot-2024.02.9/output/images/
  bzImage
  rootfs.squashfs
  wraithos.img   # only when the post-image pipeline is wired correctly
```

The external overlay was expected to contain the kernel fragment, host binary, fstab, runtime config, and systemd unit/wants symlink. The current x86 defconfig selects systemd while the overlay also contains BusyBox-style init files; see [the current WraithOS guide](docs/wraithos.md) before rebuilding.

Historical build obstacles and workarounds:

1. Oracle Linux repositories timed out; dependencies were installed through an AlmaLinux mirror or by disabling the failing repositories.
2. Legacy Buildroot options required `BR2_SKIP_LEGACY=y` or renamed options such as `BR2_PACKAGE_LIBCURL_CURL`.
3. Slow Savannah/MPFR downloads were redirected through `BR2_PRIMARY_SITE="https://mirrors.kernel.org/sources/"`.
4. Interrupted builds were resumed using Buildroot stamp files. A resume is not proof that every generated artifact is consistent.
5. Missing `gcc-c++`, `patch`, Perl, and ncurses development packages were installed through a temporary AlmaLinux repository.

## 5. Historical high-level architecture

The binary was described as combining Axum, Wasmtime components, Rhai on `spawn_blocking`, Tokio broadcast events, SQLx SQLite, and rustls/ring without OpenSSL.

Reported limits:

- 60-second epoch deadline.
- 256 MB Wasm memory per instance.
- Two global Wasm permits.
- Module and tenant memory/concurrency/rate controls.
- 256 global socket handles.
- One source build at a time with a 300-second compiler timeout.

Tenant notes described `(tenant, id)` registry keys, Bearer tokens verified against Argon2 PHC hashes or development plaintext, and `modules/{tenant}/*.wasm` storage. The notes claimed cross-tenant execution returned 404 to avoid an existence leak. Current authentication and observability are broader than namespace isolation; read [the security model](docs/security.md).

## 6. Important historical configuration

```toml
db_path = "orchestrator.db"
modules_dir = "./modules"
max_concurrent_wasm = 2
wasm_memory_limit_mb = 256
metrics_bind = "127.0.0.1:9090"
default_tenant = "default"
```

The kernel fragment was reported to enable packet sockets, BPF syscall/JIT, cgroups/BPF, watchdog nowayout, five-second panic reboot, panic on oops, SquashFS/LZ4, and overlayfs, while disabling DRM, framebuffer, sound, WLAN, and Bluetooth. Verify the final kernel config rather than assuming a fragment alone applied every option.

## 7. Historical roadmap

M1–M25 were reported complete. M26, WraithOS Level 2, had base config, kernel fragment, and overlay work marked complete, with compilation, QEMU boot, and six verification checkpoints still pending at the time of the initial note.

Planned Level 3 work included RAUC A/B updates, a first-boot wizard originally proposed on port 8080, LUKS, AArch64, and signed releases. Later section entries supersede parts of this plan; the current wizard is served by the main listener at `/wizard`.

## 8. Original resume instructions

The original note asked a new assistant chat to read this handoff, treat v0.17.1 as the latest milestone, inspect the `wraith-build` tmux session, and continue with QEMU testing after compilation. That environment is not available in the documentation workspace. Use the current source and guides rather than assuming the historical session still exists.

## 9. Original WraithOS verification checklist

The proposed six checks were:

1. `systemctl status orchestrator.service` shows active/running.
2. `curl http://127.0.0.1:9090/healthz` returns `ok`.
3. `/modules` and `/var/lib/orchestrator` are tmpfs mounts.
4. `/proc/cmdline` contains `panic=5`.
5. `/sys/fs/cgroup/cgroup.controllers` exists.
6. The BPF filesystem is mounted or a BPF feature probe succeeds.

Later notes report BusyBox rather than systemd and persistent data rather than tmpfs. Adapt checks to the actual image; do not require obsolete mount/init behavior blindly.

## 10. Ownership

- Owner: `kurohigee07` on GitHub.
- The initial note described the repository as private by default; current visibility must be checked on GitHub.
- Backups were reported across OCI, GitHub, and local storage.

---

## WraithOS Level 2 MVP — reported verified, 2026-11-02

### Reported artifacts

- Linux 6.6 `bzImage`: 11 MB.
- Immutable `rootfs.squashfs`: 7.1 MB.
- Static-musl `wraith-init`: 23 MB, embedded in the rootfs.
- The note called the image “18 MB total.” That is a historical headline, not the later full partitioned disk size.

### Reported QEMU/KVM checks

- Kernel boot in 5–8 seconds.
- `/bin/sh` used as a temporary init bypass.
- `/sbin/wraith-init` and `/etc/wraith/orchestrator.toml` present.
- tmpfs mounted at `/modules` (256M) and `/var/lib/orchestrator` (512M).
- `panic=5`, cgroups v2, and BPF filesystem observed.
- Loopback ping reported at 0.036 ms.
- Orchestrator PID 83 running; `/healthz` returned HTTP 200 `ok`.
- SQLite running in tmpfs.
- Rhai smoke event reported `delivered=2`.

### Reported MVP issues

1. BusyBox init stalled in S40network waiting for `eth0`; the workaround was manual loopback setup and host startup from a shell.
2. The systemd service did not auto-start under BusyBox; an init.d script was proposed.
3. BusyBox wget required loopback to be up.

Next steps included init/network fixes, a GRUB disk image, RAUC, LUKS, AArch64, and a wizard.

## WraithOS M28 A/B update — reported verified, 2026-11-02 14:09 UTC

The note reported:

- Slot A boot from `/dev/vda2`, healthy orchestrator.
- A → B update through `dd` to `/dev/vda3`, a `boot_b` marker, and reboot.
- Slot B boot from `/dev/vda3`, healthy orchestrator.
- B → A rollback by removing `/boot/boot_b` and rebooting.
- Bidirectional QEMU/KVM verification.

The described layout was a 64M FAT32 boot partition, two 128M SquashFS roots, and a 256M ext4 data partition. Selection used a custom updater and GRUB marker files, not RAUC or `grub-editenv`. The kernel command line carried `slot=A/B`.

Cosmetic issues included manual `/tmp` mounting, network/sysctl warnings, and the default Buildroot banner. Wizard, signing, AArch64, and LUKS remained future work in that entry.

## WraithOS integration — reported verified

The notes emphasized that the Rust orchestrator was inside WraithOS as `/sbin/wraith-init`, started by `/etc/init.d/S99wraith-init`, with config, DB/logs, and modules bind-mounted from `/data`.

The following were reported working: health, metrics, debug state, audit, dashboard, WebSocket, module CRUD, tenant routes, and the build route. The build route is opt-in in the current source; a historical endpoint list does not prove it was enabled.

QEMU access was described through:

```text
-netdev user,hostfwd=tcp::9091-:9090
```

Then the host could call `http://127.0.0.1:9091/ui`, optionally through an SSH tunnel.

The entry called the dashboard “production-ready” and proposed a separate port-8080 wizard. Those are historical claims, not current security conclusions. The current dashboard has documented authentication/HTTPS/event-escaping limitations, and the wizard shares the main listener.

A final 2026-11-02 entry reported the dashboard HTML, debug JSON with 32-second uptime, auto-start, persistent SQLite, bidirectional A/B behavior, and all 25 milestones working inside the OS. These observations were not rerun for this documentation update.

## Termux deployment — reported verified, 2026-11-02 22:40

The note reported native Android ARM64 execution in Termux without emulation:

- `wraith-termux-aarch64.tar.gz`: 8.3 MB compressed, 16 MB extracted.
- Cross-compilation through `cargo zigbuild --target aarch64-unknown-linux-musl`.
- Static musl ELF running in Termux; PID 28226 and healthy uptime reported.
- Health, readiness, metrics, audit, debug state, dashboard, and Rhai smoke event reported working.

The package directory was described as containing the binary, a phone-oriented config (128 MB memory, 64 sockets), run/stop/status helpers, and README. The binary is not tracked in the current package directory and must be supplied separately.

An OCI pre-authenticated download URL was reported with roughly seven-day expiry from 2026-11-02. No usable URL is reproduced here. The note planned to move the permanent host to an HP Termux instance after the Oracle VPS ended; that migration is not verified by this checkout.

## Session update, 2026-11-03 — v0.21.5 through v0.23.5

| Reported tag | Scope |
|---|---|
| v0.21.5 | Socket ownership checks and a logging race fix |
| v0.22.0 | TCP read/write through WIT and the async host bridge |
| v0.22.1 | CI workflow and ten integration tests |
| v0.22.2 | Authentication middleware and build gate |
| v0.23.0 | Per-module capability manifests/sidecars |
| v0.23.1 | AArch64 build and boot |
| v0.23.2 | ARM Bash and shell restrictions |
| v0.23.3 | ARM immutable SquashFS |
| v0.23.4 | ARM A/B and GRUB EFI, reported bidirectional |
| v0.23.5 | ARM size optimization, reported 17 MB SquashFS |

The note listed seven reviewer findings as addressed: missing integration tests, missing auth boundary, global-only capabilities, placeholder TCP behavior, unscoped sockets, startup execution probes, and missing ARM support. Current source still has important scope/persistence limitations; “addressed” is not equivalent to a completed security audit.

Reported state:

- x86_64 disk: 577 MB, BIOS GRUB, A/B plus data.
- ARM64 disk: 458 MB, EFI GRUB, A/B plus data, 17 MB rootfs.
- Static binaries: x86 about 23 MB, ARM about 19 MB.
- The note claimed feature parity. Current trees differ materially in boot counters, watchdog scripts, init configuration, and update behavior.

The recorded product direction was a developer PaaS; ARM appliance work was described as investment Oracle Always Free deployment rather than the core PaaS roadmap.

Remaining work at that point included the build sandbox, local HTMX/XSS fixes, wizard, LUKS, signing, boot counters, CI formatting/Clippy, distribution cleanup, and documentation synchronization.

A 2026-11-04 status update marked those phases complete except LUKS, automatic A/B boot-counter rollback, and documentation synchronization. It named `v0.27.0-wip-m31-config` and `v0.26.0`. Current source contains additional wizard/LUKS and ARM boot-counter code, so that status is itself historical.

## M31 LUKS work-in-progress specification

The historical note reported Block 1 complete: kernel DM_CRYPT and XTS/AES/SHA256/HMAC options, plus ARM cryptsetup/libgcrypt/libuuid/popt/json-c packages.

Remaining blocks were:

1. Extend the wizard finish path to format `/dev/vda4` and create ext4 on the mapper device.
2. Teach S30data to unlock LUKS before mounting.
3. Leave the ARM data region unformatted in post-image generation.
4. Rebuild and test ARM boot/unlock in QEMU.

The proposed wizard extension added optional `encryption_password`, an in-memory password field, and a `luks_enabled` marker. It specified minimum length eight, checking that `/dev/vda4` exists and is not already LUKS, then running:

```text
cryptsetup luksFormat --type luks2 --cipher aes-xts-plain64 \
  --key-size 512 --hash sha256 --batch-mode --key-file - /dev/vda4
cryptsetup luksOpen --key-file - /dev/vda4 wraith-data
mkfs.ext4 /dev/mapper/wraith-data
cryptsetup luksClose wraith-data
```

The proposal included a LUKS-active marker and an Argon2 hash file, with command failures returning a stderr tail. **This is a destructive historical specification, not a safe command to run on an arbitrary host.** The current backend still hardcodes the device, and the embedded wizard does not expose the encryption-password input.

The S30data proposal used `cryptsetup isLuks`, an unlock prompt, and a mapper mount. The genimage proposal removed the preformatted ext4 data image. The QEMU plan required setting an encryption password through the wizard, finishing setup, verifying LUKS, rebooting, unlocking through serial, and checking persistent mounts.

The final note warned that the existing tarball still used plain ext4 and had to be rebuilt before testing the new behavior. It estimated cryptsetup compilation at 5–8 minutes and kernel rebuild at about 15 minutes. Those estimates were not measured here.

---

**End of historical handoff.** For current behavior, verification boundaries, and safe operational guidance, use the [English documentation index](docs/index.html).
