# State

**Version:** 0.54.0 (tag `v0.54.0`, commit `98632c4`)

## What works (verified)

### ARM aarch64 (QEMU, v0.54.0)
- Fresh boot: dm-verity chain OK, root squashfs mounted read-only
- Wizard: complete via `/wizard/step1|step2|finish`, LUKS format `/dev/vda4`,
  `/data` bind-mounted, dropbear active
- In-place RAUC update: PASS
  - `wraith-install-bundle` -> RAUC copy -> `[handlers] post-install` ->
    `wraith-verity-rehash` recompute tree + grub.cfg slot B rewrite ->
    trial arm -> power cycle -> slot B boot with new hash -> healthcheck
    `readyz 200` -> `rauc status mark-good` -> commit -> second power cycle
    persists slot B.

### x86_64 (BIOS, v0.54.0)
- Image builds, boots, dm-verity OK. E2E RAUC cycle deferred to v0.55
  (parity with ARM, same code path expected to work).

## In progress / deferred

| Item | Target |
|------|--------|
| x86_64 E2E RAUC cycle | v0.55 |
| H3 TPM final sign-off | v0.55 |
| Bare metal boot / real HW validation | v0.55+ |
| UEFI Secure Boot per-device rollout | v0.55+ |

## Critical invariants

- `wraith-verity-rehash` runs before `wraith-bootenv trial`. Failed rehash
  -> no trial arm -> boot stays on old slot (safe).
- Rehash refuses to touch active slot (`/proc/cmdline slot=` check).
- All writes to non-active slot. Power loss mid-write -> trial not armed ->
  boot old slot.
- RAUC commands require `--override-boot-slot=<A|B>` because dm-verity root
  is `/dev/dm-0`, not `/dev/vdaN`.
