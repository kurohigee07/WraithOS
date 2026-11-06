# Configuration reference

The runtime deserializes `orchestrator.toml` into `orchestrator/src/config.rs`. Missing fields use Rust defaults. Relative paths resolve against the process working directory. Unknown TOML sections are not rejected by the current serde configuration: a typo can be silently ignored.

## Three configurations are not the same thing

1. **Rust defaults:** `Config::default()` when a field is omitted or the selected config file does not exist.
2. **Repository development config:** root `orchestrator.toml`, explicitly passed in the quick start.
3. **Appliance config:** `etc/wraith.default/orchestrator.toml` seeded into persistent storage by WraithOS scripts.

Do not infer the appliance's effective config from the root file. Mounts and working directories differ.

## Complete top-level field reference

| Field | Rust default | Meaning / important behavior |
|---|---|---|
| `db_path` | `orchestrator.db` | SQLite file; created if missing; WAL enabled; parent directory must exist. |
| `data_dir` | `./data` | Wizard marker/config storage. It does not automatically relocate the DB or modules. |
| `modules_dir` | `./modules` | Tenant subdirectories containing components and sidecars. |
| `http_timeout_secs` | `30` | Timeout of the **guest outbound reqwest client**, not the Axum request timeout. |
| `max_sockets` | `256` | Global live TCP-handle cap; must be nonzero for normal startup. |
| `max_concurrent_wasm` | `2` | Global execution permit count; must be nonzero. Not an instance cache. |
| `wasm_acquire_timeout_secs` | `10` | Wait budget for global, tenant, and module concurrency permits, applied at each relevant gate. |
| `wasm_memory_limit_mb` | `256` | Per-Store linear-memory ceiling and upper bound for explicit module memory quotas. |
| `max_db_connections` | `8` | SQLite pool maximum; must be nonzero. |
| `log_level` | `info` | Tracing filter when `RUST_LOG` is absent. |
| `log_format` | `pretty` | `pretty` or `json`; CLI can override. |
| `metrics_bind` | `Some("127.0.0.1:9090")` | Socket address for the entire router. Empty string or omitted/null distinction described below disables listener spawning. |
| `default_tenant` | `default` | Tenant used by legacy `/modules` routes. |
| `require_auth` | `false` | Adds global default-token middleware except on health/readiness. |
| `enable_build` | `false` | Registers source-build routes and requires Bubblewrap at startup. |
| `enable_build_network` | `false` | Shares network namespace in the build sandbox; otherwise Cargo offline mode. |
| `cargo_home` | `$HOME/.cargo` | Cargo installation/cache root. If HOME is missing, main falls back to `/home/opc/.cargo`. |
| `trusted_signer_keys` | `[]` | Ed25519 public keys as 64-character hex strings; nonempty enables component signature checks. |
| `[script]` | Inline logging + emission | Exactly one of `inline` or `path` must resolve to a script. |
| `[tenants.<name>]` | Empty map plus default entry | Token and optional tenant resource caps. |
| `[[capabilities]]` | Log, HTTP example.com, TCP 443 | Global fallback host-call policy; an explicitly empty TOML array means no fallback permissions. |

### `metrics_bind` subtleties

- Set `metrics_bind = "127.0.0.1:9090"` for local development.
- Set `metrics_bind = ""` to disable the HTTP listener entirely; the process can still run its workers.
- Because the Rust field is `Option<String>` with a serde container default, **omitting the field in an existing TOML file deserializes it as null**, which also disables the listener. The `Config::default()` value applies when the whole config is defaulted, not as a per-field fallback in this case. Explicitly set the address in every real config.
- Binding `0.0.0.0:9090` exposes every route on that interface. There is no built-in TLS terminator or separate admin listener. Use a firewall and authenticated reverse proxy as appropriate.

## CLI overrides

```text
orchestrator [OPTIONS]
  -c, --config <PATH>       Default: orchestrator.toml
      --modules-dir <PATH>
      --log-level <FILTER>
      --log-format <pretty|json>
      --dry-run
  -h, --help
  -V, --version
```

Only the listed fields have CLI overrides. There is no CLI flag for auth, bind address, memory, or build enablement. `RUST_LOG` overrides the tracing filter after config/CLI selection.

A nonexistent config path produces a bootstrap warning and defaults; a readable but malformed TOML file is fatal. `--dry-run` is a parsing check, not an integration readiness test.

## Resource limits and what they actually bound

### Global Wasm concurrency

```toml
max_concurrent_wasm = 2
wasm_acquire_timeout_secs = 10
```

At most two accepted executions pass the global gate concurrently. Other requests may wait at tenant/module gates first. Waiting is not free: it consumes HTTP connections and async tasks. Global pool timeout currently becomes HTTP 500; tenant/module concurrency timeout becomes 503.

### Linear memory

```toml
wasm_memory_limit_mb = 256
```

This configures Wasmtime `StoreLimits::memory_size`. It is principally a **Wasm linear-memory limit**, not a resident-set limit for the whole process. JIT compilation, host allocations, HTTP bodies, database cache, and Rust stacks require separate OS-level capacity planning.

A module quota may lower the limit. A nonzero value greater than the global ceiling is rejected when building quota state. Zero inherits the global value. Very large values are not a substitute for validation; use conservative operator-controlled numbers.

### Epoch interruption

The source fixes:

```text
EPOCH_TICK = 100 ms
EPOCH_DEADLINE_TICKS = 600
approximate deadline = 60 s
```

These are not TOML settings. Deadline is set on each fresh Store before instantiation. It is wall-clock epoch progression, not a precise per-module CPU-time quota. Host calls have their own behavior; TCP operations do not have a dedicated configurable timeout. The router has a fixed 60-second request timeout.

### Outbound HTTP and sockets

`http_timeout_secs` bounds the reqwest request. `max_sockets` bounds registry entries. HTTP hostname allowlisting is not an IP egress firewall. TCP configuration currently controls ports, not destination hosts. Close guest handles explicitly.

## Capability syntax

Capabilities use an internally tagged serde enum with kebab-case names:

```toml
[[capabilities]]
type = "log"

[[capabilities]]
type = "http-get"
allowed_hosts = ["example.com", "*.trusted.example"]

[[capabilities]]
type = "tcp-connect"
allowed_ports = [80, 443]

[[capabilities]]
type = "emit-event"
allowed_kinds = ["sample.emitted", "job.finished"]

[[capabilities]]
type = "db-query"
allowed_tables = ["example"]
```

- HTTP: exact hostname, `*.domain` proper-subdomain suffix, or `*`. Matching is ASCII-case-insensitive. `*.example.com` does not match `example.com` itself.
- TCP: allowed port numbers. No host field exists in the current shared type.
- Events: exact kind or `*`.
- Log: controls Rhai `log_info`; there is no WIT log import.
- DB query: the type/checker exists, but **no guest DB query import is wired**. Declaring it does not expose SQL to plugins.

The root config is a fallback that uploaders can replace/expand through `X-Module-Capabilities`. It is not an immutable maximum policy. See [capability authorization](security.md#capabilities-are-authorization-data).

## Module quotas

```json
{
  "max_memory_mb": 64,
  "max_concurrent": 2,
  "rate_limit_per_sec": 10
}
```

| Value | Zero means | Positive means |
|---|---|---|
| `max_memory_mb` | Inherit global memory cap | Per-instance cap, no larger than global |
| `max_concurrent` | No additional module semaphore | Module-specific concurrency cap |
| `rate_limit_per_sec` | No module rate limit | Token bucket refill rate and burst capacity |

The bucket starts full. Capacity equals `max(rate, 1)`, so a configured tenant/module rate of zero that reaches `TokenBucket::new` would still allow one token per second; normal quota constructors instead use `None` for zero. Refill is continuous, not a fixed-window counter.

The commented `[default_quota]` block in the root TOML is **not implemented as a Config field**. main constructs `ModuleQuota::default()` directly. Do not expect that section to change defaults.

## Tenant configuration

```toml
default_tenant = "default"
require_auth = true

[tenants.default]
token_hash = "$argon2id$v=19$m=19456,t=2,p=1$REAL_SALT$REAL_HASH"

[tenants.alice]
token_hash = "$argon2id$v=19$m=19456,t=2,p=1$OTHER_SALT$OTHER_HASH"
max_modules = 10
max_concurrent_wasm = 1
rate_limit_per_sec = 20
```

The PHC strings above are placeholders and will not validate. Generate a real hash from a high-entropy random token using a trusted Argon2 tool; never commit the raw token.

- `token_hash` takes precedence over `token_plain`.
- `token_plain` logs a warning and is for development only.
- A tenant with neither token is open at its handler-level verification.
- With `require_auth=true`, a caller must first satisfy the default tenant middleware; a distinct tenant token is then checked again by the tenant route.
- Tenant caps are optional. `max_concurrent_wasm=0` is treated as no extra semaphore, unlike the nonzero global field.
- Tenant names come from trusted config. Use simple path-safe identifiers; the code does not provide a general tenant-name validator.

This is namespace/resource management, not process isolation or a complete customer authorization model.

## Rhai script configuration

Inline:

```toml
[script]
inline = '''
log_info("received: " + event_kind);
emit_event("rule.processed", "{\"ok\":true}");
'''
```

Or file-based:

```toml
[script]
path = "./rules/events.rhai"
```

Do not set both. Do not omit both. The script receives `event_payload` as a JSON **string**, not an automatically parsed object. Scripts are trusted operator code with no explicit execution budget in this version.

## Appliance and Termux differences

- ARM WraithOS uses absolute DB/module/data paths under `/var/lib/orchestrator` and `/modules`, backed by `/data` mounts when available.
- The x86 overlay's `etc/wraith/orchestrator.toml` contains relative paths, while its `wraith.default` seed uses absolute paths. The seed is copied only when persistent config is missing.
- Termux lowers memory to 128 MiB and sockets to 64, but allows HTTP `*`; it is a permissive demo profile.
- The checked-in ARM `wraith-init` is a prebuilt binary. Replacing source does not update that artifact until you cross-compile and copy it into the overlay.

## Validation checklist

```bash
./target/debug/orchestrator --config orchestrator.toml --dry-run
./target/debug/orchestrator --config orchestrator.toml
curl --fail http://127.0.0.1:9090/readyz
```

Then verify effective module quotas/capabilities through `/modules`, test a denied host, and inspect startup warnings. Do not treat successful TOML parsing as proof of a secure policy.
