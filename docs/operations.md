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

## systemd user service

`deploy/orchestrator.service` assumes:

```text
WorkingDirectory = %h/orchestrator-project
Binary = %h/orchestrator-project/target/x86_64-unknown-linux-musl/release/orchestrator
Config = %h/orchestrator-project/orchestrator.toml
Logs = %h/orchestrator-project/logs/orchestrator.log
```

It restarts on failure after three seconds, sends SIGTERM, and allows 15 seconds for stop. It appends stdout/stderr to a log file.

Before installation:

```bash
mkdir -p logs
sed -n '1,160p' deploy/orchestrator.service
sed -n '1,160p' deploy/install.sh
```

If your checkout path differs, edit the unit and backup unit first. `install.sh` builds x86 musl, copies all service/timer units, enables the service and backup timer, restarts the service, and polls health. It does not install Rust/Zig/rclone or create an OCI remote for you.

```bash
bash deploy/install.sh
systemctl --user status orchestrator.service
systemctl --user list-timers orchestrator-backup.timer
```

For a user service to survive logout, the host may need lingering enabled by an administrator (`loginctl enable-linger <user>`). This is an OS policy decision, not performed by the script.

The logrotate example is hardcoded to `/home/opc/orchestrator-project/logs/orchestrator.log`, rotates daily, keeps seven compressed generations, and uses copytruncate. Adapt the path and install it through your system's logrotate mechanism if desired.

## Smoke checks: know their assumptions

`deploy/smoke.sh` restarts the user service and checks health, metrics, debug state, and a module named/containing `sample`. It expects `wasm_executions_total` and `wasm_pool_in_flight` metrics to exist. Those series are emitted by activity, so a freshly restarted service with no executions can fail that assertion even when healthy.

Run a trusted sample execution first, or treat the script as a historical smoke helper rather than a complete readiness test. It restarts a service; do not run it casually against production.

`deploy/test-ws.sh` requires websocat and the `wraith` CLI, subscribes to events, executes `sample`, and looks for `sample.emitted`/`rhai.processed`. Its cleanup uses a broad `pkill -f "tail -f /dev/null"` pattern that can affect unrelated processes. Review/replace that cleanup before using it on a shared machine.

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

## Backup to OCI

`deploy/backup.sh` hardcodes:

```text
Remote: oci:wraith-bucket/daily
Working directory: repository root determined from script location
Expected files: orchestrator.db and orchestrator.toml
```

Prerequisites: working rclone remote, permissions, and preferably the `sqlite3` CLI. The script:

1. Fails if DB/config are missing.
2. Uses `sqlite3 .backup` for a WAL-safe snapshot when available.
3. Otherwise copies DB/WAL/SHM files, explicitly noting that this fallback is not transactionally consistent.
4. Archives config, modules, the user service file if present, and deploy directory.
5. Streams tar.gz to `rclone rcat`.
6. Keeps the seven newest lexically sorted filenames.

```bash
command -v rclone
command -v sqlite3
rclone lsf oci:wraith-bucket/daily
bash deploy/backup.sh
```

The timer runs daily at 03:00 with `Persistent=true`; time zone is the host/user manager's local time. The script's fixed names mean custom DB/config/module paths need a revised backup procedure. It does not prove remote object integrity or perform a restore drill.

## Restore procedure

`deploy/restore.sh <filename.tar.gz>` downloads from the same hardcoded remote, stops the user service, extracts into a temporary directory, replaces DB/config/modules/service/deploy files, reloads systemd, starts the service, and checks health.

**Destructive:** it removes existing `modules/` and `deploy/` before copying archived versions. It assumes the current directory is the intended project root and does not make a pre-restore safety copy.

Recommended manual wrapper:

1. Announce downtime and stop writes.
2. Save the current config/module tree and a fresh consistent DB snapshot elsewhere.
3. Download and inspect the archive before extraction.
4. Verify checksum/provenance if your backup process records it.
5. Run restore from the exact intended directory.
6. Check `/readyz`, module list, a trusted execution, audit, and expected configuration.

```bash
cd "$HOME/orchestrator-project"
bash deploy/restore.sh orchestrator-YYYYMMDD-HHMMSS.tar.gz
curl --fail http://127.0.0.1:9090/readyz
```

If the service was stopped and old WAL files remain while only a snapshot DB is copied, recovery semantics deserve explicit verification. Prefer a controlled restore into a clean directory or a tested procedure that removes stale WAL siblings only while the service is stopped and after preserving evidence.

A backup is not verified until restore succeeds. Perform drills away from production.

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
