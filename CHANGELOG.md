# Changelog

All notable changes to this project. Releases are tagged
`v0.<minor>.<patch>-<slug>`. The slug retains the milestone code
(`mNN`) when the release closes a specific roadmap item; otherwise it
describes the change theme. `v0.30` was not released.

Crate manifests declare `0.1.0`; the tags are the release units.
Releases before `v0.24.0` are not itemized here; see commit history.

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
