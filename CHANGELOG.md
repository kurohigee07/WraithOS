# Changelog

All notable changes to this project. Releases are tagged
`v0.<minor>.<patch>-<slug>`. The slug retains the milestone code
(`mNN`) when the release closes a specific roadmap item; otherwise it
describes the change theme. `v0.30` was not released.

Crate manifests declare `0.1.0`; the tags are the release units.
Releases before `v0.24.0` are not itemized here; see commit history.

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

Open items from the hardening plan (see `TODO.md`):

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
