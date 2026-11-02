# Wraith-V2 (Orchestrator)

Unified core engine in Rust: sandboxed Wasm plugins + Rhai scripting + static musl binary.

## Features

- Wasmtime component model (wasm32-wasip2) with WIT host imports
- Rhai scripting via spawn_blocking
- Multi-tenant with Bearer auth (argon2 PHC)
- Per-module + per-tenant quota
- Lifecycle API: upload / replace / delete / execute
- Build pipeline (POST /modules/build)
- Ed25519 module signatures
- Defense: epoch 60s, memory 256MB, concurrency pool
- Prometheus /metrics, /debug/state
- Static musl: x86_64 23MB, aarch64 16MB
- Graceful SIGTERM shutdown

## Architecture

    +---------------------------------------------+
    |         orchestrator (single binary)        |
    |  HTTP API (axum) -> TenantRegistry          |
    |                  -> ModuleRegistry          |
    |                  -> WasmPool (2 conc)       |
    |                  -> Wasm guest (sandbox)    |
    |                  -> Host fns (http, tcp)    |
    |  Storage: sqlite + modules/{tenant}/*.wasm  |
    +---------------------------------------------+

Guests never touch OS. All I/O via capability-gated host functions.

## Quick start

    rustup target add x86_64-unknown-linux-musl
    cargo zigbuild --release --target x86_64-unknown-linux-musl
    ./target/x86_64-unknown-linux-musl/release/orchestrator --config orchestrator.toml
    curl http://127.0.0.1:9090/healthz

## Building a plugin

    wit_bindgen::generate!({
        path: "wit",
        world: "plugin",
    });

    struct MyPlugin;

    impl Guest for MyPlugin {
        fn run(payload: String) -> Result<String, String> {
            match orchestrator::host::http::get("https://example.com") {
                Ok(body) => Ok(format!(r#"{{"len":{}}}"#, body.len())),
                Err(e) => Err(e),
            }
        }
    }

    export!(MyPlugin);

    cargo build --target wasm32-wasip2 --release
    cp target/wasm32-wasip2/release/my_plugin.wasm ./modules/

## API

Lifecycle:
- GET    /modules                list
- POST   /modules                upload (X-Module-Id, optional X-Module-Signature, X-Module-Quota)
- PUT    /modules/{id}           replace
- DELETE /modules/{id}           delete
- POST   /modules/{id}/execute   invoke (JSON body)
- GET    /modules/{id}/quota     read
- PUT    /modules/{id}/quota     update

Multi-tenant: prefix /t/{tenant}, requires Authorization: Bearer if token_hash configured.

Build (M19):
- POST   /modules/build          tar.gz of crate (WARNING: arbitrary code exec)

Observability:
- GET    /healthz
- GET    /readyz
- GET    /metrics                Prometheus
- GET    /debug/state            JSON snapshot

## Configuration

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
    inline = 'log_info("hello");'

    [[capabilities]]
    type = "http-get"
    allowed_hosts = ["example.com"]

    [[capabilities]]
    type = "tcp-connect"
    allowed_ports = [443]

## Deployment

    bash deploy/install.sh        # build + install + enable + start
    bash deploy/smoke.sh          # restart + 6 assertions
    systemctl --user status orchestrator
    tail -f logs/orchestrator.log

Systemd unit: deploy/orchestrator.service (user-scope, KillSignal=SIGTERM).
Logrotate: deploy/logrotate.conf.

## Security model

Trust boundaries:
1. Host vs guest: guest cannot access OS; all I/O via capability-gated host fns.
2. Tenant vs tenant: registry key (tenant, id); cross-tenant execute returns 404.
3. Operator vs tenant: upload localhost-only; token_hash for non-default tenants.

Resource limits:
- Epoch deadline: 60s per execution
- Linear memory: 256 MB per instance
- WasmPool concurrency: 2 global
- Module quota: per-module (memory, concurrency, rate)
- Tenant caps: per-tenant (modules, concurrency, rate)
- Socket registry: 256 global
- Build timeout: 300s, concurrency 1

Out of scope (single-tenant self-host):
- Tenant isolation beyond API level (shared FS, memory)
- Build pipeline sandboxing (arbitrary code exec by design)

## Roadmap

- M1-M10: core, hardening, quota                  [done]
- M11-M13: observability, deploy scripts          [done]
- M14: ed25519 signatures                         [done]
- M15-M19: lifecycle, quota, multi-tenant, build  [done]
- M20: horizontal scaling                         [deferred - single-node]
- M21+: backup/restore, WebSocket events, per-tenant memory

## Build targets

- x86_64-unknown-linux-musl: 23 MB static
- aarch64-unknown-linux-musl: 16 MB static

Built with cargo zigbuild (Zig linker) to bypass musl FFI issues with
sqlx (bundled SQLite) and wasmtime (Cranelift).

## License

MIT OR Apache-2.0
