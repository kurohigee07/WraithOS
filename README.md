<div align="center">

<img src="docs/assets/wraith-banner.svg" alt="Wraith — Rust, WebAssembly, and an appliance operating system" width="100%">

# Wraith

**A small Rust runtime for sandboxed WebAssembly plugins.<br>An appliance OS for running that same runtime close to the metal.**

[Interactive documentation](docs/index.html) · [Quick start](docs/quickstart.md) · [Architecture](docs/architecture.md) · [API](docs/api.md) · [Security boundaries](docs/security.md)

`Rust 1.85+` · `Wasmtime 48` · `WASI Preview 2` · `Axum` · `SQLite` · `MIT OR Apache-2.0`

</div>

> **Read this first.** Wraith is an ambitious, working engineering project, not a claim of independently audited production security. This documentation follows the source at commit `20cb378`, distinguishes measurements from simulations, and calls out unfinished integration paths. Start locally; read [Security](docs/security.md) before exposing an instance to other people.

## What is Wraith?

Imagine deploying a small piece of business logic without shipping an entire container image or giving that code unrestricted access to your server.

Wraith accepts a **WebAssembly component**, exposes a narrowly defined set of host functions, and runs its exported `run` function when an HTTP request arrives. The host controls network permissions, execution concurrency, linear-memory limits, and an epoch-based interruption deadline. Execution results are recorded in SQLite and exported as Prometheus metrics.

The repository contains two connected deliverables:

1. **Wraith Orchestrator** — a Rust service combining an HTTP API, Wasmtime, Rhai event rules, an event bus, a dashboard, and a Rust SDK/CLI.
2. **WraithOS** — Buildroot definitions for a minimal Linux appliance that boots the same orchestrator, keeps application data separate from its read-only root filesystem, and supports A/B root filesystem updates.

```text
Your application / curl / Wraith CLI
                 │ HTTP + JSON
                 ▼
┌──────────────────────────────────────────────────────────────┐
│ Wraith Orchestrator                                      │
│                                                              │
│ Axum → authentication → module lookup → quota gates           │
│                                      │                       │
│                                      ▼                       │
│                           Wasmtime component                 │
│                           run(JSON) → JSON                   │
│                                      │                       │
│                 controlled HTTP / TCP / event emission       │
│                                      │                       │
│                         EventBus → Rhai / WebSocket          │
│                                                              │
│ SQLite audit log + Prometheus metrics + embedded dashboard    │
└──────────────────────────────────────────────────────────────┘
                 ▲ the same Rust binary
┌─────────────────────────────────────────────────────────────┐
│ WraithOS: kernel + read-only rootfs A/B + persistent data │
└──────────────────────────────────────────────────────────────┘
```

### Why this project is interesting

| Value | What it means in practice | Source |
|---|---|---|
| **A compact deployment unit** | The Rust service can be cross-compiled as a static musl binary; the ARM overlay includes an AArch64 ELF executable. No Node.js or Python runtime is needed by the service itself. | [Build configuration](Cargo.toml), [operations](docs/operations.md) |
| **A real sandbox boundary** | Guests use linear memory and explicit WIT imports instead of inheriting arbitrary host filesystem access. Host calls pass through capability checks. | [Wasm implementation](orchestrator/src/wasm/module.rs) |
| **Backpressure, not wishful scaling** | Tenant, module, and global concurrency gates bound work. Rate limits use token buckets; waiting for a permit has a timeout. | [Quota implementation](orchestrator/src/quota.rs) |
| **One contract across languages** | WIT defines the guest boundary. The included guest is Rust, but the contract is not inherently Rust-only. | [WIT contract](orchestrator/wit/host-capabilities.wit) |
| **Observable execution** | Audit rows, execution/wait histograms, pool gauges, health checks, and live events make the runtime inspectable. | [Telemetry](orchestrator/src/telemetry.rs) |
| **Runtime + operating system** | The project goes beyond an application server: boot scripts, persistent mounts, image generation, and ARM boot recovery are included. | [WraithOS guide](docs/wraithos.md) |
| **Inspectable trade-offs** | Limits, known gaps, and historical measurements are documented rather than hidden behind a “production-ready” label. | [Security](docs/security.md), [source map](docs/source-map.md) |

## Explore it interactively

GitHub renders Markdown but does not run JavaScript inside a README. The repository therefore includes a **standalone, dependency-free documentation site** alongside the readable guides.

From a local checkout:

```bash
python3 -m http.server 4173 --bind 0.0.0.0
```

Open **http://localhost:4173/docs/**. Alternatively, open `docs/index.html` directly in a browser; the generated site does not need a backend or CDN.

The site includes:

- **Request journey:** step through authentication, quotas, instantiation, guest execution, and response handling; inspect failure scenarios.
- **Concurrency lab:** change pool capacity, service time, arrivals, and wait timeout; watch requests run, wait, or time out.
- **A/B boot lab:** explore the ARM GRUB trial counter and fallback to the last-known-good slot.
- **Benchmark explorer:** switch between linear and logarithmic views without confusing server execution time with client wall time.
- **Full-text guide search**, code copying, and beginner/engineer explanations.

> Labs are deterministic teaching simulations. They do not contact your server, execute Wasm, write disks, or generate benchmark results.

## Measured snapshot — not a universal speed claim

The maintainer supplied the following benchmark for **v0.27.1 on 2026-11-04 UTC**:

| Measurement | Result | Scope |
|---|---:|---|
| Mean recorded Wasm execution | **180.9 µs** | 23 executions, including 3 warmups; empty guest returning `{}` |
| Mean global-pool wait | **1.45 µs** | Same 23 executions; sequential requests |
| Client wall time per request | **6.10 ms** | 20 sequential curl invocations; new connection and process each time |
| Successful executions | **23 / 23** | Reported counter and audit verification |
| Cold source build | **8,988 ms** | Fresh temporary directory; not execution latency |

The execution timer covers `Store::new`, asynchronous instantiation, `call_run`, and JSON result parsing. It excludes HTTP handling, module lookup, quota acquisition, and audit persistence. Pool wait is a separate timer. The 6.10 ms wall time must not be presented as pure runtime overhead or inverted into a throughput guarantee.

**Important:** the benchmark reports v0.27.1, while the crate manifests in this checkout declare `0.1.0`. The benchmark guest `empty3` is not the checked-in `sample.wasm`. These results are preserved as supplied, not independently rerun during this documentation update.

[Read the method, raw values, limitations, and reproduction procedure →](docs/benchmarks.md)

## Choose your path

| You want to… | Start here |
|---|---|
| Run your first module locally | [Quick start](docs/quickstart.md) |
| Understand every major runtime component | [Architecture](docs/architecture.md) |
| Write a plugin | [Plugin guide + WIT contract](docs/plugins.md) |
| Integrate over HTTP or Rust | [API reference](docs/api.md), [SDK and CLI](docs/sdk-cli.md) |
| Tune memory, timeouts, tenants, or capabilities | [Configuration](docs/configuration.md) |
| Deploy, back up, restore, and observe | [Operations](docs/operations.md) |
| Evaluate the actual security boundary | [Security and known limitations](docs/security.md) |
| Build or inspect the appliance OS | [WraithOS](docs/wraithos.md) |
| Verify performance claims | [Benchmarks](docs/benchmarks.md) |
| Find the purpose of a repository file | [Complete source map](docs/source-map.md) |
| Contribute or rerun checks | [Testing and contributing](docs/testing.md) |

## Quick start

### 1. Prerequisites

- Linux is the primary target; the deployment scripts also assume Bash, curl, and systemd user services.
- Rust **1.85 or newer**, with Cargo.
- The `wasm32-wasip2` target for building guests.
- For the supplied static-musl deployment path: `cargo-zigbuild`, Zig, and the `x86_64-unknown-linux-musl` target.
- Only if enabling source builds: Bubblewrap, GNU tar, a rustup toolchain, and a pre-fetched Cargo dependency cache.

```bash
rustup target add wasm32-wasip2
cargo build --locked -p orchestrator
cargo build --locked -p sample-plugin --target wasm32-wasip2 --release
```

### 2. Start the service

Run from the repository root so relative paths resolve correctly:

```bash
./target/debug/orchestrator --config orchestrator.toml
```

The supplied configuration binds to `127.0.0.1:9090`. Despite its name, **`metrics_bind` is the listener for the entire API and dashboard**, not just Prometheus.

```bash
curl --fail http://127.0.0.1:9090/healthz
curl --fail http://127.0.0.1:9090/readyz
```

### 3. Execute the included module

The tracked `sample.wasm` calls `example.com` over HTTP and TCP and emits an event. It needs outbound network access; a capability or transport failure can appear inside an HTTP 200 result because the sample catches those guest-side errors.

```bash
curl --fail -X POST \
  http://127.0.0.1:9090/modules/sample/execute \
  -H 'Content-Type: application/json' \
  -d '{"hello":"wraith"}'
```

For a network-free first request, build the minimal guest in the [plugin guide](docs/plugins.md).

### 4. Inspect the result

- Dashboard: `http://127.0.0.1:9090/ui`
- Readiness: `http://127.0.0.1:9090/readyz`
- Metrics: `http://127.0.0.1:9090/metrics`
- Audit: `http://127.0.0.1:9090/audit/logs`

> The dashboard's execute button does not attach a Bearer token, and its event socket hard-codes `ws://`. With authentication enabled or behind HTTPS, use the API/CLI or adapt the embedded UI; see [known limitations](docs/security.md#dashboard-and-wizard).

## The guest contract

Every plugin implements this WIT world:

```wit
world plugin {
    import http;
    import tcp;
    import events;
    export run: func(payload: string) -> result<string, string>;
}
```

The host serializes incoming JSON to a string. The guest returns a string containing valid JSON. A minimal guest can be entirely offline:

```rust
wit_bindgen::generate!({ path: "wit", world: "plugin" });

struct Empty;

impl Guest for Empty {
    fn run(_payload: String) -> Result<String, String> {
        Ok("{}".to_string())
    }
}

export!(Empty);
```

`Component::from_binary` expects a **Wasm component**, not an arbitrary `wasm32-unknown-unknown` core module. See [Plugins](docs/plugins.md) for complete manifests, build commands, capabilities, errors, and event rules.

## API at a glance

| Method | Path | Purpose |
|---|---|---|
| `GET` | `/healthz` | Process liveness; plain text `ok` |
| `GET` | `/readyz` | SQLite query and available global Wasm capacity |
| `GET` | `/metrics` | Prometheus exposition |
| `GET` | `/debug/state` | Modules, pool, sockets, uptime |
| `GET / POST` | `/modules` | List/upload in the default tenant |
| `PUT / DELETE` | `/modules/{id}` | Replace/delete a module |
| `POST` | `/modules/{id}/execute` | Execute with a JSON body |
| `GET / PUT` | `/modules/{id}/quota` | Read/replace module quota |
| `POST` | `/modules/build` | Compile uploaded source; disabled by default |
| `GET` | `/audit/logs` | Latest 50 audit rows across tenants |
| `GET` | `/ui`, `/wizard` | Embedded dashboard and setup UI |
| `GET` | `/ws/events` | Live EventBus stream |

Tenant-specific module routes use `/t/{tenant}/modules…`. Uploads use `X-Module-Id` and optional quota/capability/signature headers. The [API reference](docs/api.md) documents exact payloads, status codes, authentication behavior, and caveats.

## Configuration essentials

```toml
metrics_bind = "127.0.0.1:9090"
modules_dir = "./modules"
db_path = "orchestrator.db"
data_dir = "./data"

max_concurrent_wasm = 2
wasm_acquire_timeout_secs = 10
wasm_memory_limit_mb = 256
max_sockets = 256
http_timeout_secs = 30

require_auth = false
enable_build = false
enable_build_network = false
trusted_signer_keys = []
```

These are development defaults, **not a public-internet hardening profile**. `require_auth=false` leaves tokenless tenants open. A capability header can grant permissions beyond the global fallback; it is not intersected with that fallback. Signature enforcement also has persistence gaps. Read [Configuration](docs/configuration.md) and [Security](docs/security.md) together.

## Repository layout

```text
orchestrator/       Rust host, HTTP API, Wasmtime bridge, Rhai, UI, tests
shared-types/       Manifest, capability, and quota data types
sample-plugin/      Rust guest using HTTP, TCP, and events
wraith-sdk/         Async Rust HTTP client
wraith-cli/         `wraith` command-line application
modules/default/    Checked-in sample component and quota sidecar
deploy/             systemd, backup/restore, packaging, signing, smoke helpers
wraithos/           x86 and ARM Buildroot trees, boot scripts, Termux helpers
docs/               English guides, benchmark evidence, interactive documentation
HANDOFF.md          Historical development notes; not the current source of truth
TODO.md             Historical planning list; verify against source before using
```

The [source map](docs/source-map.md) covers every file tracked at the documented baseline, including boot overlays, legacy backups, binary artifacts, and generated/vendor files.

## Implementation status and trust

**Present in source:** component execution; HTTP/TCP/event host bridges; lifecycle API; quota gates; tenant configuration; optional global authentication; conditional Bubblewrap source builds; signatures; dashboard/WebSocket; SDK/CLI; first-boot wizard handlers; ARM boot-state/watchdog scripts; release helper scripts.

**Not established by this documentation:** a fresh OS image build, a reproducible benchmark rerun, independent penetration testing, complete tenant isolation, crash-safe update transactions, or a finished setup-to-login/network/encryption integration.

Some historical notes describe features as planned even though code now exists. Other notes describe successful QEMU runs that cannot be independently verified from this checkout alone. The new guides label those claims explicitly.

## Verification and documentation maintenance

The source contains **23 Rust unit tests and 10 HTTP integration tests**. Some integration paths require access to `example.com`; they are not a hermetic security suite. Rust tests were not run for this documentation-only update because Cargo/Rust is unavailable in the documentation workspace.

The documentation site is static and can be rebuilt with:

```bash
cd docs
npm ci
npm run build
npm start
```

See [Testing](docs/testing.md) for the Rust CI commands, documentation browser checks, and a release verification checklist. No runtime behavior was changed to produce these guides.

## License

Copyright (c) 2026 kurohigee07.

Licensed under either [MIT](LICENSE-MIT) or [Apache License 2.0](LICENSE-APACHE), at your option. Third-party components retain their own licenses; the documentation fonts include their license files in `docs/assets/fonts/`.
