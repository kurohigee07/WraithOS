# Wraith Orchestrator — Termux package

Run the Wraith orchestrator on an Android phone via
[Termux](https://termux.dev/). Loopback-only, no root required, no
Buildroot. Same Axum + Wasmtime host that runs on the WraithOS
appliance, with a permissive demo config sized for a phone.

## What this is not

This is not a self-contained package. The `orchestrator` binary is
**not** shipped (26 MB per arch). You build it from source on a
desktop, copy it to the phone, and run it under Termux. The rest of
the files (`orchestrator.toml`, `run.sh`, `status.sh`, `stop.sh`)
travel with this directory.

## Prerequisites

On the phone:

    pkg update && pkg upgrade
    pkg install curl

On the build machine (Linux, macOS, or WSL):

- Rust 1.85+ (`rustup`)
- The `aarch64-unknown-linux-musl` target:
  `rustup target add aarch64-unknown-linux-musl`
- A cross-linker. Simplest: `cargo install cargo-zigbuild` and
  `pipx install ziglang` (or `pkg install zig` on the build host).
  Alternatively use an NDK-based linker if you already have one.

## Build the binary

From the repository root:

    cargo zigbuild --locked --release \
      --target aarch64-unknown-linux-musl \
      -p orchestrator

Output: `target/aarch64-unknown-linux-musl/release/orchestrator`.

If you are building on the phone itself (slow, higher battery
drain, but possible), `cargo build --release -p orchestrator`
native aarch64 also works without the musl target.

## Copy to the phone

Easiest: from the phone, pull over SSH or `adb push`:

    # From the build machine:
    adb push target/aarch64-unknown-linux-musl/release/orchestrator \
      /sdcard/Download/orchestrator

    # On the phone (Termux):
    mkdir -p ~/wraith
    cp /sdcard/Download/orchestrator ~/wraith/orchestrator
    cp /path/to/termux-package/* ~/wraith/
    chmod +x ~/wraith/orchestrator ~/wraith/*.sh

Termux shared storage must be mounted once:
`termux-setup-storage` (grants access to `/sdcard`).

## Run

    cd ~/wraith
    ./run.sh

`run.sh` creates `logs/`, `modules/`, and `data/`, kills any prior
instance, starts the binary in the background, waits for health,
prints the dashboard URL. Logs go to `logs/wraith.log`.

    ./status.sh    # running? healthz + readyz
    ./stop.sh      # graceful stop

## Access from the phone

- Dashboard: <http://127.0.0.1:9090/ui>
- Health: <http://127.0.0.1:9090/healthz>
- Readiness: <http://127.0.0.1:9090/readyz>
- Metrics: <http://127.0.0.1:9090/metrics>

## Upload and run a module

    curl -X POST http://127.0.0.1:9090/modules \
      -H 'x-module-id: my-mod' \
      --data-binary @path/to/module.wasm

    curl -X POST http://127.0.0.1:9090/modules/my-mod/execute \
      -H 'content-type: application/json' \
      -d '{}'

The sample guest from the repository
(`modules/default/sample.wasm`) is a good first upload.

## Config

`orchestrator.toml` in this directory. Notable fields, all of which
are documented inline:

- `data_dir`, `modules_dir`, `db_path` — relative to this directory,
  not the appliance default `/var/lib/orchestrator`.
- `require_auth = false` — dev profile. Any local app can call the
  API. Set `true` and add a tenant token before exposing.
- `metrics_bind = "127.0.0.1:9090"` — loopback only. Changing this
  to `0.0.0.0:9090` exposes the API to every device on the same
  WiFi.
- `[[capabilities]] http-get allowed_hosts = ["*"]` — permissive
  demo. Any `http::get` in a module can reach any host. Tighten
  before running untrusted modules.

## Logs, DB, data

    logs/wraith.log       stdout+stderr of the running binary
    orchestrator.db       SQLite (WAL)
    modules/              uploaded *.wasm files, .sig, .quota.json
    data/                 orchestrator state dir (wizard.complete etc.)

To reset everything (destroys modules and DB):

    ./stop.sh && rm -rf data modules logs orchestrator.db*

## Troubleshooting

**`run.sh` prints "✗ Failed to start"** — `tail -20 logs/wraith.log`.
Common causes: binary missing (`chmod +x`), wrong arch (build
aarch64, not x86_64), port 9090 already in use.

**`./orchestrator: cannot execute binary file`** — binary is for
the wrong architecture. On ARM64 phone, must be
`aarch64-unknown-linux-musl` (or native aarch64 glibc). Check with
`file orchestrator`.

**`healthz` OK but `/ui` blank** — dashboard serves HTMX from
`/static/htmx.js`. If the phone's browser is in a captive portal or
aggressive network filter, htmx.js may be blocked. Termux loopback
should be fine.

**Termux killed the process when the screen locked** — Android
memory management. Run `termux-wake-lock` before `./run.sh` and
`termux-wake-unlock` after `./stop.sh`. Also disable battery
optimization for Termux in Android Settings → Apps → Termux.

**`cargo zigbuild` fails on macOS** — install `ziglang` via
`pipx install ziglang`, then ensure `zig` is on PATH before
`cargo zigbuild`.

## Not covered

- Encrypted `/data` — that's the WraithOS appliance. Termux runs on
  the phone's normal filesystem; there is no LUKS layer.
- A/B updates — the phone runs one copy. Replace the binary by hand
  when you upgrade.
- Secure Boot, TPM sealing — none of that exists on Android apps.
  This is a dev/demo profile.

See the main [README](../../../README.md) for what WraithOS the
appliance does differently, and
[`docs/configuration.md`](../../../docs/configuration.md) for the
full config schema.
