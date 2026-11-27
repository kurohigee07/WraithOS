# Roadmap

Versions are tagged `v0.<minor>.<patch>-<slug>`; slugs describe the
theme of the release. This file is updated per release; anything not
shipped by the time of the next release moves forward or is dropped
with a note.

## Shipped (reference)

- `v0.49.0` — TPM 2.0 LUKS keyfile sealing (PCR 0–7)
- `v0.50.0` — UEFI Secure Boot x86_64 (custom PK, no Microsoft shim)
- `v0.51.0` — Pre-wizard dropbear gate
- `v0.52.0` — Audit response (11/14 findings fixed, H1/H2 Wasm and
  HTTP hardening)
- `v0.53.0` — dm-verity runtime rootfs integrity (x86_64 + aarch64)
- `v0.53.1` — dm-verity in-place update hook, RAUC healthcheck,
  hardening, grubenv atomicity, bootcheck reboot-loop fix,
  H2/H3 completion, config/subprocess hardening
- **post-v0.53.1 (unreleased)** — `http::post` Wasm host capability
  with `http-post` split from `http-get`, CI aarch64 musl fix

## v0.54 — hardware validation (target: 2027-Q1)

This is the largest credibility gap on the project and the primary
budget line in the NLnet application.

- **Physical hardware validation.** TPM 2.0 bare metal (x86_64
  mini-PC with dTPM, ARM SBC with TPM module) for:
  - TPM seal / unseal across cold boots
  - Secure Boot chain end-to-end
  - dm-verity panic-on-tamper under real disk controller behaviour
  - LUKS / dm-crypt under real storage
- **IMA appraisal** (deferred from v0.50). File-level integrity inside
  the rootfs, signed at build, verified at boot.
- **Per-device Secure Boot keys.** v0.50.0 shipped a single custom PK;
  v0.54 targets per-device PK/KEK/db enrollment at first deployment.
- **aarch64 runtime end-to-end.** Boot, wizard, RAUC install, cold
  reboot — currently verified at build and image level only.
- **Failure-injection drills.** Power loss during RAUC install,
  mid-write grubenv (already atomic-rename; verify on hardware),
  TPM cleared externally, kernel panic during trial boot.
- **Second-release reviewer** (see GOVERNANCE.md bus-factor section).

## v0.55 — network-bound disk encryption (target: 2027-H1)

- **Clevis/Tang** for network-bound LUKS. Requires a Tang endpoint
  and a printed recovery keyfile. Recommended when the device is
  reachable over a management network.
- **Recovery keyfile at wizard time.** Print-once recovery key, so
  operators have a path back when the TPM is cleared and the plaintext
  keyfile has been finalized away.
- **U-Boot FIT measured boot for aarch64.** Replaces the current
  EFI-GRUB aarch64 chain with a measured U-Boot chain. Requires ARM
  SBBR compliance work.

## v1.0 — pre-audit freeze (target: TBD)

- Independent security audit of the boot chain, TPM seal/unseal, dm-
  verity integration, RAUC install path, and the Wasm sandbox.
- No new features after v1.0-rc1; only fixes.
- Public test report, published under AGPL-compatible terms.

## Out of scope (for now)

- Microsoft-signed shim path for Secure Boot. v0.50 deliberately does
  not use a Microsoft shim; the custom-PK path stays.
- Kubernetes / cluster deployment. WraithOS is a single-appliance
  image, not a cluster orchestrator.
- Multi-tenant trust. The runtime is single-operator with namespace
  tenancy; it is not designed for mutually-untrusting tenants. See
  `docs/security.md` "Deployment decision guide".
