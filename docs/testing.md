# Testing, verification, and contributing

## What the Rust suite contains

At the documented baseline there are **23 unit tests** in the orchestrator and **10 HTTP integration tests**:

| Area | Count | Coverage shape |
|---|---:|---|
| Capability matching | 2 | Wildcard and exact HTTP host matching |
| Context/resource registries | 3 | Socket cap, owner checking, global pool timeout/release |
| Module signatures | 4 | Valid, tampered, wrong key, bad signature length |
| Module quota | 3 | Token refill, concurrency, zero/default quota |
| Tenant registry | 6 | Token comparison/auth, rate, concurrency, module capacity |
| HTML escaping | 3 | Server-rendered dashboard field escaping |
| Wizard markers | 2 | Fresh state and persisted markers |
| HTTP integration | 10 | Health, upload/execute/audit, missing module, auth, build gate, capability behavior |

The integration tests call the Axum router through `tower::oneshot`; they do not launch the production binary or bind port 9090. Several use the checked-in sample and can require real access to `example.com`. Some assertions are permissive, and the suite is not a complete hostile-tenant/security test.

## Run the source checks

```bash
cargo fmt --all -- --check
cargo clippy --locked -p orchestrator --lib -- -D warnings
cargo build --locked -p sample-plugin --target wasm32-wasip2 --release
cargo test --locked -p orchestrator -p wraith-sdk -p wraith-cli --all-targets
```

The CI workflow runs on pushes/PRs targeting `main`. It has formatting/Clippy, guest build/tests, and an x86 musl static-linkage job. A session working branch does not receive the push CI trigger unless the workflow is changed or a qualifying PR is opened. CI configuration is not proof that the current checkout passed CI.

**Verification boundary for this update:** Rust/Cargo was not installed in the documentation workspace, so these Rust commands were not rerun. The work changed documentation and documentation tooling, not runtime source.

## Documentation

All documentation is plain Markdown under `docs/`. The static-site
generator (Node, `docs/build.mjs`, `docs/book/`) that used to ship
was removed in v0.52 — it added a Node toolchain dependency for a
feature that no one was using. Render the Markdown with any viewer
(`glow`, `bat`, VS Code preview, GitHub's own).

## Source/documentation consistency checklist

When changing behavior, update the corresponding guide and interactive explanation:

- WIT change → `plugins.md`, sample guests, architecture host bridges.
- Config field/default → `configuration.md` and quick start/appliance examples.
- Route/status/header → `api.md`, SDK/CLI docs if affected.
- Timer/counter semantics → `benchmarks.md` and benchmark lab labels.
- Auth/capability/socket/event scope → `security.md` and architecture.
- Boot script/partition/update change → `wraithos.md` and A/B lab.
- New/removed tracked file → regenerate/update `source-map.md`.


## Contributing a behavior change

1. Describe the user-visible problem and the trust boundary affected.
2. Add a focused failing test where practical.
3. Keep host/guest contract changes coordinated.
4. Run formatting, Clippy, tests, and a relevant manual smoke test.
5. Update English documentation and examples in the same change.
6. Report what was actually run; do not convert a planned test into a claimed pass.
7. For performance claims, preserve raw data and methodology alongside derived figures.

## Test matrix: TPM and Secure Boot

Both features ship with a test matrix that was exercised end-to-end on
QEMU before the corresponding release. The scenarios below are the ones
re-run before any change that touches the boot chain, the LUKS path,
or the SB signing pipeline.

### TPM 2.0 LUKS sealing (v0.49.0, both arches)

Run on QEMU with `-tpmdev emulator,id=tpm0 -device tpm-tis-device,tpmdev=tpm0`.

| Test | Scenario | Pass criterion |
|------|----------|----------------|
| T1 | Fresh image + TPM, wizard with encryption password, cold reboot | `/boot/luks.sealed.{pub,priv}` written; cold boot unseals; `/data` mounted; plaintext keyfile still present (v1 coexistence) |
| T2 | Fresh image, no TPM, wizard with encryption password | `/boot/luks.key` written; no sealed blob; log line `TPM absent, using plaintext keyfile` |
| T3 | Same as T1 on x86_64 | Same as T1 |
| T4 | Same as T2 on x86_64 | Same as T2 |
| T5 | TPM cleared externally (`tpm2_clear`), reboot | Unseal fails; plaintext fallback; `/data` mounted; log line `TPM unseal failed (rc=1), falling back to plaintext keyfile` |
| T6 | Firmware state changed (mutate a PCR-affecting input), reboot | Sealed unseal fails; plaintext fallback; `wraith-tpm-reseal` re-seals and works |

### UEFI Secure Boot x86_64 (v0.50.0)

Run on QEMU with OVMF + KVM. Custom keys enrolled via `virt-fw-vars`
(see `docs/wraithos.md` → "Building a UEFI Secure Boot x86_64 image").
Uses `virtio-blk-pci` for the disk; `ide-hd` presents as `/dev/sda` and
the EFI boot hangs at `Waiting for root device /dev/vda2...`.

| Test | Scenario | Pass criterion |
|------|----------|----------------|
| T-SB0 | BIOS path, unsigned image | Boots to login; `/healthz` OK; `dmesg` does not report Secure Boot enabled |
| T-SB1 | UEFI, custom PK, Secure Boot ON, signed image | Boots to login; `dmesg | grep -i "Secure boot enabled"` returns the line |
| T-SB2 | UEFI, Secure Boot OFF, unsigned image | Boots to login |
| T-SB3 | UEFI, stock OVMF (Microsoft PK), Secure Boot ON, signed image | Firmware refuses to load GRUB (`Access Denied`) |
| T-SB4a | UEFI, custom PK, Secure Boot ON, unsigned GRUB on ESP | Firmware refuses (`Access Denied`) |
| T-SB4b | UEFI, custom PK, Secure Boot ON, signed GRUB | GRUB menu appears; chain proceeds |

### TPM + Secure Boot together

Any change that touches both (e.g. the sealed keyfile path and the ESP)
must re-run T1–T6 and T-SB0–T-SB4b before merge. The two features share
the ESP (`/boot/luks.key`, `/boot/luks.sealed.*`, `/boot/grub/grubenv`,
`EFI/BOOT/BOOTX64.EFI`) and are not independent in the test rig.

## Release verification

Use the [operations checklist](operations.md#pre-release-operational-checklist). Security-sensitive changes additionally need negative tests for authorization, tenant data visibility, signature persistence, source extraction, resource exhaustion, and shutdown/update failure paths. The current test count alone is not a security certification.
