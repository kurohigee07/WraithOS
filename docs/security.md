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

- Tar extraction occurs outside the Bubblewrap Cargo invocation and relies on the external tar implementation's path/link behavior.
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
