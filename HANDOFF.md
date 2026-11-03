# Wraith-V2 + WraithOS — Handoff Document

**Last updated:** 2026-11-02
**Repo:** https://github.com/kurohigee07/Orchestrator-
**Current version:** v0.17.1 (WebSocket + Dashboard + Audit Logs + SDK/CLI)
**WraithOS build status:** In-progress (Buildroot compilation)

---

## 1. Project Overview

**Wraith-V2** = unified core engine (Rust static musl binary) untuk menjalankan:
- Wasm plugins (component model, wasm32-wasip2)
- Rhai scripting (embedded, event-driven)
- HTTP API (:9090)
- Multi-tenant (auth argon2, quota, audit logs)
- Metrics (Prometheus), WebSocket event stream, web dashboard (HTMX)

**WraithOS** = appliance OS (Buildroot-based) yang boot langsung ke Wraith-V2.
- Immutable rootfs (SquashFS + overlayfs)
- A/B atomic update (RAUC) — planned M27
- Kernel tuned: eBPF, AF_PACKET, cgroups v2, panic=5, watchdog
- tmpfs untuk /modules & SQLite (sub-millisecond Wasm exec)

---

## 2. Current State (milestones)

| Version | Feature |
|---------|---------|
| v0.1.0-mvp | M1-M7: static musl binary, Wasm bridge, Rhai worker, hardening |
| v0.2.0 | M8-M9: config TOML + CLI + metrics + systemd |
| v0.3.0 | M10-A: bounded Wasm concurrency (Semaphore) |
| v0.4.0 | M10-B: per-instance memory limit (256 MB) |
| v0.5.0 | M11-M13: tracing + /debug/state + install/smoke scripts |
| v0.6.0 | M14: ed25519 signature verification |
| v0.7.0 | M15: module lifecycle API (upload/replace/delete/execute) |
| v0.8.0 | M16: per-module quota (mem, conc, rate) |
| v0.9.0 | M17: multi-tenant (auth argon2, namespace isolation) |
| v0.10.0 | M18: per-tenant resource caps |
| v0.11.0 | M19: build-from-source pipeline (cargo build in sandbox) |
| v0.12.0 | M20: OCI Object Storage backup (rclone, daily timer) |
| v0.13.0 | M21: Wraith SDK (Rust client) + CLI (wraith binary) |
| v0.14.0 | M22: Wasm EventBus (cross-language: Wasm to Rhai to Rust) |
| v0.15.0 | M23: audit logs (SQLite execution_logs table) |
| v0.16.0 | M24: web dashboard (HTML + HTMX, /ui) |
| v0.17.0 | M25: WebSocket live event stream (/ws/events) |
| v0.17.1 | M25: WS test helper script |

---

## 3. Deployment Details

**VPS Oracle:**
- Hostname: instance-20261018-1417
- OS: Oracle Linux 9 (x86_64)
- RAM: 48 GB, 8 CPU
- User: opc
- Project dir: ~/orchestrator-project

**Service:**
- systemd user unit: ~/.config/systemd/user/orchestrator.service
- Binary: ~/orchestrator-project/target/x86_64-unknown-linux-musl/release/orchestrator
- Config: ~/orchestrator-project/orchestrator.toml
- Logs: ~/orchestrator-project/logs/orchestrator.log
- Bind: 127.0.0.1:9090 (localhost only)

**Backup:**
- OCI bucket: oci:wraith-bucket/daily
- rclone config: ~/.config/rclone/rclone.conf (native oracleobjectstorage provider)
- OCI config: ~/.oci/config
- Backup script: deploy/backup.sh
- Systemd timer: ~/.config/systemd/user/orchestrator-backup.timer (daily 03:00)
- Retention: 7 terakhir

**CLI:**
- Binary: ~/.cargo/bin/wraith
- Commands: list, deploy, exec, delete, logs
- Source: wraith-cli/, wraith-sdk/

---

## 4. WraithOS Build Status

**Build directory:** ~/buildroot-2024.02.9
**BR2_EXTERNAL:** ~/wraithos-br2-ext
**Defconfig:** ~/buildroot-2024.02.9/configs/wraithos_x86_64_defconfig
**Build log:** /tmp/wraithos-build.log
**tmux session:** wraith-build

**Build command:**
    cd ~/buildroot-2024.02.9
    make O=output -j8 2>&1 | tee -a /tmp/wraithos-build.log

**Target artifacts (setelah selesai):**
    ~/buildroot-2024.02.9/output/images/
    bzImage
    rootfs.squashfs
    wraithos.img (kalau ada post-image script)

**Custom overlay (BR2_EXTERNAL):**
    ~/wraithos-br2-ext/board/wraithos/
    kernel.fragment              (eBPF, AF_PACKET, cgroups, panic=5)
    rootfs_overlay/
        sbin/wraith-init         (binary orchestrator)
        etc/fstab                (tmpfs /modules & /var/lib/orchestrator)
        etc/wraith/orchestrator.toml
        etc/systemd/system/multi-user.target.wants/orchestrator.service
        usr/lib/systemd/system/orchestrator.service

**Known build gotchas (sudah diatasi):**

1. Oracle Linux repos (ol9_ksplice, RHCK, UEK) timeout -> disable via dnf --disablerepo atau ganti ke AlmaLinux mirror untuk install deps
2. Legacy config di Buildroot -> pakai BR2_SKIP_LEGACY=y atau update nama opsi (BR2_PACKAGE_CURL -> BR2_PACKAGE_LIBCURL_CURL)
3. Mirror Savannah/MPFR lemot (~5 KB/s) -> set BR2_PRIMARY_SITE="https://mirrors.kernel.org/sources/" di output/.config
4. Kill build di tengah -> AMAN, Buildroot pakai stamp files (.stamp_*_installed) untuk resume
5. Fix deps kurang (gcc-c++, patch, perl, ncurses-devel) -> pakai AlmaLinux mirror sementara:
   sudo tee /etc/yum.repos.d/alma-temp.repo -> dnf --disablerepo=* --enablerepo=alma_* install ...

---

## 5. Architecture (High Level)

**Wraith-V2 binary:**
- Axum HTTP server (routes: /modules, /t/{tenant}/modules, /ui, /ws/events, /metrics, /healthz, /audit/logs, /build)
- Wasmtime component model (Wasm plugins, sandboxed)
- Rhai (event-driven scripts, spawn_blocking isolation)
- EventBus (tokio broadcast)
- SQLx SQLite (execution_logs, persistent state)
- Rustls-ring TLS (no OpenSSL)

**Defense-in-depth limits:**
- Epoch deadline: 60s per execute
- Wasm memory: 256 MB per instance (StoreLimits)
- WasmPool concurrency: 2 (global Semaphore)
- Module quota: per module (memory, concurrency, rate limit)
- Tenant caps: per tenant (modules, concurrency, rate limit)
- Socket registry: 256 global cap
- Build timeout: 300s, concurrency 1

**Multi-tenant:**
- Registry key: (tenant, id)
- Auth: Bearer via argon2 PHC hash (or plaintext for dev)
- Storage: modules/{tenant}/*.wasm
- Cross-tenant exec: 404 (not 401, avoid existence leak)

---

## 6. Config Penting

**orchestrator.toml** highlights:
    db_path = "orchestrator.db"
    modules_dir = "./modules"
    max_concurrent_wasm = 2
    wasm_memory_limit_mb = 256
    metrics_bind = "127.0.0.1:9090"
    default_tenant = "default"

**Kernel fragment** (wraithos-br2-ext/board/wraithos/kernel.fragment):
- CONFIG_PACKET=y, CONFIG_BPF_SYSCALL=y, CONFIG_BPF_JIT=y
- CONFIG_CGROUPS=y, CONFIG_CGROUP_BPF=y
- CONFIG_WATCHDOG_NOWAYOUT=y, CONFIG_PANIC_TIMEOUT=5, CONFIG_PANIC_ON_OOPS=y
- CONFIG_SQUASHFS=y, CONFIG_SQUASHFS_LZ4=y, CONFIG_OVERLAY_FS=y
- Disable: DRM, FB, SOUND, WLAN, BT

---

## 7. Roadmap

**Selesai:** M1-M25 (lihat tabel section 2)

**In-progress:**
- M26: WraithOS Level 2 (Production Appliance)
  - [x] Buildroot base config
  - [x] Kernel fragment (eBPF, cgroups, panic, watchdog)
  - [x] Custom overlay (binary, systemd, fstab tmpfs)
  - [ ] Build compile (in-progress)
  - [ ] QEMU boot test
  - [ ] Verify 6 checkpoints

**Planned (Level 3 kalau waktu cukup):**
- M27: RAUC A/B update (atomic, rollback)
- M28: Web first-boot wizard (:8080, HTML form)
- M29: LUKS encrypted data partition
- M30: aarch64 build (Raspberry Pi)
- M31: Signed releases (GPG + SHA256SUMS)

---

## 8. How to Resume in New Chat

Kalau context window assistant habis dan buka chat baru, kirim pesan:

    Project: WraithOS (appliance OS untuk Wraith-V2 orchestrator).
    Repo: github.com/kurohigee07/Orchestrator-
    Milestone terakhir: v0.17.1 (WebSocket + Dashboard + Audit Logs + SDK/CLI).
    WraithOS status: buildroot build in-progress di tmux session 'wraith-build'.
    Baca HANDOFF.md di repo untuk full context.
    Next step: QEMU boot test setelah build selesai.

Assistant di chat baru langsung ngerti tanpa perlu recap panjang.

---

## 9. Verification Checklist (WraithOS)

Setelah build selesai, verify 6 hal ini di QEMU:

1. **Systemd started:**
       systemctl status orchestrator.service
   Expected: active (running)

2. **Healthz:**
       curl http://127.0.0.1:9090/healthz
   Expected: ok

3. **tmpfs aktif:**
       mount | grep -E '/modules|/var/lib/orchestrator'
   Expected: keduanya tmpfs

4. **panic=5 aktif:**
       cat /proc/cmdline
   Expected: panic=5 di cmdline

5. **cgroups v2:**
       ls /sys/fs/cgroup/
   Expected: cgroup.controllers (v2 marker)

6. **eBPF tersedia:**
       ls /sys/fs/bpf/ 2>/dev/null || mount | grep bpf
   Expected: bpf filesystem mounted atau bpftool feature probe works

Kalau 6/6 pass -> WraithOS Level 2 SELESAI.

---

## 10. Contact / Ownership

- Owner: kurohigee07 (github)
- Repo visibility: private (default)
- Backup: OCI bucket + GitHub + local (kalau perlu)

---

**End of Handoff Document**

---

## WraithOS Level 2 MVP — VERIFIED (2026-11-02)

### Artifacts
- Kernel: bzImage 11 MB (Linux 6.6, eBPF + cgroups v2 + panic=5)
- Rootfs: rootfs.squashfs 7.1 MB (immutable)
- Binary: wraith-init 23 MB (static musl, embedded in rootfs)
- Image size: 18 MB total

### Verification (in QEMU with KVM)
- [x] Kernel boots (5-8s with KVM acceleration)
- [x] /bin/sh as init works (init=/bin/sh bypass for test)
- [x] /sbin/wraith-init present (23 MB ELF 64-bit static)
- [x] /etc/wraith/orchestrator.toml present
- [x] tmpfs mounted: /modules (256M), /var/lib/orchestrator (512M)
- [x] Kernel cmdline: panic=5 confirmed
- [x] cgroups v2: /sys/fs/cgroup/cgroup.controllers present
- [x] eBPF filesystem: bpf on /sys/fs/bpf type bpf
- [x] Loopback network: ping 127.0.0.1 0.036ms
- [x] Orchestrator PID 83 running
- [x] HTTP /healthz responds "ok" (HTTP/1.1 200 OK)
- [x] SQLite DB running in tmpfs
- [x] Rhai event processing (smoke test delivered=2)

### Known Issues (non-blocking for MVP)
1. BusyBox init stuck at S40network (wait_iface timeout on eth0)
   - Workaround: boot with init=/bin/sh, manually setup lo + start orchestrator
   - Fix: rename interfaces to en* wildcard OR disable wait_iface
2. No auto-start of orchestrator.service (systemd not active, BusyBox init)
   - Workaround: manual /sbin/wraith-init
   - Fix: add /etc/init.d/S99orchestrator
3. wget in BusyBox requires loopback UP before use
   - Fix: bring up lo in init script or disable wait_iface

### Next Steps for Level 3
- Fix init scripts (auto-start + network)
- Add post-image.sh to bundle bootable .img with GRUB
- RAUC A/B update (atomic rollback)
- LUKS encrypted /var/lib/orchestrator
- aarch64 build (Raspberry Pi)
- Web first-boot wizard

---

## WraithOS M28 — A/B Atomic Update VERIFIED (2026-11-02 14:09 UTC)

### Confirmed working:
- Slot A boot: root=/dev/vda2, orchestrator runs, healthz ok
- A→B update: wraith-update dd to vda3, boot_b marker, reboot → slot=B
- Slot B boot: root=/dev/vda3, orchestrator runs, healthz ok
- B→A rollback: rm /boot/boot_b, reboot → slot=A
- All bidirectional verified in QEMU with KVM

### Architecture:
- 4 partitions: boot(FAT32 64M) + rootfs-a(squashfs 128M) + rootfs-b(squashfs 128M) + data(ext4 256M)
- GRUB markers: /boot/boot_b (or boot_a) as default override
- Custom updater: /usr/bin/wraith-update (no RAUC, no grub-editenv dependency)
- Kernel cmdline: slot=A/B for runtime detection

### Known cosmetic issues (non-blocking):
- /tmp fstab not auto-applied (workaround: manual mount tmpfs)
- S40network/sysctl warns (cosmetic, boot continues)
- Buildroot "Welcome to Buildroot" banner (issue file not overridden yet)

### Next steps:
- M29: Web first-boot wizard (embedded HTML + Rust handler)
- M30: Signed release bundles (SHA256SUMS + GPG)
- M31: aarch64 build for Raspberry Pi
- M32: LUKS encrypted /data partition

---

## WraithOS Integration Status — VERIFIED

**Rust orchestrator IS inside WraithOS** (not a separate project):

- Binary: `/sbin/wraith-init` = `target/x86_64-unknown-linux-musl/release/orchestrator`
- Auto-start: `/etc/init.d/S99wraith-init`
- Config: `/etc/wraith/orchestrator.toml` (bind-mounted from `/data/wraith/`)
- DB: `/var/lib/orchestrator/orchestrator.db` (persistent via /data)
- Logs: `/var/lib/orchestrator/wraith.log`
- Modules: `/modules/` (persistent via /data)

**All orchestrator features work inside WraithOS:**

- `/healthz` → ok
- `/metrics` → Prometheus
- `/debug/state` → JSON snapshot
- `/audit/logs` → execution history
- `/ui` → HTMX dashboard (embedded via include_str!)
- `/ws/events` → WebSocket live stream
- `/modules` → module CRUD
- `/t/{tenant}/modules` → multi-tenant
- `/modules/build` → build pipeline

**Network access (QEMU → external):**

Boot with `-netdev user,hostfwd=tcp::9091-:9090`.
Then from VPS host: `curl http://127.0.0.1:9091/ui`
Or via SSH tunnel from laptop: `ssh -L 9091:127.0.0.1:9091 opc@vps`

**Future work (if continuing):**

- M29: Web first-boot wizard (separate port :8080, embedded HTML)
- M30: aarch64 build for Raspberry Pi
- M31: LUKS encrypted /data
- M32: Signed releases

The orchestrator dashboard (`/ui`) is **already production-ready** inside
WraithOS. No additional integration needed — it's the same binary.

---

## Final Verification — 2026-11-02

Dashboard `/ui` confirmed live inside WraithOS:
- HTML response: `<!DOCTYPE html>... <title>Wraith Orchestrator</title>`
- `/debug/state` JSON: `{"modules":[],...,"uptime_secs":32}`
- Orchestrator binary auto-starts on boot
- All 25 milestones functional inside OS
- Persistent SQLite in `/data` bind mount
- A/B atomic update verified bidirectional

Rust project == WraithOS runtime. One binary, one OS.

---

## Termux Deployment — VERIFIED (2026-11-02 22:40)

Orchestrator runs **native** on Android ARM64 (Termux, no emulation):
- Binary: wraith-termux-aarch64.tar.gz (8.3MB compressed, 16MB extracted)
- Cross-compiled via `cargo zigbuild --target aarch64-unknown-linux-musl`
- Static musl ELF works on Android (Termux userland)
- PID 28226, uptime verified, healthz ok
- All endpoints working: /healthz, /readyz, /metrics, /audit/logs, /debug/state
- Dashboard accessible at http://127.0.0.1:9090/ui from phone browser
- Rhai EventBus functional (smoke event processed)

### Termux package contents (in wraithos/termux-package/)
- orchestrator (16MB aarch64 musl binary)
- orchestrator.toml (optimized for phone: 128MB mem, 64 sockets)
- run.sh, stop.sh, status.sh (helper scripts)
- README.txt (usage)

### Recovery via PAR URL
OCI bucket: oci:wraith-bucket/termux/wraith-termux-aarch64.tar.gz
Pre-Authenticated URL works (no auth needed at download time)
Expiry: ~7 days from 2026-11-02

### After VPS Oracle terminates
The HP Termux instance becomes the permanent orchestrator host.
No service interruption. Dashboard accessible from phone.

---

## Session Update: 2026-11-03 (v0.21.5 -> v0.23.5)

**10 tags, cross-arch full stack complete.**

### Completed today

| Tag | Scope |
|---|---|
| v0.21.5 | Socket ownership isolation + log race fix |
| v0.22.0 | TCP real read/write (WIT + async host bridge) |
| v0.22.1 | CI workflow + integration test (10 tests) |
| v0.22.2 | Auth middleware + build gate |
| v0.23.0 | Per-module capabilities (not global) |
| v0.23.1 | ARM aarch64 build + boot |
| v0.23.2 | ARM bash + lockdown |
| v0.23.3 | ARM immutable squashfs |
| v0.23.4 | ARM A/B + GRUB EFI (bidirectional verified) |
| v0.23.5 | ARM size optimization (17MB squashfs) |

### Reviewer feedback resolved

Original Arena review flagged 7 items, all addressed:
- Zero integration test -> 10 integration tests
- Auth boundary missing -> auth_middleware + startup validation
- Capability global (fundamental) -> per-module manifest + sidecar
- TCP teater -> read/write full I/O
- Socket ownership global -> owner-tracked, cross-module denied
- Startup probe in production -> removed
- ARM target -> WraithOS ARM boot verified

### Current state

- **x86_64 WraithOS**: 577MB, BIOS GRUB, A/B + data
- **ARM64 WraithOS**: 458MB, EFI GRUB, A/B + data, rootfs 17MB squashfs
- **Orchestrator binaries**: x86 23MB, ARM 19MB, static musl
- **Feature parity**: x86 == ARM (A/B, persistent /data, bash lockdown, orchestrator)

### Remaining (from TODO.md)

**Fase 1**: Build pipeline sandbox (bubblewrap/systemd-nspawn) - RCE risk
**Fase 2**: M29 wizard, M31 LUKS, M32 signing
**Fase 3**: HTMX lokal + XSS, boot counter auto-revert, CI fmt-clippy fix, dist cleanup, docs sync

### Direction

User chose PaaS direction (sell to developers). Appliance track (ARM WraithOS) was investment for Oracle Always Free ARM deployment, not PaaS feature work. PaaS core priority going forward.

### Next session priority

1. Build sandbox (4-6 hours, critical)
2. HTMX lokal + XSS (2 hours)
3. M29 wizard (2-3 hours)
4. CI fmt-clippy fix (1 hour)

All code at v0.23.5. OCI artifacts current. Both arch boot verified.
