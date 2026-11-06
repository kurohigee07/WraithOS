# WraithOS / Orchestrator — Historical TODO

> **Archival planning list, translated into English.** This records the reported v0.20.8-final plan and an approximately 15-day Oracle VPS window starting 2026-11-02. Checkboxes and effort estimates are historical, not current status. Several items now have implementations in source. Use [docs/testing.md](docs/testing.md), [docs/security.md](docs/security.md), and [docs/source-map.md](docs/source-map.md) for the current verification boundary. The original mixed-language list is preserved at [baseline commit 20cb378](https://github.com/kurohigee07/Orchestrator-/blob/20cb3785d1f17e4b05759b539ffa016eaea50324/TODO.md).

## Critical — before public exposure

- [ ] **Source-build sandbox.** `POST /modules/build` can execute arbitrary code through `build.rs`.
  - Proposed fix: Bubblewrap/systemd-nspawn sandbox, or disable in production.
  - Historical estimate: 1–2 days.
  - Current source: builds are disabled by default and use Bubblewrap when enabled; additional archive/resource/authz hardening is still needed.

- [ ] **Tenant isolation.** The `(tenant, id)` registry was recognized as a namespace rather than a complete security boundary.
  - Shared process/filesystem/resources; the release profile aborts on panic.
  - Proposed fix: process-per-tenant or multiple instances behind a gateway.
  - Historical estimate: one week.
  - Relevance: especially important for mutually untrusting customers.

## High — feature roadmap

- [ ] **M29: first-boot wizard.** Embedded HTML and Rust handlers for hostname, Argon2 admin password, DHCP/static network, and optional disk encryption. Historical estimate: 2–3 hours.
- [ ] **M30: AArch64 build.** Raspberry Pi/ARM mini-PC target and `wraithos_aarch64_defconfig`. Historical estimate: 2–3 hours, mostly build time.
- [ ] **M31: encrypted `/data`.** Passphrase or TPM2 unlock. Historical estimate: 3–4 hours.
- [ ] **M32: signed releases.** GPG, SHA256SUMS, and reproducible builds. Historical estimate: 1–2 hours.

These estimates are preserved as planning context, not promises. Current implementations and their integration gaps are described in the guides.

## Medium — technical debt

- [ ] **Wasmtime execution scheduling.** The note proposed moving CPU-heavy work to `spawn_blocking` for ten or more parallel calls. Historical estimate: 2–3 hours. Validate with measurements before changing the async execution model.
- [ ] **Per-module CPU/deadline configuration.** A manifest-level epoch deadline was proposed alongside the global 60-second deadline. Historical estimate: one hour.
- [ ] **HTTP timeout.** Add a tower-http `TimeoutLayer`. Historical estimate: 30 minutes. Current source has a fixed 60-second outer timeout.
- [ ] **Deep `/readyz`.** Add database/module checks. Historical estimate: one hour. Current source checks SQLite and global-pool availability; zero modules is still considered ready.
- [ ] **Integration suite.** Start → upload → execute → verify. Historical estimate: 1–2 days. Current source contains ten router-level integration tests, not a complete production-binary/security suite.
- [ ] **Metric cardinality.** Review tenant labels at 100+ tenants. Historical estimate: one day; relevant to PaaS scale.

## Low — polish

- [ ] More informative `--dry-run` output.
- [ ] Configurable graceful-shutdown timeout.
- [ ] Structured log fields instead of debug formatting.
- [ ] OpenAPI documentation.
- [ ] A real backup/restore drill.

## Historical priority order

**Phase 1, days 1–2: avoid preventable regret**

1. Verify image and daily backups.
2. Add HTTP timeout and deep readiness checks.
3. Sandbox the build pipeline before sharing access.
4. Tag `v0.21.0-stable`.

**Phase 2, days 3–8: feature completion**

- Wizard → AArch64 → LUKS → signing.
- Tag `v0.22.0-feature-complete`.

**Phase 3, days 9–14: hardening and release**

- Integration tests, final documentation, signed release tarballs, and OCI archival.
- Day 15 reserved as buffer/migration time.

## Reported cosmetic fixes

- `/tmp` mount working in v0.20.5.
- Seven clean boot log lines and no errors reported in v0.20.5.
- Bash/bracketed paste and gray prompt reported in v0.20.6.

## Historical backup inventory

- Source: GitHub `kurohigee07/Orchestrator-`.
- Buildroot configuration: `wraithos/br2-ext/`.
- Reported 577 MB image: `oci:wraith-bucket/wraithos-images/`, approximately 150 MB compressed.
- Binary artifacts: `oci:wraith-bucket/wraithos-builds/`.
- Daily DB/config backups: `oci:wraith-bucket/daily/`, seven-generation retention.

The proposed roughly 30-minute recovery procedure was: clone the repository, copy the image from OCI, extract it, then write it to USB or boot it in QEMU. Those remote objects and timings were not verified here. Writing an image to a block device is destructive; use the current [operations](docs/operations.md) and [WraithOS](docs/wraithos.md) guides with backups and an isolated target.
