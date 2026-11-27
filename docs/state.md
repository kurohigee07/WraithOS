# State

**Version:** 0.53.1 (tag `v0.53.1-update-hook`) + post-tag commits on `main`

## What works (verified)

### Wasm host capabilities (post-v0.53.1, on `main`)
- `http::get` — HTTPS GET, `allowed_hosts` check, 16 MiB body cap
- `http::post` — HTTPS POST with `http-post` capability split from `http-get`,
  same `allowed_hosts` + body cap. Verified end-to-end on x86 QEMU:
  guest → host function → HTTPS POST to `httpbin.org` → 381-byte response.
- `tcp::connect/read/write/close` — raw socket, `allowed_ports` check
- `events::emit`, `log`
- CI green: fmt, clippy, tests, sample-plugin wasm, x86_64 musl, aarch64 musl.

### ARM aarch64 (QEMU, v0.53.1)
- Fresh boot: dm-verity chain OK, root squashfs mounted read-only
- Wizard: complete via `/wizard/step1|step2|finish`, LUKS format `/dev/vda4`,
  `/data` bind-mounted, dropbear active
- In-place RAUC update: PASS
  - `wraith-install-bundle` -> RAUC copy -> `[handlers] post-install` ->
    `wraith-verity-rehash` recompute tree + grub.cfg slot B rewrite ->
    trial arm -> power cycle -> slot B boot with new hash -> healthcheck
    `readyz 200` -> `rauc status mark-good` -> commit -> second power cycle
    persists slot B.

### x86_64 (BIOS, v0.53.1)
- Image builds, boots, dm-verity OK. `http::post` capability verified
  end-to-end via QEMU. Full RAUC E2E cycle deferred — see docs/testing.md
  E2E-3.

## In progress / deferred

| Item | Target |
|------|--------|
| x86_64 E2E RAUC cycle | next milestone |
| H3 TPM final sign-off | next milestone |
| Bare metal boot / real HW validation | after hardware grant |
| UEFI Secure Boot per-device rollout | after hardware grant |

## Critical invariants

- `wraith-verity-rehash` runs before `wraith-bootenv trial`. Failed rehash
  -> no trial arm -> boot stays on old slot (safe).
- Rehash refuses to touch active slot (`/proc/cmdline slot=` check).
- All writes to non-active slot. Power loss mid-write -> trial not armed ->
  boot old slot.
- RAUC commands require `--override-boot-slot=<A|B>` because dm-verity root
  is `/dev/dm-0`, not `/dev/vdaN`.
