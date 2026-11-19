# Operations: build, deploy, observe, recover

This guide describes the scripts actually present in `deploy/`. Read each script before running it: several assume the historical path `%h/orchestrator-project`, an OCI rclone remote, and systemd user services. They are not universal installers.

## Build choices

### Native development build

```bash
cargo build --locked -p orchestrator
./target/debug/orchestrator --config orchestrator.toml
```

### Static Linux release build

The repository's deployment path uses cargo-zigbuild:

```bash
rustup target add x86_64-unknown-linux-musl
cargo install --locked cargo-zigbuild
cargo zigbuild --locked --release --target x86_64-unknown-linux-musl -p orchestrator
readelf -l target/x86_64-unknown-linux-musl/release/orchestrator | grep INTERP || true
```

A statically linked executable has no ELF interpreter. CI uses `file … | grep "statically linked"` for that check. Zig is required by cargo-zigbuild; install a compatible version using your toolchain policy.

ARM cross-build:

```bash
rustup target add aarch64-unknown-linux-musl
cargo zigbuild --locked --release --target aarch64-unknown-linux-musl -p orchestrator
```

The checked-in ARM appliance binary is about 19.8 MB decimal; historical notes use rounded/binary units and sometimes older sizes. Measure the artifact you actually ship rather than copying an old README number.

The release profile uses fat LTO, one codegen unit, symbol stripping, and `panic="abort"`. A panic can terminate the whole process; service restart policy matters.

## Runtime files and directories

For the root development config:

```text
./orchestrator.db
./orchestrator.db-wal / -shm     # while SQLite is active
./modules/<tenant>/<id>.wasm
./modules/<tenant>/<id>.wasm.quota.json
./modules/<tenant>/<id>.wasm.capabilities.json
./data/wizard.complete
./data/wraith/...               # wizard output
```

Create parent directories for nondefault DB paths before startup. The module writer creates tenant directories as needed. Protect configuration and data directories because they can contain tokens, modules, and guest error details.

## Metrics and operational signals

| Signal | Interpretation |
|---|---|
| `/healthz` fails | Process/listener/proxy path problem |
| `/readyz` reason `db` | SQLite query failure; inspect storage and logs |
| `/readyz` reason `wasm_pool_saturated` | All global permits occupied; not necessarily process failure |
| Rising pool-wait sum/count | Contention before instantiation |
| Rising 429/rejection counters | Rate budget exhausted; clients need backoff |
| Execution errors | Guest traps/errors or invalid result; inspect audit/error details |
| Socket gauge stays high | Possible guest handle leak or long-lived connections |
| Rhai lag warnings | Broadcast subscriber missed events; bus is not durable |
| Audit stops growing while executions succeed | Audit insert errors are ignored; check DB independently |

Counters are process-local and reset at restart. Use Prometheus rate/increase functions over persisted time series rather than comparing raw restarted counters. Histogram quantiles in the supplied benchmark report zero at sub-millisecond scales; use sum/count and understand exporter resolution.

## Package releases

`deploy/package.sh` builds x86_64 and aarch64 musl binaries and stages tarballs under `dist/`. It currently hardcodes `VERSION="v0.18.0"` and begins by removing `dist/*`. Review the version and output directory before running; do not assume the package version follows Cargo or historical tags.

The package includes the binary, config, README, selected deploy files, and an INSTALL text. It does not include the entire source tree or WraithOS image. It generates SHA256SUMS but does not sign them.

## GPG release signing and verification

Maintainer signing:

```bash
WRAITH_SIGNING_KEY=release@wraith.local bash deploy/sign.sh
```

The script requires a secret key already in GPG, regenerates checksums for tarballs in `dist/`, creates armored `SHA256SUMS.asc`, and creates binary detached `.tar.gz.sig` files. Never commit/export a private key into the repository.

Verifier:

```bash
DIST_DIR=dist bash deploy/verify-signature.sh
```

It imports `deploy/wraith-release.pub` into a temporary keyring, verifies SHA256SUMS.asc, then checks tarball hashes. The old README's `gpg --import wraith-release.pub` path is missing the `deploy/` prefix.

The documented release fingerprint is:

```text
21FC5ED189E9C5E7D765129718274FF4C5A2F444
```

Confirm that fingerprint through a trusted maintainer channel, not only this repository. A mathematically valid signature from an untrusted replaced key is not supply-chain authentication. The helper verifies the checksum manifest; verify individual detached tarball signatures too if your distribution relies on them.

## WraithOS and Termux

- `deploy/run-wraithos.sh` runs an existing x86 disk image in QEMU; it does not build/download one.
- WraithOS source build and A/B procedures are architecture-specific: [WraithOS guide](wraithos.md).
- Termux helpers expect a separately supplied `orchestrator` binary that is intentionally ignored/not tracked in the Termux package directory.

## Pre-release operational checklist

- [ ] Build from locked dependencies and record toolchain/target/commit.
- [ ] Verify static linkage and artifact architecture.
- [ ] Run unit/integration tests with network assumptions understood.
- [ ] Start with the exact shipped config; check health/readiness and a trusted module.
- [ ] Test auth denial, capability denial, rate/concurrency behavior, and shutdown under load.
- [ ] Backup and restore into an isolated environment.
- [ ] Verify release signatures using an independently trusted fingerprint.
- [ ] For appliance releases, test both slots, failed updates, persistent data, and device-specific wizard paths.
- [ ] Document actual binary/image sizes and benchmark environment; do not reuse stale headline numbers.

## Metrics cardinality (2026-11-08)

Prometheus exposition in this project is process-scoped:
`metrics-exporter-prometheus` 0.17.2. Counter, gauge, and histogram
label sets are registered once per label combination and persist for
the lifetime of the process. Deleting a module or tenant via the API
does not remove its series from /metrics until restart.

This is standard Prometheus client behavior, not a custom eviction
policy. Operators with high upload/delete churn should plan for one
of:

- Periodic orchestrator restart (weekly cron, or restart after a
  deployment churn event).
- Per-tenant `max_modules` cap in `orchestrator.toml` to bound the
  worst-case label cardinality.
- Downstream Prometheus `metric_relabel_configs` drop rules, or
  scrape-level filtering (drops are applied at scrape time, not at
  exporter).

### Measured

Baseline (empty state, 2 modules): 546 bytes, 10 series.

Load test (20 modules across 3 tenants, each module executed once):

| Metric | Baseline | After load | Delta |
|---|---:|---:|---:|
| /metrics bytes | 546 | 36,020 | +35,474 |
| /metrics series | 10 | 391 | +381 |
| wasm_executions_total series | 0 | 20 | +20 |
| scrape time (5 runs) | <10 ms | <10 ms | flat |

Marginal cost per module: ~1.8 KB / ~19 series (1 counter series +
2 histograms × ~7 quantile series + summary sum/count + shared pool
series). Linear with module count; no super-linear growth observed.

Restart clears the registry (wasm_executions_total drops from 20 to 0
in the same 546-byte baseline payload), confirming process-scoped
semantics.

### Deferred

- Not implemented: label eviction on module/tenant delete. Would
  require wrapping the metrics crate with a dynamic label set or
  moving to OpenTelemetry metrics SDK with cardinality limits.
- Not measured: behavior beyond 20 modules. Production guidance is
  to keep per-instance module count low (tens, not thousands) and
  rely on the max_modules tenant cap as the primary bound.
