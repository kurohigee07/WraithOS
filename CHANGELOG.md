# Changelog

All notable changes to this project. Releases are tagged
`v0.<minor>.<patch>-<slug>`. The slug retains the milestone code
(`mNN`) when the release closes a specific roadmap item; otherwise it
describes the change theme. `v0.30` was not released.

Crate manifests declare `0.1.0`; the tags are the release units.
Releases before `v0.24.0` are not itemized here; see commit history.

## [Unreleased]

### Fixed

- **RAUC trial-boot healthcheck reliability.** Three defects in
  `wraith-rauc-healthcheck` could rollback a healthy trial boot or
  corrupt slot state on a busy system:
  1. `wget` had no `-T` timeout. If `/readyz` accepted the
     connection but stalled (DB locked, handler wedged), the busybox
     `wget` blocked on the default socket timeout (900s+), far past
     the script's own 60s budget. The trial boot would always
     rollback even with a healthy orchestrator. Fixed by adding
     `-T 5` per attempt; 30 attempts * (5s + 2s) stays bounded.
  2. No singleton lock. `S99wraith-init` is re-run by
     `wraith-watchdog` on crash/restart; a second healthcheck could
     spawn mid-run. Two instances racing `rauc status
     mark-good` / `mark-bad` would corrupt slot state. Fixed with
     an atomic `mkdir` lock under `/run/wraith/`.
  3. `rauc status` / `wraith-bootenv` failures were silently
     dropped inside the pipe `cmd | while read ...`. A failing
     `rauc status mark-good` looked identical to a succeeding one in
     the log. Fixed by capturing output + exit code separately, then
     logging a WARN on non-zero rc.

### Fixed

- **wraith-bootenv atomic grubenv write.** The GRUB env block was
  written with `cat "$ENV_TMP" > "$ENV_FILE"` — the shell truncates
  the target first, then writes. A power loss mid-write would leave
  a partial 1024-byte block; GRUB reads garbage and may not find a
  valid slot, causing a boot loop. `ENV_TMP` also lived under
  `/run` (tmpfs), so even a `mv` would have been cross-fs
  copy+truncate. Fixed: tmp lives in `/boot/grub/` (same fs as the
  target) and the final step is `mv`, an atomic rename on vfat.
  Old env stays valid until the rename commits.
- **S30data mount failure cleanup.** After a successful TPM
  unseal, if the ext4 mount failed (corrupt fs, transient I/O),
  the LUKS mapper was left open. The plaintext fallback then
  failed with "mapper already exists", so `/data` stayed
  unmounted until reboot. Fixed: close mapper on mount failure
  so the fallback path can re-open.

### Security

- **H3: auto-finalize LUKS keyfile after TPM seal verified.** Previously
  the wizard sealed /boot/luks.key to the TPM but left the plaintext
  keyfile on the FAT32 ESP until an operator ran `wraith-tpm-finalize`
  manually. The plaintext keyfile fully bypasses the TPM seal (anyone
  who can read /boot can `cryptsetup luksOpen` without the TPM), so
  relying on operator discipline meant the "TPM-sealed LUKS" claim was
  only true on disciplined deployments. The wizard now finalizes
  automatically when (a) TPM seal succeeded and (b) S30data restart
  successfully unlocked /data from the TPM blob. `wraith-tpm-finalize`
  re-unseals the blob before removing /boot/luks.key, so any transient
  TPM failure leaves the plaintext in place and boot falls back
  normally — no new brick path. Manual `wraith-tpm-finalize` remains
  idempotent and available for cases the wizard cannot cover (TPM
  absent at wizard time, deferred operator flows).

  **Verification status:** 8 unit tests in `wizard::h3_tests` cover
  exit-code mapping (`Sealed/TpmAbsent/Failed`,
  `Finalized/Skipped/Failed`) and missing-script paths. The
  end-to-end TPM path (T7/T8 in `docs/testing.md`) is scheduled
  for v0.54 hardware validation: QEMU's TPM emulator backend
  aborts in init across QEMU 10.1 and 8.2 in the reference build
  host, and swtpm cannot be exercised from QEMU in this
  environment. WraithOS code and shell scripts are unchanged
  from v0.53 in the finalize logic — H3 only adds a caller that
  invokes the existing `/sbin/wraith-tpm-finalize`.

- **Rhai script fuel budget (H2).** Operator Rhai scripts ran on a
  bare `Engine::new()` with no operation, memory, or recursion budget.
  A `while true {}` typo in an event handler wedged the worker's
  blocking thread indefinitely; a `s += "x"` loop could exhaust host
  memory. Every Engine (compile-time and per-dispatch) now applies
  `max_operations = 10M`, `max_string_size = 1 MiB`,
  `max_array_size = 100k`, `max_map_size = 10k`,
  `max_expr_depth = 64`, `max_function_expr_depth = 32`. All limits
  configurable under `[script]` in `orchestrator.toml`.

- **dm-verity runtime rootfs integrity on x86_64.** Secure Boot
  verified the boot chain but not the running rootfs. A raw disk
  attacker could replace `/dev/vda2` or `/dev/vda3` after install
  and the kernel would mount it. dm-verity closes this: a Merkle
  hash tree is computed at build time and stored inline in each
  rootfs partition, and the kernel verifies every block against it
  on read. Any modified block triggers a kernel panic.

  Implementation shape:
  - The hash tree is appended to the squashfs *in the same
    partition* (rootfs-verity.squashfs = 128M squashfs + tree +
    zeros). No new partition, so data stays at `/dev/vda4` and
    S30data / wizard.rs are unchanged. The v0.52 -> v0.53 RAUC
    upgrade path works without re-partitioning.
  - Root mounts via `/dev/dm-0` (created by the kernel from a
    `dm-mod.create=` cmdline param; `CONFIG_DM_INIT=y` in the
    kernel fragment). No initramfs.
  - On corruption: panic. `panic=5` reboots. Correct behavior for
    a tamper-detection mechanism is to fail hard, not fall through.
  - Trust anchor: root hash + salt live in `grub.cfg` on the ESP.
    On the v0.50 Secure Boot image that chain is firmware-verified;
    on v0.51+ BIOS-only images it is trusted-by-origin, not
    cryptographically anchored. An attacker who can write the ESP
    can also write the hash. Documented in `docs/security.md`.

  What this catches: any tamper of the rootfs slot after install
  (raw disk access, silently corrupted disk, malicious RAUC bundle
  that somehow passed signature check).

  What this does *not* catch (honest list in `docs/security.md`):
  ESP tamper on BIOS-only images, `/data` tamper (LUKS covers
  confidentiality not integrity), and runtime kernel-level memory
  tamper.

  x86_64 shipped in this batch. aarch64 follows in the same
  release. Tested on QEMU TCG (x86_64, `-cpu max`): happy path boots
  to login; tampered byte triggers
  `Kernel panic - not syncing: dm-verity device corrupted` within
  ~3.6 s, `Rebooting in 5 seconds` (panic=5).

## [v0.52.0-security-hardening] - 2026-11-14

### Security

- Batch-1 fixes from the 2026-11-14 audit:
  - **H2 tenant path validation.** `s.modules_dir.join(tenant)`
    was called with a tenant from the `{tenant}` URL segment
    without validation. `POST /t/%2e%2e/modules` could write
    outside `modules_dir`. `validate_tenant()` now requires
    `[a-zA-Z0-9_-]{1,64}` and is called at the top of
    `upload_shared`, `replace_shared`, `delete_shared`,
    `put_quota_shared`.
  - **M3 default body limit on root router.** Wizard, UI, and
    metrics routes had no body limit; a large JSON POST to
    `/wizard/step1` could allocate unbounded memory before serde
    rejected. `.layer(DefaultBodyLimit::max(1 MiB))` on the outer
    router. Inner 10 MiB limits still win (innermost wins at
    body-read time).
  - **M5 drop `WasiCtxBuilder::inherit_stdio()`.** Guest modules
    inherited host stdin/stdout/stderr. Host I/O leak removed.
    Module logging goes through the log capability.
  - **M1 rhai `emit_event` capability check.** `log_info` checked
    `log_allowed`, but `emit_event` did not check
    `CapabilitySet::check_emit_event`. A rhai module without
    `EmitEvent` could emit any event kind. `CapabilitySet` is now
    cloned before `spawn_blocking`, passed into the closure, and
    checked before the sink push.
  - **L1 audit writer drain on shutdown.** `AppContext::shutdown`
    used `self.audit.sender().closed()` which never completes
    because the `AuditWriter` held by `AppContext` keeps the
    channel open; the previous path was always a 2-second no-op
    followed by `db.close()` while the writer was still running.
    Adds `AuditWriter::drain(&self)` (Notify + Mutex-guarded
    join handle), a `shutdown.notified()` select arm in
    `writer_loop`, and a `try_recv` drain before the final flush.
    `AppContext::shutdown` calls `audit.drain()` before
    `db.close()`. Landed in `30146dd`.
  - **M2 http::get body cap.** `wasm/host_impls.rs` read the
    whole `http::get` response body via `resp.text().await` with
    no size limit. A hostile host in `allowed_hosts` could
    exhaust process memory. Now rejects `Content-Length` > 16 MiB
    up front and enforces the same cap on the streaming read
    (`bytes_stream()` cumulative total), so decompressed bodies
    are bounded too. Helper `check_body_size` extracted and
    covered by 4 unit tests. Constant
    `MAX_HTTP_BODY_BYTES = 16 MiB`.
  - **H1 wasm fuel budget.** Wasmtime ran with only the 60s epoch
    deadline as a CPU bound; a CPU-bound guest could hold a Wasm
    pool slot for the full window. Enabled
    `config.consume_fuel(true)` and set
    `store.set_fuel(MAX_FUEL_PER_EXECUTION = 100_000_000)` per
    execution (fresh Store each call). Chosen empirically: the
    heaviest legit fixture (`cpu_heavy.wasm`) consumes ~1.59M
    fuel, `sample.wasm` ~20K, `json_heavy.wasm` ~34K,
    `adversarial.wasm` ~2.7K, `empty.wasm` ~1.2K. 100M is ~63x
    headroom over the heaviest and bounds worst-case CPU to a
    few hundred ms. Fuel exhaustion is surfaced as a distinct
    error message
    (`guest exceeded fuel budget of N instructions`) instead of
    the raw trap, detected by matching Wasmtime's trap text.
    Benchmark instrumentation was a temporary test deleted in
    the same commit (see CHANGELOG evidence below).
  - **L4 audit send failure visibility.** `wasm/module.rs` used
    `let _ = audit_tx.send(...)` and silently dropped the row if
    the writer task had exited. Now logs a `warn!` with tenant,
    module_id, and the channel error on the `Err` branch. The
    execute response is unchanged. Landed in `30146dd`.
  - **C2-E reset-luks confirmation header.**
    `reset_luks_handler` runs `luksErase /dev/vda4`. Full block
    on `w.completed` would break the documented recovery path
    for damaged `/data`, so a header requirement
    (`X-Wizard-Confirm: I-understand-data-loss`) plus an
    `audit` event (`wizard.luks_erase`) is used instead. See
    `docs/wraithos.md` -> "Wizard reset-luks recovery".
- **M4 capability ceiling intersection.** `resolve_capabilities`
  now intersects the requested `X-Module-Capabilities` header
  with the deployment-wide ceiling: requested caps that are not a
  subset of `ctx.capabilities` are rejected at upload time. Adds
  `CapabilitySet::is_subset_of` + `host_subsumes` + a unit test
  (`subset_of_enforces_ceiling`). Landed in commit `9c5f3d5`
  (folded into a fmt commit; annotated in `c012d2a`).
- **C1 tar extraction before bwrap.** External `tar` invocation
  replaced with the `tar` crate. Entries with `..`, absolute
  paths, prefixes, symlinks, hardlinks, or non-regular
  non-directory types are rejected before any write. Three tests:
  accept, parent-traversal reject, symlink reject. Landed in
  commit `d160b43`.

### Documentation

- **Post-mortem for the v0.50.0 Secure Boot key loss** at
  `docs/security.md` -> "Build-process post-mortem (v0.50.0 Secure
  Boot key loss)". Documents the timeline (script defaulted to
  `/tmp/sb-keys`, inverted warning logic), why the v0.50.0 image
  remains valid (PE signatures are self-contained; `db.crt` is
  embedded in the signed binaries and published as a release
  asset), why a lost key is not a compromise (no attacker
  capability added), and the fix that landed on main after the
  v0.50.0 tag (`aae2af7`: `gen-sb-keys.sh` now defaults to
  `$HOME/sb-keys`, refuses `/tmp/` without `--force-dev`, pauses
  with a backup checklist).
- **Certificate migration runbook** at `docs/wraithos.md` ->
  "Certificate migration v0.50.0 -> v0.52+" (subsection 6 of
  "Building a UEFI Secure Boot x86_64 image"). Step-by-step
  procedure for replacing the enrolled `db.crt` on a
  v0.50.0-provisioned device before flashing a v0.52+ image
  signed with a different key. Explicitly destructive: wipes the
  disk including LUKS `/data`. Includes firmware enrollment
  verification step and `/data` restore step from a v0.50.0
  backup.
- Both docs are additive to v0.51.0 (which shipped the dropbear
  gate). The v0.51.0 tag is not modified; the docs land on
  `main` after the tag and will roll into v0.52 release notes if
  not surfaced sooner.

## [v0.51.0-dropbear-provisioning] - 2026-11-14

### Security

- **Pre-wizard SSH is closed.** Before this release, `S50dropbear`
  started on every boot including the first one, before the
  first-boot wizard had completed. The shipped default root
  password (`wraithos-admin-change-me`, documented in README) was
  therefore reachable on port 22 on a fresh device from the moment
  network came up, before the operator could provision anything.
  The SSH pubkey mode (`dropbear -s`) only disables password auth
  *after* the wizard writes the `ssh-pubkey-only` marker, so it did
  not close the pre-wizard window. An attacker with network
  reachability to a fresh device could authenticate as root with
  the public default.
- Fix: `S50dropbear` now refuses to start until
  `/var/lib/orchestrator/wizard.complete` exists. On skip it prints
  `SKIPPED (wizard not complete)` and exits 0 (same exit code as
  success so BusyBox init does not treat the skip as a failure).
  The gate applies to both x86_64 and aarch64 boards.
- `wizard.rs` `finish_handler` restarts `S50dropbear` after the
  marker is written, so port 22 becomes available in the same
  wizard run without a reboot. Ordering is intentional: the
  restart happens after `w.completed = true` and after
  `write_sync(&marker, ...)`; running it earlier would re-trip the
  guard.
- Serial console remains password-authenticated with the documented
  default throughout. Serial is the primary provisioning interface
  and the documented recovery path (`docs/security.md` -> "SSH
  authentication", "Default credentials"). The gate is about
  *timing* (no pre-wizard SSH exposure), not about *credential*
  (the default is still valid on serial, and on SSH post-wizard
  until the operator sets an SSH pubkey or rotates `/etc/shadow` at
  build time).

### Changed

- README callout, `docs/security.md` "SSH authentication" and
  "Default credentials" sections, and `docs/wraithos.md`
  "First-boot wizard and LUKS" section now document the gate and
  the pre-v0.51 exposure it closes.

### Upgrade notes

- RAUC bundle from v0.50.0 -> v0.51.0 works normally on both
  arches; the gate is a rootfs change (S50dropbear script +
  orchestrator binary), and rootfs is what RAUC updates.
- v0.51.0 does **not** ship an SB-enabled disk image. The v0.50.0
  SB keys are lost (see the v0.50.0 release notes' "Key
  lifecycle"). Building v0.51.0 with `WRAITH_SB_KEYS` will fail
  signing because the private keys are not available; v0.51.0
  images are BIOS-only by design. SB will resume in a future
  release after the device-specific key rollout. Existing v0.50.0
  SB deployments continue to be RAUC-updatable without re-enroll
  (ESP unchanged).
- Serial console behaviour, LUKS/TMP sealing (Item 1), and the
  SB/BIOS dual chain (Item 2) are unchanged from v0.50.0.

### Verified on QEMU

- **T-V051.1** fresh image, no wizard: host SSH probe on port 22
  gets connection refused / empty banner; serial shows
  `Starting dropbear sshd: SKIPPED (wizard not complete)`;
  serial login with `wraithos-admin-change-me` succeeds;
  wizard endpoints step1/step2/finish return 200.
- **T-V051.2** wizard completes over serial; within ~2s of
  `finish` returning `{"status":"complete"}`, `ss -tln` shows
  `LISTEN 0.0.0.0:22`; host SSH banner is
  `SSH-2.0-dropbear_2024.85`; `/var/lib/orchestrator/wizard.complete`
  exists; `healthz` over SSH returns `ok`.
- **T-V051.3** reboot with marker present: serial shows
  `Starting dropbear sshd: OK` (no SKIPPED); `/data` mounted via
  plaintext keyfile (TPM absent fallback, Item 1 path unchanged);
  `healthz` over SSH returns `ok`; no wizard prompt.
- **T-SB0** BIOS regression: BIOS path boots, `/healthz` ok,
  dropbear opens normally when marker present.

### Known gap: SB v0.51 signing keys lost

The SB v0.51 keys (PK/KEK/db) were generated during the v0.51 build
cycle but never used: v0.51 was tagged but not published (superseded
by v0.52) and v0.52 ships **BIOS-only** (no SB chain in the ESP, see
"v0.52 images are BIOS-only" above). After v0.52 was published, the
`master-keys-*.enc` archive holding those v0.51 private keys could
not be decrypted — the password had been rotated between encryption
(2026-11-14 03:47 UTC) and recovery attempt.

**Impact: zero for deployed devices.** No SB-enabled v0.51 or v0.52
image was ever shipped, and no user enrolled a v0.51 SB certificate.
v0.50.0 SB image + `db.crt` remain valid and unaffected. v0.53+ will
ship per-device SB keys per `docs/wraithos.md` → "Building a UEFI
Secure Boot x86_64 image", the same protocol as v0.50.

`master-keys-v2-*.tar.gz.enc` (uploaded to OCI safe-backup with a
fresh password) contains the intact RAUC CA production keys and the
recovered v0.50 SB public cert, so RAUC bundle signing and v0.50 SB
verification continue to work indefinitely.

### Not changed

- Default root password remains `wraithos-admin-change-me`
  (documented as a conscious decision in
  `docs/security.md` -> "Default credentials"). Changing it is a
  build-time edit to `/etc/shadow` in the board overlay; not part
  of this fix.
- No new firewall rules. SSH still binds `0.0.0.0:22` post-wizard;
  the operator is expected to restrict reachability upstream
  (`docs/security.md` -> "SSH authentication" known gaps).
- No changes to `/etc/rauc/system.conf`, RAUC CA, `make-bundle.sh`,
  the v0.50.0 tag, or the v0.50.0 release assets.

## [v0.50.0-secureboot] - 2026-11-14

### Added

- **UEFI Secure Boot for x86_64** (aarch64 Secure Boot remains out of
  scope; requires U-Boot FIT measured boot which the current
  EFI-GRUB aarch64 image does not provide).
- Device-specific Secure Boot key generation script at
  `deploy/wraithos/uefi/gen-sb-keys.sh`. Produces PK, KEK, and db
  (RSA-2048, SHA-256, 10-year) encrypted with a password from
  `WRAITH_SB_KEYPASS`. Keys are never committed; the script refuses
  overwrite and warns loudly if the output directory is outside
  `/tmp/`.
- Build-time signing pipeline in `post-image.sh`:
  - `bzImage` is signed with the db key before being staged on the
    ESP. Firmware `LoadImage` verifies it through the EFI stub
    loader.
  - `grubx64.efi` is built from an `x86_64-efi` GRUB target and
    signed with the db key before being placed at
    `EFI/BOOT/BOOTX64.EFI`.
  - The whole pipeline is gated by `WRAITH_SB_KEYS` and
    `WRAITH_SB_KEYPASS`. Without `WRAITH_SB_KEYS` the build emits
    the legacy BIOS-only image and prints a loud warning so a
    production release cannot ship unsigned by accident.
- `deploy/wraithos/x86_64/genimage-efi.cfg`: same as `genimage.cfg`
  but includes `EFI` in the `boot.vfat` files list. The build picks
  `genimage-efi.cfg` when signing is enabled and `genimage.cfg`
  otherwise.
- New Buildroot config in `wraithos_x86_64_defconfig`:
  `BR2_TARGET_GRUB2_X86_64_EFI=y` and a matching
  `BR2_TARGET_GRUB2_BUILTIN_MODULES_EFI` list. GRUB dual-build now
  produces both `grub.img`/`grub-eltorito.img` (BIOS) and
  `EFI/BOOT/BOOTX64.EFI` (UEFI).
- GRUB 2.12 patches at `deploy/wraithos/grub-patches/` applied at
  build time:
  - `0001-skip-shim-lock-check.patch`: `linux.c` bypasses the
    `grub_is_shim_lock_enabled()` guard so GRUB uses the EFI stub
    loader instead of the legacy shim-requiring path.
  - `0002-bypass-shim-lock-verifier.patch`: `kern/efi/sb.c`'s
    `shim_lock_verifier_write` returns `GRUB_ERR_NONE` when the
    Microsoft shim protocol is absent. This only unblocks the open
    of an already-firmware-verified kernel; it does not disable
    firmware verification of `grubx64.efi` or `bzImage`.

### Changed

- `grub.cfg` adds `rootwait` to both menu entries. The UEFI EFI stub
  boot path enumerates virtio-blk later than the BIOS path; without
  `rootwait`, `VFS: Cannot open root device "/dev/vda2"` panics
  before the device is ready.
- `genimage.cfg` was silently diverged from the ext-tree copy
  (`size = 640M` present in the repo, absent in ext-tree). The
  divergence is now removed; the repo file matches the ext-tree
  source. The 640 MiB padding is still applied by
  `truncate -s 640M` in `post-image.sh`, not by the genimage config.
- `post-image.sh` stages `EFI/` from the boot stage into
  `$BIN_DIR/EFI` before invoking genimage, because genimage resolves
  `files = {...}` entries from `--inputpath`, not `--rootpath`.

### Verified on QEMU OVMF (KVM)

- **T-SB0** BIOS regression, unsigned image: boots, `/healthz` OK,
  `rootwait` in cmdline, `dmesg | grep -ci "secure boot"` = 0,
  `rauc status` reports slot A active.
- **T-SB1** UEFI + custom PK + Secure Boot ON + signed image:
  boots to `wraithos login:`, `dmesg` contains
  `Secure boot enabled`, `rootwait` in cmdline, `/data` mount attempt
  reached (fallback wizard path, no LUKS set up on fresh image).
- **T-SB2** UEFI, Secure Boot OFF, unsigned image: boots to login.
- **T-SB3** stock OVMF (Microsoft PK still in db), Secure Boot ON,
  signed image: firmware refuses,
  `BdsDxe: failed to load Boot0001 ...: Access Denied`.
- **T-SB4a** custom PK, Secure Boot ON, tampered **unsigned** GRUB
  on the ESP: firmware refuses, same `Access Denied`.
- **T-SB4b** custom PK, Secure Boot ON, signed GRUB: reaches GRUB
  menu and boots to login.

### Notes

- **Honest gaps (documented in `docs/security.md`):**
  - Kernel signature verification depends on OVMF's `LoadImage` PE
    verification via the EFI stub loader. This works in QEMU OVMF
    and should work on physical UEFI, but physical hardware is not
    tested in v0.50.0. Section title:
    "QEMU-verified, hardware validation pending v0.51".
  - Rootfs tamper is **not** detected at boot. `/dev/vda2` and
    `/dev/vda3` are raw squashfs slots with no runtime dm-verity
    layer. An attacker with raw disk access can replace the rootfs
    even with Secure Boot on. This gap existed on the BIOS path too;
    Secure Boot narrows the attack surface to post-install rootfs
    writes without closing it.
  - IMA appraisal is not enabled. Deferred to v0.51.
  - ARM Secure Boot is not part of v0.50.0.
- **No Microsoft shim.** The build uses two local GRUB patches to
  bypass the shim-lock requirement rather than shipping a
  Microsoft-signed shim. This keeps the release self-contained and
  avoids Microsoft's submission process. The patches do not weaken
  firmware verification.
- **Production build requirement.** `WRAITH_SB_KEYS` must be set
  when building a release image. Without it, the image is BIOS-only
  and the build prints a loud warning. This is intentional: dev/CI
  builds without keys must not fail, but production builds must not
  ship unsigned by accident.
- **Release engineering note.** The git tag
  `v0.50.0-secureboot` does not include built images. Operators
  build with their own device-specific keys; per-device keys are not
  shared across the fleet.

## [v0.49.0-tpm-luks-seal] - 2026-11-13

### Added

- **TPM 2.0 sealing of the LUKS keyfile (x86_64 + aarch64).** The
  wizard now seals `/boot/luks.key` to a TPM 2.0 device, when present,
  under a PCR policy (`sha256:0,1,2,3,4,5,6,7`) that includes firmware
  code/config, the GRUB EFI binary, grub.cfg, and Secure Boot state.
  The sealed blob is written to `/boot/luks.sealed.{pub,priv}` next to
  the plaintext keyfile. Anyone with only the disk can no longer
  decrypt `/data`: the sealed blob will not unseal on a different TPM
  and will not unseal if the boot chain's PCRs have changed.
- Five new operator scripts under `/sbin`:
  - `wraith-tpm-seal` — seal a keyfile to the current TPM. Idempotent
    in the sense that it overwrites the sealed pair.
  - `wraith-tpm-unseal` — unseal to a tmpfs keyfile. Writes only the
    keyfile path to stdout, all diagnostics to stderr and to
    `/var/log/wraith-tpm.log`.
  - `wraith-tpm-finalize` — one-way transition: re-verify the sealed
    blob unseals in the current boot, then remove the plaintext
    keyfile and write `/boot/.luks-sealed-verified`. Idempotent.
  - `wraith-tpm-reseal` — re-seal after a PCR change. Requires the
    plaintext keyfile to be present; refuses otherwise.
  - `wraith-tpm-prepare-update` — restore the plaintext keyfile from
    the sealed blob before a PCR-affecting operation (kernel/GRUB
    update, firmware update). No-op if plaintext already present.
- Shared library `/lib/wraith/tpm-lib.sh` with `TPM2TOOLS_TCTI=device:
  /dev/tpmrm0` (kernel resource manager, not `tpm2-abrmd`), `/boot`
  read-write probe, and `tpm_check` tri-state (present+functional /
  present+broken / absent).
- Kernel support: `CONFIG_TCG_TPM=y` + `CONFIG_TCG_TIS=y` builtin on
  both arches. ARM additionally `CONFIG_TCG_TIS_SPI=y` (override of
  arm64 defconfig's `=m` because the initramfs does not load modules).
- Buildroot packages in both defconfigs: `BR2_PACKAGE_TPM2_TSS=y`,
  `BR2_PACKAGE_TPM2_TOOLS=y`. `tpm2-abrmd` is not shipped; kernel
  resource manager is used.

### Changed

- `/etc/init.d/S30data` unseal priority: TPM-sealed blob first, then
  plaintext `/boot/luks.key`, then wizard-accessible fallback. Each
  outcome logs a distinct line on the serial console so recovery is
  visible without reading the kernel ring buffer.
- `wizard.rs::finish_handler` calls `/sbin/wraith-tpm-seal` immediately
  after LUKS setup. Failure is non-fatal: the wizard completes with
  plaintext-only fallback, and the failure is logged at warn level.
- Wizard-time `/etc/init.d/S30data restart` performs an unseal
  validation but does **not** remove the plaintext (v0.49.0 policy,
  see Notes).

### Verified on QEMU

- **ARM aarch64, swtpm + tpm-tis-device.** Wizard flow → seal → cold
  reboot → TPM unseal → `/data` mounted. Serial:
  `Setting up /data: TPM unseal OK. OK (A/B)`. `/var/log/wraith-tpm.log`:
  `unseal: OK keyfile=/run/wraith/luks.key.N`.
- **ARM aarch64, no TPM.** Wizard with encryption password → plaintext
  only, no sealed blob. Cold boot serial:
  `Setting up /data: TPM absent, using plaintext keyfile. OK (A/B)`.
- **ARM aarch64, TPM cleared externally.** New TPM state, same image
  with sealed pair. Cold boot serial:
  `Setting up /data: TPM unseal failed (rc=1), falling back to
  plaintext keyfile. OK (A/B)`. Sealed pair, plaintext, and `/data`
  all intact.
- **ARM aarch64, PCR change.** `tpm2_pcrextend 0:sha256=...01` inside
  the guest changed PCR 0 from `0x5F10...` to `0x3E93...`. Subsequent
  `wraith-tpm-unseal` failed with TPM error `0x99D` (`a policy check
  failed`), proving the PCR policy is enforced at the TPM, not by the
  script. Fallback to plaintext worked.
- **x86_64, swtpm + tpm-tis.** Same wizard → seal → cold-boot unseal
  flow as ARM. Cold boot serial:
  `Setting up /data: TPM unseal OK. OK (A/B)`.
- **x86_64, no TPM.** Fresh image, wizard → plaintext-only, sealed
  pair never created. Cold boot serial:
  `Setting up /data: TPM absent, using plaintext keyfile. OK (A/B)`.

### Notes

- **v0.49.0 policy: sealed + plaintext coexist.** The plaintext
  keyfile is never auto-removed. `wraith-tpm-finalize` performs the
  one-way transition explicitly, after a successful unseal in the
  current boot. Auto-removal on first successful unseal was
  rejected as a data-loss risk: any future PCR change (kernel/GRUB
  update, firmware update, TPM clear) with plaintext already removed
  would brick `/data` without an operator in the loop. Auto-removal
  is deferred to v0.50.0 after a production data cycle.
- **Kernel/initramfs updates require `wraith-tpm-prepare-update`
  before `rauc install`** while plaintext is finalized away.
  Skipping this step bricks `/data`. A RAUC bundle-level
  `hooks.sh` that automates this is deferred to v0.50.0.
- **x86 PCR dump not captured.** PCR 0-7 stability on x86_64 is
  inferred from successful seal/unseal across cold boots; individual
  PCR contents were dumped only on ARM. A dedicated x86 PCR dump
  is deferred to v0.50.0 hardening.
- **Recovery runbook** is in `docs/wraithos.md` → "TPM 2.0 sealing".
  Threat model and PCR justification are in `docs/security.md` →
  "TPM 2.0 sealing (shipped in v0.49.0)".
- ARM Verified Boot via U-Boot FIT signing remains out of scope and
  is unchanged from prior releases.

## [v0.48.0-audit-batching] - 2026-11-12

### Changed
- Audit-log writes are now batched. `orchestrator/src/audit.rs` adds a
  bounded `tokio::sync::mpsc` channel (capacity 10,000) and a dedicated
  writer task that coalesces rows into one multi-row `INSERT` per 50 ms
  or 100 rows. `WasmModule::execute_quota` sends an `AuditEntry` instead
  of awaiting a per-request INSERT; `AppContext::shutdown` drains the
  writer before closing the pool. Bounded capacity means backpressure,
  not data loss, under extreme load.
- Trade-off: a row is visible in `execution_logs` within 50 ms of the
  execute response, not immediately. The write path no longer pays an
  fsync-bound commit per request.

### Results

Benchmark measured on QEMU x86_64 on the same host, same `hey` client,
before vs after. Full methodology, raw `hey` output, per-run metric
snapshots and the aggregation JSON are checked in under
`docs/evidence/benchmarks-v0.47.0/`; the write-up is
`docs/benchmarks-v0.47.0.md`.

Empty guest (`tests/fixtures/empty/`):

| concurrency | before (req/s) | after (req/s) | boost |
|---:|---:|---:|---:|
| 1 | 917 | 5,374 | 5.9x |
| 5 | 959 | 9,924 | 10.3x |
| 10 | 933 | 9,813 | 10.5x |
| 50 | 984 | 9,309 | 9.5x |

Sustained 30 s at c=50: 292,967 requests, 0 errors, 9,764 req/s.

CPU-bound guest (`cpu_heavy`, 10,000 iterations):

| concurrency | before (req/s) | after (req/s) | boost |
|---:|---:|---:|---:|
| 1 | 798 | 3,149 | 3.9x |
| 5 | 825 | 5,592 | 6.8x |
| 10 | -- | 5,611 | -- |

Audit table count matched the requested execution count within the
50 ms flush window; no `audit batch insert failed` warnings in the
orchestrator log.

### Notes
- The bottleneck was never Wasm. `/healthz` reached 81,657 req/s and
  `/modules` (read-only) 66,212 req/s on the same host; `/execute`
  was the only path under 1,000 req/s. The per-request synchronous
  INSERT serialised on SQLite's single writer.
- Pool-wait still grows linearly above c=25 because `max_concurrent_wasm`
  is 2. Raising it was not attempted; a conservative default keeps tokio
  workers available for HTTP, metrics and signal handling when a guest
  is CPU-bound until the epoch trap.
- The audit table is not pruned. Long-running deployments accumulate
  rows and will eventually need retention or archival; not addressed
  here.

## [v0.47.0-x86-luks-fixes] - 2026-11-12

### Fixed
- x86 data partition table extended past the end of the image. The
  DOS partition table in `genimage.cfg` did not auto-pad the file to
  the last partition's end when that partition has no image source
  (p4 data is raw). genimage stopped writing after p3 content, so the
  kernel rejected p4 with "partition outside the disk" and `/dev/vda4`
  never appeared. GPT (ARM) auto-pads, hence the asymmetry. x86
  `post-image.sh` now runs `truncate -s 640M` after genimage.
- x86 `orchestrator.toml` default config was missing `data_dir`, so
  the wizard fell back to `default_data_dir()` = `./data`, resolved
  against the process CWD (`/`) to `/data` while `/data` was still
  the read-only squashfs root. The first wizard `mkdir wraith` failed
  with EROFS and returned 500 before LUKS was even attempted. The
  x86 default config now carries `data_dir = "/var/lib/orchestrator"`
  like ARM.
- `config.rs`: `default_data_dir()` now returns an absolute,
  writable path (`/var/lib/orchestrator`) instead of `./data`. A
  relative default is ambiguous for init-spawned processes and was
  the root cause of the x86 wizard 500.

### Verified on QEMU x86_64 with a broken bundle

Mirrors the ARM test from v0.46.0:

  boot 1: fresh image, wizard LUKS, wraith-install-bundle broken
          -> marker armed, grubenv ORDER=B A
  boot 2: slot B, orchestrator fails, healthcheck polls /readyz
          for 60 s, times out at 70.9 s, mark-bad + demote + reboot
  boot 3: slot A, grubenv slot=A tries=0 good=1 lkg=A B_OK=0,
          rauc status reports rootfs.1 bad and rootfs.0 booted good,
          /data/orchestrator/healthcheck.log contains the trail,
          /readyz 200

Both arches now pass the end-to-end health-gated rollback test
with an intentionally broken bundle.

## [v0.46.0-health-rollback-fixes] - 2026-11-12

### Fixed
- `wraith-bootenv demote` wrote the surviving slot as
  `tries=3, good=0, lkg=$c` where `$c` is the failing slot. After a
  rollback the grubenv looked like the surviving slot was itself
  unproven and the broken slot was labelled last-known-good.
  Functional rollback still worked because `grub.cfg` selects by
  `ORDER`, but the metadata was wrong and misleading to monitoring.
  `demote` now writes the surviving slot with `tries=0, good=1,
  lkg=$o`.
- `wraith-rauc-healthcheck` logged only through syslog. `/var/log`
  is a tmpfs; the timeout branch reboots, so the reason for the
  rollback disappeared before it could be read. The script now
  writes to `/data/orchestrator/healthcheck.log` in addition to
  syslog. `/data` is on the LUKS volume and survives the reboot.

### Verified on QEMU aarch64 with a broken bundle

A crafted bundle whose `/sbin/wraith-init` is a shell script that
exits 1 was installed on a fresh image:

  boot 1: wizard LUKS, wraith-install-bundle broken -> marker
          armed, grubenv ORDER=B A
  boot 2: slot B, orchestrator fails, healthcheck polls /readyz
          for 60 s, times out at 69.7 s, mark-bad + demote +
          reboot
  boot 3: slot A, grubenv slot=A tries=0 good=1 lkg=A B_OK=0,
          rauc status reports rootfs.1 bad and rootfs.0 booted
          good, /data/orchestrator/healthcheck.log contains the
          full timeout trail, /readyz 200

The successful path (`readyz 200 -> mark-good + commit`) was
verified in v0.45.0.

## [v0.45.0-rauc-health-rollback] - 2026-11-12

### Added
- Health-gated RAUC rollback. `wraith-install-bundle` wraps
  `rauc install` and writes `/data/orchestrator/rauc.pending` on
  success. On the next boot `S99wraith-init` spawns
  `wraith-rauc-healthcheck` in the background, which polls
  `http://127.0.0.1:9090/readyz` for up to 60 s (2 s interval).
  If `/readyz` returns 200 the script runs `rauc status mark-good`
  and `wraith-bootenv commit`, then removes the marker. If it
  times out the script runs `rauc status mark-bad`,
  `wraith-bootenv demote`, removes the marker, and reboots; the
  GRUB trial counter falls through to the last-known-good slot.
- `wraith-update` (dd path, non-RAUC) also writes the same marker
  so the health gate covers both install paths.
- `[pubkey-only]` marker line was added to S50dropbear logs in
  v0.44.0; this release keeps the same behaviour and reuses the
  same marker file name.

### Changed
- `/readyz` is used instead of `/healthz`. It confirms both the
  HTTP listener and the SQLite connection, which is what the
  health gate actually needs. `/healthz` remains a shallow check
  for upstream probes.
- The health check runs in the background after `S99wraith-init`
  finishes starting the orchestrator. It never blocks the boot
  sequence. If `/data/orchestrator/rauc.pending` is absent the
  script is a no-op.

### Notes
- Tested on QEMU aarch64:
  - `wraith-install-bundle` on a fresh LUKS-mounted system: marker
    written, reboot boots slot B, `readyz 200 after 2s`, mark-good,
    `wraith-bootenv commit`, marker removed, grubenv shows
    `slot=B, good=1, tries=0`.
  - Manual marker on slot A with orchestrator already ready: `sh -x
    wraith-rauc-healthcheck` exits 0, marker removed, grubenv
    `seq=2`, `/var/log/messages` shows
    `rauc-health: readyz 200 after 2s, marking good`.
- The broken-slot path (`readyz` timeout) was not exercised end to
  end in QEMU for this release. The timeout branch is unit-tested by
  setting `TIMEOUT_SECS=2` and pointing `READY_URL` at a closed
  port; the script runs `mark-bad`, `demote`, `reboot` in that
  order. Full E2E with a deliberately broken bundle is on the P2
  follow-up list.
- This release rebuilds the ARM and x86_64 bundles because the
  previous v0.44.0 draft bundle was produced before the health
  check scripts existed. v0.44.0 tag stays in history; its draft
  release will be removed once v0.45.0 is published.

## [v0.44.0-ssh-pubkey-only] - 2026-11-12

### Added
- Optional SSH public key at wizard step 1. When set, `finish_handler`
  writes `/etc/wraith/authorized_keys` (persisted on the LUKS volume)
  and a marker file `/etc/wraith/ssh-pubkey-only`. On every subsequent
  boot `S50dropbear` stages the key into `/root/.ssh/authorized_keys`
  (tmpfs) and starts dropbear with `-s`, so SSH refuses password
  authentication. Serial console keeps `/etc/shadow` password auth
  regardless — a lost SSH key is recoverable from the serial line.

### Changed
- `docs/security.md`: new sections covering SSH authentication modes,
  the physical-access threat model (LUKS keyfile on the ESP), boot
  chain integrity (no Secure Boot, no runtime dm-verity on the
  rootfs slot), and update verification / health-based rollback.
  TPM2 sealing, Clevis/Tang, UEFI Secure Boot, ARM Verified Boot,
  and S99wraith-init health polling are documented as v0.5x
  roadmap, not implemented.
- README: `Emergency SSH access` section documents both auth modes
  and the serial-console recovery path.

### Security
- Wizard validates `ssh_pubkey` server-side before saving. Allowed
  key types: `ssh-ed25519`, `ssh-rsa`, `ecdsa-sha2-nistp256/384/521`,
  and the two `sk-*@openssh.com` FIDO variants. Payload must start
  with `AAAA`, be at least 16 chars, and contain only base64
  characters. Invalid input returns 400 with a line-scoped reason;
  the wizard state is not modified.
- Multiple keys are accepted, one per line. Comment lines (`#`) and
  blank lines are stripped.

### Notes
- Empty `ssh_pubkey` (default) keeps password auth on dropbear,
  matching the behaviour of v0.43.x. Pubkey-only is opt-in per
  device, not a fleet default.
- Ephemeral host keys are unchanged: each boot regenerates them,
  clients see a `known_hosts` mismatch. Documented in
  `docs/security.md` under "Known gaps".

## [v0.43.0-luks-wizard-ssh] - 2026-11-12

### Added
- LUKS2 encryption for `/data` via first-boot wizard. `/dev/vda4` is
  formatted with `aes-xts-plain64`; the keyslot uses a 32-byte keyfile
  derived from the operator-chosen encryption password via argon2, and
  the keyfile is written to `/boot/luks.key` for auto-unlock at boot.
- `POST /wizard/reset-luks`: destructive recovery endpoint. Erases the
  LUKS header and clears wizard state so a lost or corrupt keyfile does
  not leave the device unreachable. Guarded by an unmounted `/data`
  check and a LUKS-device probe.
- Dropbear SSH on port 22 inside the guest as an emergency backup to
  serial console. Password authentication only; host keys regenerate
  each boot. QEMU wrappers forward host ports 2222 (x86_64) and 2223
  (aarch64).
- `deploy/run-wraithos.sh` and `deploy/run-wraithos-arm.sh` wrap QEMU
  with `-no-reboot` and a restart loop. Guest `reboot` under QEMU TCG
  leaves corrupted serial/tty state; restarting the QEMU process
  clears it. Reboot from the guest's perspective is unchanged.

### Changed
- `wizard.rs` / `wizard.html`: `encryption_password` is required at
  step 1; `luks_recovery_needed` is exposed from `/wizard/status` and
  drives a banner with a reset button. `setup_luks()` was reworked to
  use the derived keyfile rather than passing the password on stdin —
  QEMU serial drops bytes and users were getting locked out after
  mistyping.
- `S30data` unified across x86_64 and aarch64. Keyfile auto-unlock,
  mapper mount, bind mounts for `/data/{orchestrator,modules,wraith}`,
  and a tmpfs-backed fallback that keeps the wizard reachable when
  `/data` is unavailable. The previous x86 script had no LUKS path and
  the previous ARM script blocked forever on a `cryptsetup` passphrase
  prompt if the operator did not answer within the getty timeout.
- `genimage` configs: `data` partition no longer ships a preformatted
  ext4 image. The wizard formats the partition so LUKS can be applied
  at first boot.
- `kernel.fragment` (x86_64): add `BLK_DEV_DM`, `DM_CRYPT`,
  `CRYPTO_XTS`, `CRYPTO_AES`, and the user-space crypto APIs so
  `cryptsetup` works. The aarch64 fragment already had them.
- `inittab`: getty timeout raised to 86400 seconds on both arches.
  The password prompt timeout (60s) is hardcoded in BusyBox
  `/bin/login` and is not patched.
- `S41network-optional`: DHCP glob now matches `eth*` in addition to
  `en*`/`ens*` so virtio-net interfaces come up automatically.

### Notes
- Buildroot's `package/dropbear/dropbear.mk` is patched outside the
  repo with `DROPBEAR_CONF_ENV += ac_cv_search_crypt=-lcrypt`. musl's
  `crypt()` stub causes configure to resolve the symbol from libc and
  drop `-lcrypt`; without this fix password auth silently fails. The
  in-repo `S50dropbear` also sets `LD_PRELOAD` as a second guarantee.
- `libxcrypt` is now a dependency of dropbear on both arches.
- Images rebuilt and verified on 2026-11-12: wizard LUKS format,
  keyfile auto-unlock, dropbear password auth, eth0 DHCP, and SSH
  login all confirmed on QEMU aarch64 and x86_64 (fresh boot).

## [v0.42.2-arm-rauc-glib-fix] - 2026-11-10

### Fixed
- ARM image shipped a `rauc` binary that aborted at startup with
  `libglib-2.0.so.0: No such file or directory`. `post-build-arm.sh`
  removed libglib/libgio/libgobject/libgmodule after install on the
  assumption they were stale, but RAUC links against them. Image
  booted fine and `rauc status` never ran, so the regression was
  invisible until `rauc install` was attempted on ARM.
- Stopped removing the openssl CLI. It is ~1 MB, useful for
  inspecting bundles and CA certs on device, and not a dependency
  of anything removed by the same cleanup.

### Notes
- Rebuilds and re-releases the ARM image and bundle from v0.42.1.
  v0.42.1 assets remain but the ARM `.raucb` should be considered
  superseded.

## [v0.42.1-rauc-ca-prod] - 2026-11-09

### Fixed
- RAUC bundles could not be verified on device. The CA was present in
  the overlay source but never reached `output/target` because the
  build did not re-apply `BR2_ROOTFS_OVERLAY`, and `make-bundle.sh`
  fell back to a randomly generated self-signed CA when the
  repo-local path was empty.
- `make-bundle.sh` now reads `$CA_PROD` (default
  `~/backups-b/rauc-ca-prod`, overridable) and aborts on fingerprint
  mismatch. Removed the self-signed fallback and a dead `docker run`
  masked by `|| true`.
- Dev signing cert regenerated with
  `extendedKeyUsage=critical,codeSigning,emailProtection`. The
  previous cert only had `codeSigning`, which OpenSSL 3.x CMS
  rejects with "unsuitable certificate purpose".
- `wraithos_aarch64_defconfig` now pins the squashfs compression
  options (`SQUASHFS4_GZIP`, `BS="128K"`, `BS_128K`, `PAD`) that
  previously existed only in the generated `.config`. A fresh
  defconfig now produces the layout genimage expects.

### Added
- `deploy/wraithos/{x86_64,aarch64}/rootfs_overlay/etc/rauc/ca.cert.pem`:
  per-arch overlay copies of the production CA (SHA256
  `5C:75:AC:3B:...:FD:38`), tracked so a clean checkout reproduces
  the image.
- `.gitignore`: block `*.key`, `*.csr`, `*.srl`, `dev.cert.pem` under
  `deploy/wraithos/rauc-ca/`.

### Changed
- Production CA rotated. Replaces the previous release CA
  (`A9:CB:1C:42:...:C9:A1`).

### Notes
- Supersedes `v0.42.0-credentials-rauc-fix`. v0.42.0 tag stays, but
  its image artifact did not contain the CA or P0 #2 / P0 #3 fixes.
- ARM image fully rebuilt on 2026-11-09; `output-arm-m31/{build,host,
  target}` were missing.
- Bundles built locally, verified via `rauc info --keyring`, attached
  to the GitHub release. Not committed (`dist/` ignored).

## [v0.38.0-m10-metrics-cardinality] - 2026-11-08

### Documented
- Prometheus cardinality scaling. Baseline 546 B / 10 series; after
  20 modules x 3 tenants loaded and each executed once: 36 KB / 391
  series. Marginal ~1.8 KB / ~19 series per module. Scrape time
  stayed under 10 ms.
- Label lifetime is process-scoped (`metrics-exporter-prometheus`
  0.17.2): module or tenant deletion does not evict series; a
  restart clears the registry. Operator guidance (periodic restart,
  `max_modules` cap, downstream `metric_relabel_configs`) added to
  `docs/operations.md`.
- Deferred: per-delete label eviction (needs OTel metrics SDK or a
  dynamic label backend).

## [v0.37.0-m9-watchdog-lockdown] - 2026-11-08

### Fixed
- x86 inittab now launches `wraith-watchdog` (sysinit background +
  respawn), matching ARM. Softdog was previously never armed on x86
  because no userspace process opened `/dev/watchdog`.
- `wraith-watchdog` opens `/dev/watchdog` once (fd 9) and pets via
  the held fd; removes "watchdog did not stop!" dmesg spam
  (~12 lines/min caused by open+close without a magic `V`).
- x86 tmpfs mounts (`/dev/shm`, `/run`, `/tmp`, `/var/log`,
  `/var/tmp`, `/var/lib`) add `nosuid,nodev,noexec`, matching ARM.

### Changed
- `/root/.bashrc` now enforces a real shell lockdown: `HISTFILE=
  /dev/null`, `HISTSIZE=0`, `HISTFILESIZE=0`, `set +H`,
  `set -o noclobber`, readonly `PATH`, `umask 022`, `rm`/`cp`/`mv`
  aliased with `-i`, slot indicator in PS1.

## [v0.36.0-m8-boot-recovery] - 2026-11-08

### Fixed
- `S30data` now logs mount failure to `/dev/kmsg` (previously
  silenced with `2>/dev/null`) and attempts `fsck.ext4 -p` before
  giving up. Adds an actionable recovery hint and `restart`/`stop`
  handlers.
- x86 `post-image.sh` exports `PATH="${HOST_D}/bin:${PATH}"` so
  `genext2fs` is found by `genimage`'s subshell (same fix as ARM in
  v0.32.0).

## [v0.35.0-race-hardening] - 2026-11-08

### Fixed
- Concurrent `POST /modules` with the same id could delete the
  winner's `.wasm`: 50 parallel uploads all passed the pre-check,
  each wrote the file, then 49 losers removed it as rollback.
  Registry slot is now reserved via `insert_if_absent` before any
  disk write; losers return 409 without touching disk; a failed
  disk write rolls back the registry entry.
- `SIGTERM` hung for 15 s and ended in SIGKILL because the
  `derived_handle` awaited `event_bus.recv()` on a bus kept alive by
  the metrics server. The task is now aborted before metrics
  shutdown. `systemctl --user stop` completes in 0 s.

### Verified
- Corrupt `.wasm` on disk: loader logs `skip: load failed`, host
  healthz stays OK.
- Upload to read-only `modules/`: 500 `disk: Permission denied`,
  registry rollback, no leftover files.
- Corrupt `orchestrator.db`: actionable `file is not a database`,
  no crash loop; restore DB recovers.
- `rate_limit_per_sec=5`: 20 concurrent executes = 5 x 200 / 15 x
  429; metric `tenant_quota_rejected_total{reason=rate}=15`.

## [v0.34.0-m4-tenant-isolation] - 2026-11-08

### Fixed
- `auth_middleware` hardcoded the default tenant's token. `/t/
  {tenant}/...` routes rejected legitimate tenant tokens before the
  handler's `verify_tenant` ran. Middleware now extracts the tenant
  from the path and verifies against that tenant; non-`/t/` routes
  fall back to default.
- Unknown tenant in path returns 401 (not 404) to avoid tenant
  enumeration.
- `wizard.rs` test module moved to end of file (clippy
  `items after a test module`).

### Added
- 6 integration tests pinning multi-tenant middleware behavior.

## [v0.33.0-adversarial-hardening] - 2026-11-08

### Fixed
- Outer HTTP timeout raised from 60 s to 65 s. A spinning guest
  previously hit the epoch trap at 60 s but the outer layer fired
  simultaneously and returned a bodyless 408. Now the trap path
  completes and returns a clean 500 with trap body.

### Added
- `tests/fixtures/adversarial/` covering panic, bad JSON, memory
  bomb, CPU spin, capability violation, TCP violation. Standalone
  crate (not a workspace member); source tracked, `.wasm`
  gitignored.

## [v0.32.0-arm-parity-hardening] - 2026-11-08

### Fixed
- ARM image boots cleanly to orchestrator: `/etc/wraith/` directory
  restored with `.gitkeep`; `S30data` uses `-s` (not `-f`) for the
  config seed; `S99wraith-init` uses `>>` (not `>`) so busybox
  `noclobber` does not block restart.
- ARM `post-image-arm.sh` exports `PATH` so `genext2fs` is found.
- ARM libglib2: `.files-list-target-installed` tracking rebuilt;
  `.so` present in rootfs so RAUC loader resolves.
- x86 `S30data` gets a tmpfs fallback for `/etc/wraith`.

## [v0.31.0-rauc-arm-parity] - 2026-11-07

### Added
- ARM RAUC bundle install end-to-end on TCG QEMU.
- ARM defconfig enables `LIBGLIB2`, `RAUC`, `RAUC_NETWORK`,
  `LIBUBOOTENV`, `HOST_GENEXT2FS`.
- ARM `genimage-arm.cfg` gets a `data.ext4` image block so `/data`
  is a writable ext4 partition.
- ARM `post-image-arm.sh` writes a full RAUC-compatible grubenv
  (`A_OK`, `B_OK`, `A_TRY`, `B_TRY`, `ORDER`).
- ARM `grub-arm.cfg` selects slot from `ORDER`.
- `wraith-bootenv` (both arches) preserves unknown grubenv keys so
  `wraith-bootenv demote` does not corrupt RAUC state.

### Known debt
- SHA512 login on ARM rejected despite byte-identical shadow between
  slots. Workaround: shadow root hash cleared, passwordless. Fix
  requires busybox crypt config with musl libc.

## [v0.29.0-rauc-ab-update] - 2026-11-07

### Added
- RAUC bundle install writes the bundle rootfs to the inactive slot,
  updates `ORDER` in grubenv, and the reboot switches slot per
  `ORDER`. `rauc status` and `rauc status mark-good` verified.

### Fixed
- `system.conf`: slot type `squashfs` -> `raw` (RAUC 1.11 rejects
  `squashfs`); grubenv path `/grub/grubenv` ->
  `/boot/grub/grubenv`; `data-directory=/data`.
- `S06rauc-mnt` mounts tmpfs on `/mnt` and `/run/rauc`; RAUC needs
  writable mount points for the bundle loop device.
- `grub.cfg` reads `ORDER` for slot selection (fallback to legacy
  `slot` variable).

## [v0.28.0-m26-wraithos-x86arm] - 2026-11-07

### Added
- WraithOS x86_64 Buildroot build (BIOS GRUB i386-pc, musl, kernel
  6.6, squashfs, dual-slot A/B, RAUC + libubootenv, bash).
- WraithOS aarch64 Buildroot build (EFI GRUB arm64-efi).
- `deploy/wraithos/` board files, defconfigs,
  `sync-wraith-init.sh`.
- `wraith-bootenv`, `wraith-bootcheck`, `wraith-update`,
  `S05bootstate`, kernel `CONFIG_SOFT_WATCHDOG`.

### Fixed (bundled)
- Wasm pool acquire timeout returns 503 (was 500) via
  `OrchestratorError::WasmPoolTimeout` and
  `QuotaExecError::PoolTimeout`.
- Persist raw 64-byte ed25519 signature to `.wasm.sig` on upload /
  replace; loader verify survives restart under
  `trusted_signer_keys`.
- `trusted_signer_keys` moved out of `[script]` section (was
  silently parsed as `script.trusted_signer_keys`, a nonexistent
  field, so enforcement was effectively off despite config).
- `bwrap` startup check is a functional probe (`--unshare-all`
  attempt), not just `--version`.
- `POST /modules/build` explicitly rejects `X-Module-Capabilities`
  (build path hardcodes global caps; fail loud).
- `wizard.rs` writes are fsync-backed.
- `S41network-optional` matches `eth*` in addition to `en*`.
- x86 `orchestrator.toml` binds `0.0.0.0:9090` for host
  port-forward.

## In progress

Open items from the hardening plan:

- M1: repository hygiene. Remove the legacy `wraithos/` tree; a
  19 MB binary is currently in git history (repo size ~18 MiB). Plan
  is to rewrite history once; not yet executed.
- M2: CI matrix. ARM build not currently in CI. Integration tests
  hit `example.com` over the network; needs a fake HTTP fixture so
  CI runs offline.
- M11: docs sweep (this release). Remaining: benchmark CI capture,
  docs site deploy target.
- M12: end-to-end release artifact. Tarball + SHA256SUMS + RAUC
  bundle published alongside tag.
