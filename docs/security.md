# Security model and known limitations

**Audience:** anyone considering a non-loopback bind, shared deployment, uploaded source, or customer-facing plugin service.

Wraith has useful defensive mechanisms, but the current implementation is best understood as a **single-operator runtime with namespace/resource tenancy**, not an independently audited hostile multi-tenant platform. This page describes what the code does and does not guarantee.

## Trust boundaries

| Boundary | Implemented control | What it does not establish |
|---|---|---|
| Guest → host | Component model, explicit WIT imports, capability checks, Store memory limit, epoch deadline | A secure boundary for every WASI/host resource or a total process RSS/CPU budget |
| Caller → API | Optional Bearer middleware, per-tenant token verification | Roles, tenant-wide isolation of admin endpoints, or safe tokenless tenants |
| Tenant → tenant | `(tenant,id)` registry keys, directories, optional resource caps | Separate process/kernel/filesystem/event/audit isolation |
| Source → compiler | Opt-in route, Bubblewrap namespaces, read-only toolchain, timeout, one-build semaphore | Audited arbitrary-source execution or complete archive/resource sandboxing |
| Artifact → runtime | Optional raw Ed25519 verification | Complete signature persistence across every ingestion path |
| OS update → boot | Read-only rootfs; ARM trial/commit/watchdog scripts | Verified boot, signed rootfs updates, or transactional power-loss safety |

## Recommended local posture

1. Keep `metrics_bind="127.0.0.1:9090"`.
2. Leave `enable_build=false` and `enable_build_network=false`.
3. Upload only components you built or trust.
4. Use explicit outbound host/port permissions; avoid `*` outside a demo.
5. Keep token plaintext, signing private keys, and appliance password material out of Git.
6. Use an OS service account and resource limits appropriate to the host.
7. Before remote access, place the service behind a correctly configured TLS proxy and firewall, and resolve the authorization limitations below.

Binding to `0.0.0.0` is not a secure “publish” switch. The same listener carries module administration, metrics, audit, debug state, WebSocket, and wizard routes.

## Authentication is not tenant isolation

With `require_auth=false`, a tenant without a token passes handler verification without credentials. This is intentional development behavior.

With `require_auth=true`:

1. Middleware checks the default tenant's token for every route except exact `/healthz` and `/readyz`.
2. A named-tenant handler checks that tenant's token again.
3. If the named tenant has no token, its handler allows the request after default-token middleware.
4. If it has a different token, the request cannot supply two different Bearer values in the current design.

Therefore the default token effectively grants access to tokenless named tenants and global operator endpoints. This may suit one administrator; it is not a customer-safe permission model.

Global `/audit/logs`, `/debug/state`, `/metrics`, UI fragments, wizard routes, and `/ws/events` are not tenant-filtered. The module list route is tenant-specific, but that alone does not create an information-security boundary.

**Operational implication:** do not give mutually untrusting customers access to the same instance under the assumption that tenant tokens isolate all data and resources.

## Capabilities are authorization data

The global `[[capabilities]]` list is a fallback. An uploader can provide `X-Module-Capabilities` to replace it, including broader permissions. The code does not intersect the requested set with a global maximum.

Anyone allowed to upload effectively participates in granting host permissions. Keep upload access operator-only until a separate policy-authorization layer exists.

An empty capability sidecar/manifest falls back to global permissions on reload. `[]` is not a durable deny-all. A network-free guest plus controlled deployment is safer than relying on an empty manifest for isolation.

### HTTP-specific risks

The HTTP check is hostname-based. It does not restrict schemes to HTTPS, block private/link-local IP ranges, pin DNS answers, or cap response-body size. Redirects are disabled, which removes one bypass class but not DNS rebinding or an allowlisted host that serves internal content.

Use network egress controls for SSRF defense. Treat `allowed_hosts=["*"]` as broad outbound GET authority.

### TCP-specific risks

Only ports are configurable through the current capability type. There is no destination-host allowlist in that public schema. Socket ownership uses module ID rather than tenant-qualified identity, and handles can outlive a Store unless closed. Same-ID modules across tenants are not strongly separated at this registry layer.

### Unused capability surfaces

`db-query` has a shared enum/checker but no WIT database import. `log` controls Rhai logging, not a guest logging import. Their presence in a manifest should not be mistaken for implemented guest functionality.

## Execution resource limits

Wasm execution is bounded by three independent mechanisms:

1. **Epoch deadline** (`EPOCH_DEADLINE_TICKS = 600` at 100 ms per
   tick = 60 s wall clock). Enforced by a background ticker that
   bumps the Wasmtime epoch; a guest that overruns traps cleanly.
2. **Fuel budget** (`MAX_FUEL_PER_EXECUTION = 100_000_000`
   instructions, `config.consume_fuel(true)`). Enforced per
   execution; a fresh `Store` is created for each `execute` call so
   the budget is not shared. Empirical consumption on
   `modules/default/*.wasm` (2026-11-14): `empty` ~1.2K,
   `adversarial` ~2.7K, `sample` ~20K, `json_heavy` ~34K,
   `cpu_heavy` ~1.59M. 100M is ~63x over the heaviest legit module
   and bounds worst-case CPU to a few hundred milliseconds. Fuel
   exhaustion surfaces as
   `guest exceeded fuel budget of N instructions`.
3. **Memory limit** (per-module `max_memory_mb`, capped at the
   global `wasm_memory_limit_mb`). `StoreLimitsBuilder` rejects
   allocation past the limit.

Rhai scripts (the other embedded runtime) do not have a fuel or
instruction budget in this release. A Rhai script with an infinite
loop blocks one event-worker thread until the process is killed.
Fix deferred to v0.52; mitigation is to run Rhai only on
operator-controlled scripts (`docs/wraithos.md`).

The HTTP capability (`http::get`) caps the response body at
`MAX_HTTP_BODY_BYTES = 16 MiB`, enforced both on `Content-Length`
(fast path) and on the streaming read (`bytes_stream()` cumulative
total, decompressed). See `wasm/host_impls.rs` and
`CHANGELOG.md` for the M2 entry.


Implemented:

- Global semaphore: default 2 concurrent executions.
- Optional tenant/module concurrency semaphores.
- Optional tenant/module token buckets.
- Default 256 MiB Wasm linear-memory ceiling per Store.
- Approximate 60-second epoch deadline.
- Default 256 global TCP handles.
- Fixed 60-second HTTP timeout and 30-second outbound HTTP timeout.

Not equivalent to a complete container sandbox:

- Linear-memory limits do not bound all host/JIT/DB/network allocations.
- Epoch time is not per-module CPU accounting and cannot be configured per manifest in this checkout.
- TCP read/connect behavior lacks a dedicated configurable operation timeout.
- Rhai runs trusted scripts without an explicit operation/fuel/memory limit.
- Shared process failure can affect all tenants; release builds use `panic="abort"`.
- SQLite/event/dashboard work shares the same service resources.

Use host-level cgroups/systemd limits and load tests for deployment capacity. Do not infer safe tenant density from an empty-guest microbenchmark.

## Signatures

Two different signing systems exist:

1. **Wasm module signatures:** raw Ed25519, 64-byte signature, 32-byte hex public key.
2. **Release signatures:** GPG signature over SHA256SUMS and detached tarball signatures.

They are not interchangeable.

When `trusted_signer_keys` is nonempty, disk loading requires a valid `.wasm.sig`; raw upload requires `X-Module-Signature`. However:

- Raw upload does not write the supplied signature to a sibling file.
- Source build does not enforce the signature header or produce a signature sibling.
- Delete does not reliably remove signature/capability siblings.
- Trusted-key hex is decoded during verification, not prevalidated as a complete startup policy. An invalid hex entry returns an error and stops that verification attempt; wrong-length or invalid key bytes are skipped with warnings.

A module accepted over HTTP may therefore disappear after restart when signature enforcement is active. Prefer an offline build/sign/deploy workflow that atomically stages the component and raw signature, then verifies startup loading. Do not claim end-to-end signed ingestion until these paths are fixed and tested.

The repository contains a release **public** key. It does not establish that every binary in the tree was signed by that key. Verify the actual release manifest and out-of-band fingerprint.

## Source-build sandbox

The current code is substantially more defensive than the old TODO description: builds are disabled by default and Cargo runs through Bubblewrap when enabled. System/toolchain paths are read-only; home/tmp are tmpfs; namespaces are unshared; network is off unless explicitly enabled; one global build slot is allowed.

Remaining concerns include:

- Tar extraction occurs outside the Bubblewrap Cargo invocation and
  relies on the external tar implementation's path/link behavior.
  Confirmed by the 2026-11-14 audit: `extract_tar` calls
  `Command::new("tar")` before any bwrap invocation, with no
  entry filtering (no rejection of `..`, absolute paths, or
  symlink entries). On WraithOS the target `tar` is BusyBox
  (`/bin/tar -> busybox`), which is not guaranteed to sanitize
  `..` in the same way GNU tar does. Exposure was gated by
  `enable_build = false` (build API only mounted when explicitly
  enabled) and by `require_auth` (off by default). **Fixed in
  commit `d160b43`:** the external tar invocation was replaced
  with the `tar` crate. Every entry is filtered before any
  filesystem write: `..` components, absolute paths, and Windows
  prefixes are rejected, and only regular files and directories
  are extracted (symlinks, hardlinks, device nodes, fifos are
  rejected). Three tests cover the accept path, the
  parent-traversal path, and the symlink path. The build API is
  still off by default; the filter narrows the blast radius if an
  operator enables it.
- Archive expansion and compiler memory/CPU/disk consumption need stronger explicit budgets.
- Network-enabled builds can execute build scripts/proc macros with outbound access.
- A read-only offline cache must be prepared correctly; dependency resolution is not magically available offline.
- The 60-second HTTP timeout differs from the 300-second compiler timeout.
- Build artifact signing/capability persistence is incomplete.

Keep this route restricted to trusted operators. “Sandboxed” does not mean “safe to accept arbitrary public source without further review.”

## Dashboard and wizard

The dashboard serves HTMX locally and escapes server-generated HTML table fields. However, its WebSocket message handler inserts event kind/source/payload into `innerHTML` without equivalent escaping. Event payloads can originate from permitted plugins. Treat the dashboard as an operator surface and do not assume all event content is safe HTML.

The UI hard-codes `ws://` and does not attach Bearer tokens to fetch/HTMX requests. HTTPS and authenticated deployments require UI changes or a carefully designed proxy session-auth layer. There is no built-in CSRF token/CORS hardening profile; browser exposure deserves a separate review.

The wizard stores an Argon2 admin-password hash, hostname, and network TOML, but this checkout does not show a complete path that applies those values to OS login, API Bearer configuration, network activation, or an actual reboot. The HTML does not expose the optional encryption-password field even though the backend accepts it. Do not infer that completing the web form secures console login or rotates runtime credentials.

## Appliance credential and boot material

The ARM tree contains a tracked `board/wraithos/etc/shadow` file outside the rootfs overlay. Its contents are intentionally not reproduced here. Review whether that build-time credential material belongs in Git, whether it enters an image, and rotate/remove it through a deliberate security change before public distribution. This documentation update does not alter credentials.

The ARM shell profile disables history/core dumps and fixes PATH, but these are usability/hardening settings, not a substitute for authenticated console access. `root` is the configured shell account. Read-only rootfs does not mean verified boot or encrypted persistent data.

## WraithOS update and encryption caveats

- x86 uses a simple `boot_b` marker; no automatic failed-boot counter is implemented there.
- ARM uses GRUB env plus watchdog proof, but `wraith-update` does not cryptographically verify the supplied rootfs image.
- ARM updater writes only the inactive SquashFS slot; it does not replace the ESP's `Image-a`/`Image-b` kernels. Kernel updates require a separate coordinated procedure.
- GRUB environment rewriting uses a temporary runtime file followed by overwrite plus sync; it is not proven power-loss atomic at the storage layer.
- Watchdog health proof is stronger than “process exists,” but its logic and grace behavior need hardware/failure-injection validation.
- LUKS setup targets hard-coded `/dev/vda4` and formats it. A wrong device/layout or populated partition can cause irreversible data loss.

See [WraithOS](wraithos.md) for exact architecture differences and recovery procedures.

## Data handling and persistence

SQLite stores execution status/errors, not a designed durable queue of guest payloads. Error strings can still contain sensitive guest/host details. Restrict audit/debug/log access and define retention; the application does not automatically prune audit rows.

The event bus is global, in-memory, and lossy. WebSocket clients see events across tenants. Do not publish secrets in event payloads on a shared instance.

Individual disk writes use fsync/rename, but module + sidecars + registry are not one transaction. Backups use SQLite `.backup` when the sqlite3 CLI is installed; the fallback file copy is explicitly not transactionally consistent.

## SSH authentication

Dropbear runs on port 22 inside the guest. It is a backup entry point
behind the serial console, not the primary interface.

- **Password auth (default).** Enabled until the wizard receives a
  public key. Same credential as the serial console. Suitable for
  development and for recovery when the operator has not yet enrolled
  a key.
- **Pubkey-only auth.** When the wizard step 1 `ssh_pubkey` field is
  non-empty, `finish_handler` writes `/etc/wraith/authorized_keys`
  (persisted on the LUKS volume) and a marker file
  `/etc/wraith/ssh-pubkey-only`. On the next boot `S50dropbear`
  stages the key into `/root/.ssh/authorized_keys` (tmpfs) and
  starts dropbear with `-s`, disabling password auth for SSH.
- **Serial console password never changes.** The Linux root password
  in `/etc/shadow` is independent of dropbear. A lost SSH key can be
  recovered by logging in on the serial tty and clearing
  `/etc/wraith/ssh-pubkey-only` (or by re-running the wizard from a
  fresh image). There is no SSH-only lockout.
- **Pre-wizard gate (v0.51+).** `S50dropbear` refuses to start
  until `/var/lib/orchestrator/wizard.complete` exists. This closes
  the pre-wizard window during which the documented default root
  password was reachable on port 22 before the operator had
  provisioned anything. The gate is lifted by `finish_handler`
  (wizard.rs) restarting `S50dropbear` at the end of the same
  wizard run; no reboot required. Serial console remains
  password-authenticated with the same default throughout - it is
  the primary provisioning interface and the documented recovery
  path. Pre-v0.51 images do not have this gate; see CHANGELOG
  v0.51.0 for the rationale and the attack window.

Known gaps:

- Host keys are ephemeral. Every reboot produces a new SSH host key;
  clients will see a `known_hosts` mismatch. This is a deliberate
  trade-off — `/data` is LUKS-encrypted and `/etc` is a read-only
  squashfs, so there is nowhere persistent to store the host key
  that survives a factory reset without adding a second secret.
- SSH bind address is `0.0.0.0:22`. There is no built-in firewall
  rule restricting SSH to a management network; the operator is
  expected to configure `iptables` or the upstream hypervisor.

## Threat model — physical access

The current release stores the LUKS keyfile for `/data` at
`/boot/luks.key` on the unencrypted ESP so the device can auto-unlock
on boot. An attacker with physical access to the disk can extract
that keyfile and decrypt `/data` offline. This is a real limitation,
not a theoretical one.

### TPM 2.0 sealing (shipped in v0.49.0)

The wizard seals `/boot/luks.key` to a TPM 2.0 device under a PCR
policy (`sha256:0,1,2,3,4,5,6,7`) whenever one is present. The sealed
blob is written to `/boot/luks.sealed.{pub,priv}` next to (not
replacing) the plaintext keyfile. On every boot `/etc/init.d/S30data`
attempts TPM unseal first; only if it fails does it fall back to the
plaintext keyfile.

PCR selection by architecture:

- **ARM aarch64.** PCR 0-7 (edk2 firmware code/config, GRUB EFI
  binary, grub.cfg, Secure Boot state). Verified stable across cold
  boots on QEMU `QEMU_EFI.fd`. Protects against disk theft to a
  different TPM and against tampering with the GRUB binary/config.
  Does **not** cover U-Boot FIT measured boot (no U-Boot on this
  image); tracked as v0.50 roadmap.
- **x86_64.** Same PCR set; verified stable on BIOS GRUB with
  SeaBIOS 1.16.3 and `-cpu max`.

Kernel support: `CONFIG_TCG_TPM=y` + `CONFIG_TCG_TIS=y` builtin on
both arches (ARM additionally `CONFIG_TCG_TIS_SPI=y`). The kernel
resource manager `/dev/tpmrm0` is the TCTI endpoint
(`device:/dev/tpmrm0`); `tpm2-abrmd` is not shipped.

**v0.49.0 policy: sealed + plaintext coexist.** The plaintext
keyfile is **never** auto-removed. Auto-removal requires an explicit
operator command (`wraith-tpm-finalize`) after re-verifying that the
sealed blob unseals successfully in the current boot. This is a
data-safety boundary: accidentally removing the only fallback would
brick `/data` on the next PCR change (kernel/firmware update, TPM
clear). Auto-removal on first successful unseal is deferred to
v0.50.0 after the TPM path has survived a production data cycle.

**Recovery procedures.** See `docs/wraithos.md` → "TPM 2.0 sealing"
for the operator-facing runbook (how to re-seal after a PCR change,
how to recover when the TPM has been cleared externally, how to
revert to plaintext-only mode). The kernel/initramfs update procedure
requires `wraith-tpm-prepare-update` **before** `rauc install` while
plaintext has been finalized away.

Mitigations still planned for v0.5x:
- **Network-bound disk encryption.** Clevis/Tang with a Tang server
  on a trusted network, optionally bound to a second factor. Requires
  a Tang endpoint and a recovery keyfile printed at setup time. This
  is the recommended path when the device is reachable over a
  management network.
- **Recovery key.** Regardless of sealing method, print a recovery
  keyfile once at wizard completion for offline backup. Current
  release has no recovery path beyond `POST /wizard/reset-luks`
  (which destroys the volume).

## Boot chain integrity

The current release does not verify the boot chain cryptographically.

- **No UEFI Secure Boot.** GRUB, the kernel, and the initramfs are
  unsigned from the firmware's point of view. An attacker with
  physical access can substitute any of them and the firmware will
  load it. This invalidates the LUKS protection above: a modified
  initramfs can log the decryption keyfile at boot.
- **No ARM Verified Boot.** The aarch64 image uses the same unsigned
  GRUB + kernel chain. There is no U-Boot FIT signing.
- **dm-verity is applied to RAUC bundles only.** The `format=verity`
  bundle verifies the rootfs squashfs against a signed root hash
  during `rauc install`. This is a **transport** integrity check
  (bundle → slot), not a **runtime** integrity check. The rootfs
  itself is mounted from the raw slot with no dm-verity layer, so
  post-install tampering with `/dev/vda2` or `/dev/vda3` is not
  detected at boot.

Mitigations planned for v0.5x:

- UEFI Secure Boot with a per-device key pair (PK, KEK, db). Sign
  GRUB and the kernel during build; enroll the custom keys in
  firmware at first deployment. Requires build-time key management
  and either a cloud KMS or a hardware token for the private keys.
- ARM Verified Boot with U-Boot FIT signing. Requires replacing the
  current EFI-GRUB path on aarch64 with a U-Boot verified-boot
  chain.
- Runtime dm-verity on the rootfs slot: add a small initramfs that
  opens the slot squashfs under a dm-verity table backed by a signed
  root hash stored in the boot partition, then pivots to it. This
  closes the "attacker replaced the raw slot after install" gap but
  does not close the "attacker modified the kernel" gap without
  Secure Boot.

### UEFI Secure Boot (shipped in v0.50.0, x86_64 only)

**What is verified at boot:**

1. **Firmware verifies GRUB EFI binary.** The board is provisioned with
   device-specific Secure Boot keys (PK, KEK, db) generated by
   `deploy/wraithos/uefi/gen-sb-keys.sh`. Microsoft's default keys are
   explicitly excluded (`--no-microsoft`). `grubx64.efi` is signed with
   the db key at build time (`sbsign`) and verified by OVMF at
   `LoadImage`.
2. **Firmware verifies the Linux kernel.** GRUB passes the signed
   `bzImage` to the firmware `LoadImage` protocol via the EFI stub
   loader. OVMF verifies the kernel PE signature against db before
   handover.
3. **Kernel confirms Secure Boot state** (`dmesg | grep -i "Secure
   boot enabled"`); `/proc/cmdline` retains the build-time `rootwait`
   argument.

**GRUB patch context.** Upstream GRUB 2.12 refuses to load kernel
modules when Secure Boot is active unless Microsoft's shim
(`shim_lock` protocol) is present, even for the kernel itself, which
this image does not ship (Microsoft signing process is weeks-long and
per-release). Two small local source patches bypass the shim-lock
requirement and use the EFI stub loader directly:

- `grub-core/loader/efi/linux.c`: `if (grub_is_shim_lock_enabled())`
  forced to `if (0)` so GRUB does not fall back to the legacy loader.
- `grub-core/kern/efi/sb.c`: `shim_lock_verifier_write` returns
  `GRUB_ERR_NONE` when the shim protocol is absent, so `grub_file_open`
  does not reject the (already firmware-verified) kernel.

Both patches live in `deploy/wraithos/grub-patches/` and are applied
at GRUB build time. They do not weaken the firmware's verification of
`grubx64.efi` or of `bzImage`; they only remove the shim requirement
so the EFI stub loader can run.

**What is NOT verified at boot (honest gaps):**

- **Rootfs tamper.** `/dev/vda2` and `/dev/vda3` are mounted from the
  raw slot with no runtime dm-verity layer. An attacker with raw disk
  access can replace the squashfs and the kernel will boot it. This
  gap exists on the BIOS path too; Secure Boot narrows the attack
  surface to "post-install rootfs write" without closing it.
- **IMA appraisal.** Not enabled. File-level integrity inside the
  rootfs is not verified at runtime. Deferred to v0.51.
- **Hardware validation.** All testing is QEMU OVMF + KVM. Physical
  board firmware quirks (variable store size, TPM interaction,
  measured-boot events) are not exercised. Section is
  "QEMU-verified, hardware validation pending v0.51" for this reason.

**v0.50.0 test matrix (QEMU OVMF, KVM):**

| Test  | Scenario                                        | Result |
|-------|-------------------------------------------------|--------|
| T-SB0 | BIOS path, unsigned image                       | boots, healthz OK, no SB |
| T-SB1 | SB on, custom PK, signed image                  | boots to login, `Secure boot enabled` |
| T-SB2 | SB off, unsigned image                          | boots |
| T-SB3 | stock OVMF (MS PK), SB on, signed image         | firmware refuses (`Access Denied`) |
| T-SB4a| custom PK, tampered unsigned GRUB               | firmware refuses (`Access Denied`) |
| T-SB4b| custom PK, signed GRUB                          | boots to GRUB |

**Build-time key requirement.** `WRAITH_SB_KEYS` must point at a key
directory generated by `gen-sb-keys.sh`. Without it, `post-image.sh`
builds the legacy BIOS-only image and prints a loud warning. Production
releases must set the variable. The key directory is never committed.
Keys are device-specific: losing the db private key prevents future
signed updates; leaking it lets an attacker sign images that pass
Secure Boot.

**Deferred to v0.51:** physical hardware validation, IMA appraisal
policy, Secure Boot on aarch64 (requires ARM SBBR / U-Boot FIT
measured boot which the current EFI-GRUB aarch64 image does not
provide).

### Build-process post-mortem (v0.50.0 Secure Boot key loss)

v0.50.0 shipped correctly signed images. The signing key material
that produced them was lost in a build-process incident after the
image was built. This section documents what happened, why the
release remains valid despite it, and what changed as a result.

**Timeline.**

1. `deploy/wraithos/uefi/gen-sb-keys.sh` was executed with its
   default output directory. At the time the default was
   `/tmp/sb-keys`.
2. The script contained an inverted safety check: it printed a
   warning when the output path was persistent (`$HOME/sb-keys`)
   and silently accepted the volatile path (`/tmp/sb-keys`). The
   intended behaviour was the opposite. The bug meant the operator
   was not alerted that the keys were about to be written to a
   directory that `/tmp` cleanup on most hosts (including the
   build VPS) removes.
3. The image was built and signed. `sbverify --cert db.crt`
   returned `Signature verification OK` for both `bzImage` and
   `grubx64.efi` in the resulting ESP. PE signatures are
   self-contained; they do not depend on the private key remaining
   available after signing.
4. `/tmp` was wiped (either by a reboot or by cleanup). `db.key`,
   `PK.key`, and `KEK.key` were lost. `db.crt` (public) survived
   inside the signed PE binaries as the embedded signer
   certificate.

**What remains valid.**

- The v0.50.0 image boots under Secure Boot when the target
  firmware has `db.crt` enrolled. This was verified with QEMU
  OVMF and custom PK/KEK/db provisioned for the release (see the
  test matrix above).
- `db.crt`, `bz.pkcs7`, `grub.pkcs7`, `bz.chain.pem`,
  `grub.chain.pem`, and `SHA256SUMS` are published as release
  assets. Their SHA-256 fingerprint matches the certificate
  embedded in the image; independent verification with `sbverify`
  succeeds.
- Existing v0.50.0 devices continue to accept RAUC bundle updates
  from later releases. RAUC bundles only write to the rootfs
  slots (`/dev/vda2`, `/dev/vda3`); they do not touch the ESP.
  The signed `grubx64.efi` and `bzImage` on the ESP remain valid
  under the enrolled `db.crt` indefinitely.

**What no longer works.**

- A new image signed under the v0.50.0 certificate cannot be
  produced. `db.key` is gone.
- A v0.51+ image signed with a *different* key cannot boot on a
  v0.50.0-provisioned device without re-enrolling the new
  certificate. Migration steps are in `docs/wraithos.md` ->
  "Certificate migration v0.50.0 -> v0.52+".

**Why this is not a compromise.**

A lost private key means the key cannot be used by anyone. A
leaked private key means an attacker can produce a signed image
that would pass firmware verification. The two cases have
opposite security consequences:

- Lost: the set of signed images is frozen. No new image can be
  produced that trusts against the enrolled `db.crt`. No attacker
  capability is added.
- Leaked: the trust anchor is defeated. An attacker can sign a
  modified `grubx64.efi` that loads an attacker kernel, and the
  target firmware will accept it. Remediation requires revoking
  the old certificate via `dbx` (destructive, bricks existing
  devices) or re-enrolling new keys on every device.

Because v0.50.0 is a *lost key* case, no revocation action is
required. The enrolled `db.crt` continues to verify only the
specific PE signatures that were produced before the loss.

**Fix (commit `aae2af7`).**

`gen-sb-keys.sh` now:

- Defaults its output to `$HOME/sb-keys`, not `/tmp/sb-keys`.
- Refuses to write under `/tmp/` or `/var/tmp/` unless the caller
  passes `--force-dev` as the first argument. The refusal is a
  hard `exit 1`, not a warning.
- Prints a five-second pre-write pause with a checklist on
  persistent paths: "encrypted USB x 2", "password-manager
  entry", "copy to offline cloud storage", "rotation policy
  documented". The operator can Ctrl-C before any key is
  generated.
- Prints the SHA-256 fingerprint of each generated certificate
  on completion so the operator can record it alongside the
  password-manager entry.

The fix landed on `main` after the v0.50.0 tag. The tag itself is
not modified. v0.51.0 images are unsigned (BIOS-only) by design:
the operator decision was to not attempt signing with unrecoverable
keys and to defer per-device keys to v0.52+.


## Update verification and rollback

RAUC installs to the inactive slot and marks it active. The current
release **does not confirm health before committing the trial boot**.
If the new slot boots far enough to start `init` but the orchestrator
never becomes reachable on `/healthz`, the system stays on the failing
slot across the remaining trial attempts and then rolls back to the
last-known-good slot via the existing GRUB trial counter. This is a
kernel-level rollback, not a service-level rollback: a new slot that
boots the kernel and init but leaves the orchestrator dead will still
be marked good by the bootloader, and the operator has to intervene.

Mitigations planned for v0.5x:

- S99wraith-init extension: after starting the orchestrator, poll
  `http://127.0.0.1:9090/healthz` for up to 30 s. On success, run
  `rauc status mark-good`. On timeout, run `rauc status mark-bad`
  and `reboot` so the next trial attempt falls through to the
  previous slot.
- The watchdog (`softdog` via `wraith-watchdog`, `soft_margin=15`)
  already provides a last-resort reboot if the orchestrator hangs.
  It does not distinguish between "hung" and "not yet started", so
  it is a coarse signal and cannot replace health-based mark-good.

## Deployment decision guide

| Use case | Current recommendation |
|---|---|
| Personal localhost runtime | Reasonable starting point with trusted modules and conservative capabilities |
| One operator serving their own integrations | Possible after auth, proxy, backup, resource-limit, and restore validation |
| Trusted teammates uploading prebuilt modules | Restrict administration; document signature/build gaps and audit access |
| Public arbitrary source compilation | Do not expose without additional sandbox/queue/resource/security work |
| Mutually untrusting paying tenants | Require stronger authz, tenant-scoped observability/events/sockets, and process/instance isolation first |
| Security-critical appliance fleet | Add signed/verified updates, credential provisioning, device identity, and failure-injection testing first |

## Priority hardening backlog

This is a documentation-derived recommendation, not a claim that these changes were implemented:

1. Define operator/admin versus tenant roles and tenant-filter every sensitive surface.
2. Make global capability policy an upper bound; persist explicit deny semantics.
3. Scope socket ownership and event/audit visibility by tenant and execution.
4. Persist and verify signatures consistently across upload/build/restart/delete.
5. Add archive validation, compiler resource budgets, and a durable build job model.
6. Add Rhai limits, outbound response limits, egress controls, and shutdown drain tests.
7. Complete wizard-to-OS credential/network/reboot integration and remove tracked secret material.
8. Validate ARM boot/update/encryption through power-loss and failed-boot drills; sign update artifacts.

No security audit or penetration test was performed for this documentation update.

## Default credentials

WraithOS ships with a documented root password for serial console
login: `wraithos-admin-change-me`. It is set as a SHA512-crypt hash in
each board overlay's `etc/shadow` (x86 at
`deploy/wraithos/x86_64/rootfs_overlay/etc/shadow`, ARM at
`deploy/wraithos/aarch64/rootfs_overlay/etc/shadow`). Busybox `login`
verifies it against the hash at each console login.

Why a documented default instead of per-device random generation:

- The rootfs is a read-only squashfs. `/etc/shadow` cannot be modified
  at runtime by `passwd` (returns "Read-only file system"). Any
  per-boot random generation would need a writable bind mount over
  `/etc/shadow` from the persistent `/data` partition; the
  implementation was attempted and dropped after repeated
  instability (bind mount on read-only squashfs, inode replacement
  semantics).
- The console is physically isolated: `getty` binds only to the
  serial port (`ttyS0` on x86, `ttyAMA0` on ARM). The
  network-facing login (dropbear, port 22) is gated to post-wizard
  as of v0.51: `S50dropbear` refuses to start until
  `/var/lib/orchestrator/wizard.complete` exists. Pre-v0.51 images
  did not have this gate; the exposure is closed. See "SSH
  authentication" above.
- Every commercial appliance OS ships a documented default
  (OpenWrt `root`/no password by default, Ubiquiti `ubnt`/`ubnt`,
  pfSense `admin`/`pfsense`). Operators are expected to change it
  before deployment.

### First-boot checklist

Before exposing a WraithOS device to any network:

1. Boot once with serial console access. Log in as `root` with
   `wraithos-admin-change-me` to confirm the image works.
2. Edit `deploy/wraithos/{x86_64,aarch64}/rootfs_overlay/etc/shadow`
   in your checkout, replacing the root hash with your own. Generate
   a hash with:

       openssl passwd -6 '<your-password>'

   Busybox `login` accepts SHA512-crypt (`$6$...`), SHA256-crypt
   (`$5$...`), MD5-crypt (`$1$...`), and DES crypt. SHA512 is
   recommended.
3. Rebuild the image (`bash deploy/wraithos/...` pipeline) or patch
   the rootfs squashfs directly.
4. After boot, verify only the serial console is exposed. Keep
   `/healthz` and `/readyz` behind whatever network policy you
   apply — they are public by default but bind to
   `127.0.0.1:9090` unless you changed `metrics_bind`.
5. Set `trusted_signer_keys` and `require_auth` in
   `/etc/wraith/orchestrator.toml` (see the tenant and signing
   sections above) before accepting external traffic.

### What is NOT defended by default

- The console is password-protected (`wraithos-admin-change-me`) but
  the string is public. Physical access without a changed password
  gives root.
- The orchestrator HTTP API is unauthenticated by default
  (`require_auth = false`, `trusted_signer_keys = []`). Any network
  reachable to `metrics_bind` can POST modules and execute them.
- The signed-module enforcement is off by default
  (`trusted_signer_keys = []`). Enabling it requires signing every
  module in `modules/{tenant}/` (matching `.wasm.sig` siblings), or
  the loader skips them on startup.

Operators who need a stronger default should fork the board overlay
and set their own root hash before building; the image build is
reproducible from `deploy/wraithos/`.
