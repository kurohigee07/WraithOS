# Wraith

Unified Wasm runtime + immutable appliance OS. Rust static musl, single binary,
no runtime dependencies. Built for edge deployment of sandboxed Wasm plugins.

Two deliverables in one repository:

- **Wraith-V2** — the orchestrator engine (axum HTTP API, Wasmtime sandbox, Rhai
  scripting, multi-tenant quotas, Prometheus metrics, WebSocket events, build
  pipeline)
- **WraithOS** — the appliance OS (Buildroot + BusyBox, immutable SquashFS rootfs,
  kernel tuned with eBPF + cgroups v2, boot-to-orchestrator in 5 seconds)

WraithOS image size: **18 MB total** (11 MB kernel + 7 MB rootfs). Compare Ubuntu
server 2+ GB.

## Features

### Orchestrator (Wraith-V2)

- Wasmtime component model (wasm32-wasip2) with WIT-defined host imports
- Rhai scripting via spawn_blocking (event-driven rules)
- Cross-language EventBus (Wasm <-> Rhai <-> Rust)
- Multi-tenant with Bearer auth (argon2 PHC), per-tenant caps
- Per-module quota: memory, concurrency, rate limit (sidecar JSON)
- Lifecycle API: upload / replace / delete / execute modules over HTTP
- Build pipeline: POST /modules/build compiles uploaded tar.gz source
- Ed25519 module signatures (verify .wasm before load)
- Audit logs (SQLite execution_logs, queryable via /audit/logs and wraith logs)
- Web dashboard (HTML + HTMX, /ui) with live WebSocket event feed (/ws/events)
- Observability: Prometheus /metrics, /debug/state JSON snapshot
- Static musl binaries: x86_64 (23 MB), aarch64 (16 MB)
- Graceful SIGTERM shutdown with clean DB close
- OCI Object Storage backup (rclone, daily systemd timer, 7-day retention)

### Appliance OS (WraithOS)

- Buildroot 2024.02.9 + BusyBox (musl libc)
- Immutable SquashFS rootfs (read-only, anti-tamper)
- Kernel 6.6 tuned: eBPF + AF_PACKET + cgroups v2 + panic=5 + watchdog
- tmpfs runtime: /modules + /var/lib/orchestrator (sub-ms Wasm execution)
- Auto-start orchestrator via /etc/init.d/S99wraith-init
- Boot time: 5-8 seconds (KVM accelerated) or 15-20s (TCG emulation)
- Image size: 18 MB

## Architecture

Orchestrator request flow:

    HTTP client
        -> axum router (/modules, /t/{tenant}/modules, /ui, /ws/events, /metrics)
        -> TenantRegistry (auth check, per-tenant caps)
        -> ModuleRegistry (lookup by (tenant, id))
        -> ModuleQuotaState (rate + concurrency)
        -> WasmPool (global semaphore)
        -> Wasmtime component (sandboxed guest)
            -> host imports (http.get, tcp.connect) via capability gate
            -> events.emit -> EventBus -> subscribers (Rhai, WebSocket)
        -> response

Storage:

    SQLite (execution_logs, persistent state)
    modules/{tenant}/{id}.wasm
    modules/{tenant}/{id}.wasm.quota.json

WraithOS boot flow:

    QEMU/mini PC boot
        -> kernel (bzImage, 11 MB)
        -> /sbin/init (BusyBox)
        -> /etc/init.d/S99wraith-init
        -> mount tmpfs (/modules, /var/lib/orchestrator)
        -> /sbin/wraith-init --config /etc/wraith/orchestrator.toml
        -> HTTP /healthz responds on 127.0.0.1:9090

## Quick start (Orchestrator)

Build:

    rustup target add x86_64-unknown-linux-musl
    cargo zigbuild --release --target x86_64-unknown-linux-musl

Run:

    ./target/x86_64-unknown-linux-musl/release/orchestrator --config orchestrator.toml

Verify:

    curl http://127.0.0.1:9090/healthz
    curl http://127.0.0.1:9090/metrics

Install as systemd user service:

    bash deploy/install.sh
    bash deploy/smoke.sh
    systemctl --user status orchestrator

## Building a plugin

Crate must declare crate-type = ["cdylib"] and import the host WIT:

    wit_bindgen::generate!({
        path: "wit",
        world: "plugin",
    });

    struct MyPlugin;

    impl Guest for MyPlugin {
        fn run(payload: String) -> Result<String, String> {
            match orchestrator::host::http::get("https://example.com") {
                Ok(body) => Ok(format!("{{\"len\":{}}}", body.len())),
                Err(e) => Err(e),
            }
        }
    }

    export!(MyPlugin);

Build and upload:

    cargo build --target wasm32-wasip2 --release
    cp target/wasm32-wasip2/release/my_plugin.wasm ./modules/default/

Or upload via HTTP:

    curl -X POST http://127.0.0.1:9090/modules \
        -H "X-Module-Id: my-plugin" \
        --data-binary @my_plugin.wasm

## CLI (wraith)

Global install:

    cargo install --path wraith-cli --force

Commands:

    wraith list                                 list modules
    wraith deploy -i id -f file.wasm            upload module
    wraith exec id -p '{"key":"val"}'           execute with JSON payload
    wraith delete id                            remove module
    wraith logs                                 view audit logs (last 50)
    wraith --tenant alice --token X list        multi-tenant

## API reference

All endpoints on 127.0.0.1:9090. Multi-tenant routes prefixed /t/{tenant} and
require Authorization: Bearer <token> if the tenant has token_hash configured.
Legacy /modules routes map to the default tenant.

Module lifecycle:

    GET    /modules                       list
    POST   /modules                       upload (X-Module-Id, opt X-Module-Signature, opt X-Module-Quota)
    PUT    /modules/{id}                  replace (hot-swap)
    DELETE /modules/{id}                  remove
    POST   /modules/{id}/execute          invoke with JSON payload
    GET    /modules/{id}/quota            read quota
    PUT    /modules/{id}/quota            update quota

Build pipeline (M19 - trusted networks only):

    POST   /modules/build                 tar.gz of crate
                                          WARNING: runs cargo build = arbitrary code execution

Observability:

    GET    /healthz                       "ok"
    GET    /readyz                        "ok"
    GET    /metrics                       Prometheus exposition
    GET    /debug/state                   JSON snapshot (modules, pool, sockets, uptime)
    GET    /audit/logs                    last 50 execution logs
    GET    /ui                            web dashboard (HTMX)
    GET    /ui/fragments/*                HTMX polling endpoints
    GET    /ws/events                     WebSocket live event stream

## Configuration (orchestrator.toml)

    db_path = "orchestrator.db"
    modules_dir = "./modules"
    http_timeout_secs = 30
    max_sockets = 256
    max_concurrent_wasm = 2
    wasm_acquire_timeout_secs = 10
    wasm_memory_limit_mb = 256
    log_level = "info"
    log_format = "pretty"
    metrics_bind = "127.0.0.1:9090"
    default_tenant = "default"

    [script]
    inline = 'log_info("rhai received: " + event_kind);'

    [[capabilities]]
    type = "http-get"
    allowed_hosts = ["example.com"]

    [[capabilities]]
    type = "tcp-connect"
    allowed_ports = [443]

    [[capabilities]]
    type = "emit-event"
    allowed_kinds = ["*"]

Per-tenant config (optional):

    [tenants.alice]
    token_hash = "$argon2id$v=19$m=19456,t=2,p=1$..."
    max_modules = 10
    max_concurrent_wasm = 4
    rate_limit_per_sec = 100

## Deployment (systemd)

    bash deploy/install.sh        build + install + enable + start (idempotent)
    bash deploy/smoke.sh          restart + 6 assertions
    bash deploy/test-ws.sh        verify WebSocket event stream
    systemctl --user status orchestrator
    tail -f logs/orchestrator.log

Backup to OCI Object Storage:

    bash deploy/backup.sh
    systemctl --user list-timers orchestrator-backup.timer

Restore:

    bash deploy/restore.sh <filename.tar.gz>

## WraithOS

Build the appliance image from scratch (Buildroot 2024.02.9 + musl + BusyBox):

    # 1. Download Buildroot
    wget https://buildroot.org/downloads/buildroot-2024.02.9.tar.gz
    tar -xzf buildroot-2024.02.9.tar.gz
    cd buildroot-2024.02.9

    # 2. Copy defconfig
    cp ~/orchestrator-project/wraithos/buildroot-configs/wraithos_x86_64_defconfig configs/

    # 3. Setup BR2_EXTERNAL
    cp -a ~/orchestrator-project/wraithos/br2-ext ~/wraithos-br2-ext

    # 4. Copy orchestrator binary to overlay
    cd ~/orchestrator-project
    cargo zigbuild --release --target x86_64-unknown-linux-musl
    cp target/x86_64-unknown-linux-musl/release/orchestrator \
       ~/wraithos-br2-ext/board/wraithos/rootfs_overlay/sbin/wraith-init
    chmod +x ~/wraithos-br2-ext/board/wraithos/rootfs_overlay/sbin/wraith-init

    # 5. Build (first run ~60-90 min)
    cd ~/buildroot-2024.02.9
    export BR2_EXTERNAL=$HOME/wraithos-br2-ext
    make O=output wraithos_x86_64_defconfig
    make O=output -j$(nproc)

    # 6. Output: output/images/bzImage + rootfs.squashfs

Boot in QEMU (KVM required for speed):

    qemu-img create -f raw /tmp/wraithos.img 512M
    mkfs.ext4 -d ~/buildroot-2024.02.9/output/target /tmp/wraithos.img

    qemu-system-x86_64 -machine q35 -enable-kvm -m 2048 \
        -kernel ~/buildroot-2024.02.9/output/images/bzImage \
        -drive file=/tmp/wraithos.img,format=raw,if=virtio \
        -append "root=/dev/vda rw console=ttyS0 panic=5 loglevel=7" \
        -nographic -cpu host

In-guest verification:

    cat /etc/os-release                       # NAME="WraithOS"
    uname -a                                  # Linux 6.6.0
    ps | grep wraith                          # orchestrator PID
    wget -qO- http://127.0.0.1:9090/healthz   # ok
    mount | grep -E 'tmpfs|bpf'               # /modules, /var/lib/orchestrator, /sys/fs/bpf
    cat /proc/cmdline                         # panic=5
    ls /sys/fs/cgroup/                        # cgroup.controllers (v2)

Kernel tuning (wraithos/br2-ext/board/wraithos/kernel.fragment):

    CONFIG_BPF_SYSCALL=y, CONFIG_BPF_JIT=y, CONFIG_PACKET=y (AF_PACKET)
    CONFIG_CGROUPS=y, CONFIG_CGROUP_BPF=y
    CONFIG_WATCHDOG_NOWAYOUT=y, CONFIG_PANIC_TIMEOUT=5, CONFIG_PANIC_ON_OOPS=y
    CONFIG_SQUASHFS=y, CONFIG_OVERLAY_FS=y

## Security model

Trust boundaries:

1. Host vs guest (Wasm): guest cannot access OS. Only host functions exposed via
   WIT (http.get, tcp.connect, events.emit). All gated by CapabilitySet from
   config. Invalid capability = guest receives error, host stays safe.
2. Tenant vs tenant: registry keyed by (tenant, id). Bearer auth per request.
   Cross-tenant execute returns 404 (not 401) to avoid existence leaks.
3. Operator vs tenant: upload endpoints bound to 127.0.0.1 only. token_hash
   (argon2 PHC) protects non-default tenants.

Resource limits (defense-in-depth):

    Epoch deadline        60s per execution
    Linear memory         256 MB per Wasm instance
    WasmPool concurrency  2 global
    Module quota          per-module (memory, concurrency, rate)
    Tenant caps           per-tenant (max_modules, concurrency, rate)
    Socket registry       256 global
    Build timeout         300s, concurrency 1

Out of scope (single-user self-host assumption):

- Tenant isolation beyond API level (shared filesystem and process memory)
- Build pipeline isolation beyond tempdir + env_clear + timeout
- Running mutually untrusting tenants in the same process

## Roadmap

    M1-M10        core, hardening, quota                              done
    M11-M13       observability, deploy scripts                       done
    M14           ed25519 module signatures                           done
    M15-M19       lifecycle API, quota, multi-tenant, build pipeline  done
    M20           OCI backup automation                               done
    M21-M25       SDK, CLI, EventBus, audit log, dashboard, WebSocket done
    M26           multi-arch release packaging                        done
    M27 (MVP)     WraithOS Level 2 - boot, orchestrator live          done
    M28           RAUC A/B atomic update                              planned
    M29           Web first-boot wizard                               planned
    M30           aarch64 build for Raspberry Pi                      planned
    M31           LUKS encrypted data partition                       planned
    M32           signed releases (GPG + SHA256SUMS)                  planned

## Build targets

    x86_64-unknown-linux-musl    23 MB static (orchestrator)
    aarch64-unknown-linux-musl   16 MB static (orchestrator)
    Buildroot image (WraithOS)   18 MB total (11 MB kernel + 7 MB rootfs)

Built with cargo zigbuild (Zig as linker) to bypass musl FFI issues with sqlx
(bundled SQLite) and wasmtime (Cranelift).

Key dependencies: tokio 1.54, wasmtime 48, reqwest 0.13 (rustls-no-provider +
ring), sqlx 0.8 (sqlite bundled), axum 0.8, rhai 1.22, ed25519-dalek 2, argon2 0.5.

## Repository structure

    orchestrator/           core engine (axum + wasmtime + rhai + sqlx)
    wraith-sdk/             Rust client SDK
    wraith-cli/             wraith CLI binary
    shared-types/           DTOs (ModuleManifest, Capability, ModuleQuota)
    sample-plugin/          example Wasm guest
    wraithos/               Buildroot sources (BR2_EXTERNAL + configs + README)
    deploy/                 systemd units, scripts, logrotate
    HANDOFF.md              session handoff document for context preservation

## License

MIT OR Apache-2.0
