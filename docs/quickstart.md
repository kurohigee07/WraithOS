# Quick start: from checkout to first execution

This guide uses the repository's development configuration. It does **not** install a public service, enable source compilation, or change your operating system. Allow 15–30 minutes, excluding dependency downloads.

## 1. Understand what you are starting

There are three different programs/artifacts:

| Name | What it is | Where it runs |
|---|---|---|
| `orchestrator` | The HTTP host and Wasmtime runtime | Your Linux machine or appliance |
| `sample-plugin` | Source for a guest component | Compiled to Wasm; executed by the host |
| `wraith` | An HTTP client CLI | Your workstation; it does not execute Wasm itself |

The host listens on the address named `metrics_bind`. Its name is historical: disabling it disables the **whole HTTP listener**, including modules and the dashboard.

## 2. Install the local prerequisites

On a Linux development machine, install Git, curl, a C toolchain, and rustup using your distribution's usual package mechanism. Then:

```bash
rustc --version   # Must be >= 1.85
cargo --version
rustup target add wasm32-wasip2
```

The workspace pins dependencies in `Cargo.lock`. Use `--locked` so a build does not silently select different versions.

> Platform note: the service uses Unix signal handling and Linux-oriented deployment tooling. This guide does not promise a native Windows build. A Linux VM is the least ambiguous starting point.

## 3. Build the host and sample guest

From the repository root:

```bash
cargo build --locked -p orchestrator
cargo build --locked -p sample-plugin --target wasm32-wasip2 --release
```

Expected outputs:

```text
target/debug/orchestrator
target/wasm32-wasip2/release/sample_plugin.wasm
```

The checked-in `modules/default/sample.wasm` is also available. Its size in the documented checkout is **54,639 bytes**, not the 68,405-byte `empty3` benchmark module. A fresh source build can produce different bytes.

## 4. Inspect the configuration before running

Open `orchestrator.toml`. The important development values are:

```toml
metrics_bind = "127.0.0.1:9090"
db_path = "orchestrator.db"
modules_dir = "./modules"
max_concurrent_wasm = 2
wasm_memory_limit_mb = 256
enable_build = false
enable_build_network = false
require_auth = false
```

Paths are relative to the **process working directory**, not automatically relative to the TOML file. Start the process from the repository root.

Check configuration parsing:

```bash
./target/debug/orchestrator --config orchestrator.toml --dry-run
```

`--dry-run` parses configuration and exits after logging initialization. It does not open SQLite, bind the listener, validate every signer key, or prove that the Rhai script compiles. Those happen later in startup. Also avoid sharing dry-run logs publicly: debug-formatted configuration can contain configured plaintext tokens or hashes.

## 5. Start the host

In terminal A:

```bash
./target/debug/orchestrator --config orchestrator.toml
```

Look for:

```text
AppContext initialized
tenant registry ready
module registry ready
metrics server up
```

Exact tracing formatting can vary. A missing module directory is a warning, not necessarily a fatal error. An invalid loaded component is skipped with a warning; check the loaded module count rather than assuming every file was accepted.

In terminal B:

```bash
curl --fail http://127.0.0.1:9090/healthz
curl --fail http://127.0.0.1:9090/readyz
curl --fail http://127.0.0.1:9090/modules
```

- `/healthz` returns plain text `ok` if the process can serve the route.
- `/readyz` queries SQLite and requires at least one free global Wasm permit. It can return 503 under saturation even though the process is alive.
- `/modules` returns JSON with `count` and `modules`.

## 6. Execute your first module

### Option A: the included network-using sample

```bash
curl --fail -X POST \
  http://127.0.0.1:9090/modules/sample/execute \
  -H 'Content-Type: application/json' \
  -d '{"message":"hello"}'
```

The sample attempts:

1. HTTPS GET to `example.com`.
2. A plaintext HTTP request over TCP port 80.
3. Emission of `sample.emitted`.

The result contains separate `http`, `tcp`, and `emit` outcomes. The guest catches host-call errors, so **HTTP 200 does not prove every outbound operation succeeded**. Inspect those fields. The root config allows TCP ports 80 and 443; the built-in Rust defaults allow only 443.

### Option B: a deterministic, offline guest

Build the complete minimal example included with these guides:

```bash
cargo build --manifest-path docs/examples/empty-plugin/Cargo.toml \
  --target wasm32-wasip2 --release

curl --fail -X POST http://127.0.0.1:9090/modules \
  -H 'X-Module-Id: hello' \
  -H 'Content-Type: application/wasm' \
  --data-binary @docs/examples/empty-plugin/target/wasm32-wasip2/release/empty_plugin.wasm

curl --fail -X POST http://127.0.0.1:9090/modules/hello/execute \
  -H 'Content-Type: application/json' \
  -d '{"anything":true}'
```

Expected execution body:

```json
{}
```

The guest ignores the input and returns the JSON string `"{}"`. This is the same behavioral shape as the supplied benchmark, but not a claim of identical binary bytes or performance.

## 7. Inspect what happened

```bash
curl --fail http://127.0.0.1:9090/audit/logs
curl --fail http://127.0.0.1:9090/metrics
curl --fail http://127.0.0.1:9090/debug/state
```

Open `http://127.0.0.1:9090/ui` in a browser. The dashboard polls HTML fragments every three seconds and subscribes to live events.

Audit `duration_ms` is an integer. A sub-millisecond execution can legitimately appear as `0`; that does not mean it took zero time. Use the Prometheus histogram sum/count for a fractional measurement.

## 8. Use the CLI instead of curl

```bash
cargo run --locked -p wraith-cli -- list
cargo run --locked -p wraith-cli -- exec hello -p '{"test":1}'
cargo run --locked -p wraith-cli -- logs
```

To install the binary into Cargo's bin directory:

```bash
cargo install --locked --path wraith-cli
wraith --help
```

Global flags such as `--server`, `--tenant`, and `--token` work before or after the subcommand. Passing a token on the command line can expose it in process listings and shell history; prefer curl with a protected environment variable for sensitive use.

## 9. Change a quota

```bash
curl --fail -X PUT http://127.0.0.1:9090/modules/hello/quota \
  -H 'Content-Type: application/json' \
  -d '{"max_memory_mb":64,"max_concurrent":1,"rate_limit_per_sec":2}'

curl --fail http://127.0.0.1:9090/modules/hello/quota
```

A nonzero memory quota cannot exceed the global memory ceiling. Zero means inherited memory or no additional module rate/concurrency gate; it does not disable the global pool.

A quota PUT reloads the component and replaces its quota state, including token-bucket fullness. In-flight executions keep their old `Arc` and permits until complete.

## 10. Stop cleanly

Press Ctrl+C in terminal A, or send SIGTERM to the process. The implementation stops the Rhai worker and HTTP listener and closes the SQLite pool. Review the [shutdown limitation](architecture.md#shutdown) before relying on bounded shutdown under heavy load.

## Troubleshooting

| Symptom | Check |
|---|---|
| `metrics bind … address in use` | Another process owns port 9090, or `metrics_bind` is not the address you are curling. |
| `failed to parse wasm component` | You uploaded a core Wasm module, arbitrary bytes, or an incompatible component. Build with `wasm32-wasip2` and the supplied WIT world. |
| `X-Module-Id header required` | Upload identity comes from the header, not the filename. |
| 404 for `/modules/build` | Source builds are disabled by default. Read the [build pipeline guide](plugins.md#build-from-source) before opting in. |
| 401 from the dashboard | The embedded UI does not attach Bearer credentials. Use curl/CLI or adapt the UI. |
| 429 | A tenant or module token bucket is empty. Respect `Retry-After` and `retry_after_ms`. |
| 503 with concurrency timeout | A tenant/module permit was not acquired before the configured timeout. |
| 500 mentioning pool acquire timeout | The **global** pool timed out; this path currently maps to 500, not 503. |
| `/readyz` returns 503 | Check the JSON `reason`: SQLite failure or a saturated global pool. |
| Sample result contains network errors | DNS, Internet access, remote service availability, and configured capabilities all matter. Try the offline example. |
| Module disappears after restart with signing enabled | HTTP signature headers are not saved as `.wasm.sig` files. See [signature persistence](security.md#signatures). |

## What to read next

- [Architecture](architecture.md): what happens inside each request.
- [Configuration](configuration.md): every runtime setting and its actual default.
- [Security](security.md): mandatory reading before binding to a non-loopback address.
- [Operations](operations.md): static builds, systemd, backup, restore, and release verification.
