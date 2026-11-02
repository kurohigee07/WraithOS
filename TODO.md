# WraithOS / Orchestrator — TODO

Status per v0.20.8-final. VPS Oracle deadline: ~15 days from 2026-11-02.

## 🔴 Critical — sebelum buka ke publik

- [ ] **Build pipeline sandbox** — `POST /modules/build` = arbitrary code execution via `build.rs`
  - Fix: bubblewrap / systemd-nspawn sandbox, atau disable di production
  - Effort: 1-2 hari

- [ ] **Tenant isolation** — registry `(tenant, id)` cuma namespace
  - Shared process, shared FS, shared memory. Panic 1 tenant = crash semua
  - Fix: fork per tenant, atau multi-instance + gateway
  - Effort: 1 minggu
  - Relevan: hanya kalau jual ke multi-customer

## 🟡 High — feature roadmap

- [ ] **M29: First-boot wizard** — `/ui/wizard` embedded HTML + Rust handler
  - Setup: hostname, admin password (argon2), network (DHCP/static), disk encryption option
  - Effort: 2-3 jam

- [ ] **M30: aarch64 build** — Raspberry Pi / ARM mini PC
  - Buildroot config `wraithos_aarch64_defconfig`
  - Effort: 2-3 jam (mostly waiting)

- [ ] **M31: LUKS encrypted `/data`** — passphrase atau TPM2 unlock
  - Effort: 3-4 jam

- [ ] **M32: Signed releases** — GPG sign + SHA256SUMS + reproducible build
  - Effort: 1-2 jam

## 🟢 Medium — technical debt

- [ ] **Wasmtime async in tokio worker** — currently pool cap 2. For 10+ parallel, migrate to `spawn_blocking`
  - Effort: 2-3 jam

- [ ] **CPU time per module** — epoch 60s global. Per-module `epoch_deadline` in manifest
  - Effort: 1 jam

- [ ] **HTTP handler timeout** — Axum default no timeout. Add `TimeoutLayer` from tower-http
  - Effort: 30 menit

- [ ] **`/readyz` deep check** — currently returns "ok" without DB/module check
  - Fix: `db.ping()`, module count check
  - Effort: 1 jam

- [ ] **Integration test suite** — no end-to-end test (start → upload → execute → verify)
  - Effort: 1-2 hari

- [ ] **Metrics cardinality** — label `tenant` per metric. 100+ tenants = 100x time series
  - Relevan: hanya kalau PaaS-scale
  - Effort: 1 hari

## ⚪ Low — polish

- [ ] Config `--dry-run` output lebih informatif
- [ ] Graceful shutdown timeout configurable
- [ ] Log struct field proper (currently Debug formatting)
- [ ] OpenAPI spec docs
- [ ] Backup restore drill (test actual restore)

## Urutan prioritas (rekomendasi)

**Fase 1 (hari 1-2): anti-regret**
1. Backup verify (image + daily)
2. Quick wins: HTTP timeout + `/readyz` deep (1 jam)
3. Build pipeline sandbox (kalau buka ke temen)
4. Tag `v0.21.0-stable`

**Fase 2 (hari 3-8): feature complete**
- M29 wizard → M30 aarch64 → M31 LUKS → M32 signing
- Tag `v0.22.0-feature-complete`

**Fase 3 (hari 9-14): hardening + release**
- Integration test suite
- Docs finalization
- Release tarball + signature
- Archive everything to OCI
- **Hari 15**: buffer + migrate kalau perlu

## Known cosmetic (non-blocking)

- `/tmp` mount working (fixed v0.20.5)
- Boot log clean (7 OK lines, zero errors, v0.20.5)
- Bash + bracketed paste (v0.20.6)
- Gray prompt (v0.20.6)

## Backup status

- Source code: GitHub `kurohigee07/Orchestrator-`
- Buildroot configs: GitHub `wraithos/br2-ext/`
- Full image 577MB: OCI `wraith-bucket/wraithos-images/` (compressed ~150MB)
- Artifact binary: OCI `wraith-bucket/wraithos-builds/`
- Daily DB/config: OCI `wraith-bucket/daily/` (7 retention)

Recovery from scratch (~30 menit):
1. Clone repo
2. `rclone copy oci:wraith-bucket/wraithos-images/`
3. `tar -xzf` image
4. `dd` to USB / boot QEMU
