# WraithOS — Project State Snapshot

Quick read-only summary of what works, what is verified, and what is
pending. This is the file to open when you need a one-page answer to
"is this shippable yet?".

Last updated: 2026-11-16. No patches expected until further notice; treat this snapshot as the current baseline.

## Current release

- Tag: `v0.53.0-dm-verity` (published)
- HEAD: `5463c13` (unreleased fixes on top of `v0.53.0`)
- CI: green
- Two arches: `x86_64` (BIOS + UEFI SB path), `aarch64` (UEFI)

## What ships in the image today

| Feature | Verified how | Confidence |
|---|---|---|
| Boot x86_64 (BIOS) | QEMU TCG, GRUB + login prompt | High |
| Boot x86_64 (UEFI SB) | QEMU TCG with OVMF + custom PK | High (v0.50) |
| Boot aarch64 (UEFI) | QEMU TCG with QEMU_EFI.fd | High |
| A/B rootfs + RAUC | QEMU, tamper tested | High (v0.48) |
| dm-verity runtime rootfs | QEMU, tamper tested (happy + corrupt) | High (v0.53) |
| TPM 2.0 LUKS seal (PCR 0-7) | QEMU + swtpm on prior build host | High (v0.49) |
| Pre-wizard dropbear gate | QEMU (fresh boot vs wizard.complete) | High (v0.51) |
| Wasm fuel budget (H1) | Benchmarked against 5 fixtures | High (v0.52) |
| http::get 16 MiB body cap (M2) | Unit + QEMU | High (v0.52) |
| Rhai operation/memory budget (H2) | 4 unit tests; QEMU path unchanged | High |
| LUKS keyfile auto-finalize (H3) | 8 unit tests; QEMU TPM path untested | Medium |
| Boot banner + version unify | QEMU x86 + ARM visual | High |

## Known limitations (honest, publicly documented)

- **Hardware validation pending.** QEMU only, no bare-metal. Tracked for v0.54.
- **H3 end-to-end TPM path not exercised.** QEMU's TPM emulator backend aborts in init on the current build host (QEMU 10.1 Alma and QEMU 8.2 Ubuntu both). swtpm is healthy; QEMU aborts the emulator chardev before sending the TPM command body. WraithOS's H3 code is a thin caller of the v0.49 `/sbin/wraith-tpm-finalize` script (unchanged since v0.49), so blast radius is limited to "does the caller fire at the right time" — covered by unit tests.
- **IMA appraisal deferred** (v0.54).
- **v0.51+ BIOS-only** for Secure Boot. v0.50 was the last SB-enabled release; per-device SB keys in v0.54.
- **SB key v0.50 lost**; migration runbook exists.
- **Rhai has no fuel budget for Wasm** (different mechanism): Wasm fuel is H1, Rhai is H2. Both bounded now.
- **Bus factor 1.**

## How to consume today

Download a release bundle, write to USB (x86) or flash to SD (aarch64), boot. On first boot:

1. Serial console shows wizard banner.
2. POST `/wizard/step1`, `/wizard/step2`, `/wizard/finish` via `http://<device>:9090/wizard/...` (or via the dashboard).
3. Wizard writes orchestrator config + LUKS-encrypts `/data` + seals keyfile to TPM (if present) + auto-finalizes.
4. Reboot: `/data` unlocks from TPM if present, plaintext fallback if not.

No cloud, no external service.

## Backup + restore

OCI Object Storage layout:

    oci:wraith-bucket/wraithos-builds/
      wraithos-git-<TS>.gitbundle    # full git history, verify with `git bundle verify`
      wraithos-state-<TS>.tar.gz     # orchestrator-project + ext trees + sb-recovered

Restore:

    git clone wraithos-git-<TS>.gitbundle wraithos-restore
    cd wraithos-restore
    git remote set-url origin <your-git-url>
    git push origin main --tags
    tar -xzf wraithos-state-<TS>.tar.gz -C ~/

## What to work on next (in priority order)

1. **Hardware validation (v0.54).** Real TPM chip, real UEFI Secure Boot path, per-device SB keys. This closes the "QEMU only" gap. Includes T7/T8 H3 e2e.
2. **IMA appraisal.** Requires kernel cmdline + a writable IMA policy + keyring setup. Deferred from v0.50.
3. **SB key migration path.** v0.50 keys are lost; runbook exists but no automated migration.
4. **HN launch.** Draft ready. Timing decision.

## Emergency playbook

- If CI fails after a push: `gh run view --log-failed`.
- If a release tag must move: **do not**. Force-push breaks forks. Cut a new tag.
- If a security regression ships: publish a `-hotfix` tag with the fix, keep the old one accessible.

## Notes to self

- Do not trust release notes without reading the linked commit.
- Rebuild both arches before cutting a tag — the ARM build can diverge from x86 if ext trees are edited independently.
- `sync-wraith-init.sh` must be run after any orchestrator rebuild, before `make` in buildroot.
- OCI backup runs on its own cadence; run it after any commit that touches `orchestrator/`, `deploy/`, `docs/`, or the ext trees.
