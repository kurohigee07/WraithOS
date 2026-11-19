# Benchmark: an empty component on benchmark host

> **See also:** [`benchmarks-v0.47.0.md`](benchmarks-v0.47.0.md) -- 2026-11-12 measurement with a batched audit writer. `/execute` throughput rises from ~950 req/s to ~9,800 req/s at concurrency 5-10 on the same host, with the empty guest as the fixture. The v0.27.1 numbers below are preserved as-is.


**Evidence status:** maintainer-supplied measurement dated **2026-11-04 08:07:39 UTC**. It was not independently rerun for this documentation update. Raw values are preserved in [benchmark-data.json](benchmark-data.json), and a reproducible procedure is provided below.

## Results at a glance

| Metric | Value | What it measures |
|---|---:|---|
| Mean execution duration | **180.854 µs** | Sum/count over 23 executions, including three warmups |
| Mean global-pool wait | **1.450 µs** | Separate sum/count over the same 23 executions |
| Client wall time | **6.10 ms/request** | 20 sequential curl processes after warmup |
| Reported successful executions | **23/23** | Execution counter and audit verification |
| Cold source build | **8,988 ms** | One full compile in a fresh temporary directory |
| Guest artifact size | **68,405 bytes** | The reported `empty3` component, not the checked-in sample |

The interactive benchmark explorer displays these scopes separately. It does not turn an empty no-I/O benchmark into a production throughput or tail-latency claim.

## Environment reported by the maintainer

```text
Orchestrator label: v0.27.1
Platform:          benchmark host
Kernel:            6.9.0-207.111.5.4.el9uek.x86_64
Binary target:     x86_64-unknown-linux-musl, static, stripped
Runtime:           Wasmtime 48.0.8, Cranelift JIT
Global Wasm cap:   2
Memory cap:        256 MiB per instance
Pool wait timeout: 10 seconds
Client/server:     curl → 127.0.0.1:9090
Concurrency:       sequential; a new TCP connection per request
```

The source lockfile contains Wasmtime 48.0.8. The crate manifests, however, declare version `0.1.0`; historical notes and the supplied benchmark use different release labels. The exact benchmark commit/build command was not supplied. **Do not assume the reported v0.27.1 binary is byte-identical to a binary built from this checkout.**

## Guest and procedure

The guest exports the standard `run(string) -> result<string,string>` function and immediately returns `"{}"`. It performs no I/O or intentional computation.

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

Reported procedure:

1. Upload source tar.gz to `POST /modules/build`; compile and load as `empty3`.
2. Execute three warmup requests and discard their client timings.
3. Execute 20 sequential requests inside a shell `time` block.
4. Read `/metrics` and verify `/audit/logs`.

The metrics count is **23**, because warmups are included. Dividing the execution sum by only 20 would mix scopes and produce the wrong mean.

## Raw server values

```text
wasm_executions_total{module="empty3",tenant="default",result="ok"} 23

wasm_pool_wait_seconds_sum{module="empty3",tenant="default"} 0.000033360999999999994
wasm_pool_wait_seconds_count{module="empty3",tenant="default"} 23

wasm_execution_duration_seconds_sum{module="empty3",tenant="default"} 0.004159642
wasm_execution_duration_seconds_count{module="empty3",tenant="default"} 23
```

All reported quantiles for both histograms are zero, including quantile 1. The supplied explanation attributes this to exporter resolution at sub-millisecond durations. That explanation is preserved as context, not independently validated here. **Zero quantiles are not evidence that every request took zero microseconds.** Use sum/count for the reported average; do not infer p95/p99 from these values.

First three supplied audit rows:

```json
{"module_id":"empty3","status":"ok","duration_ms":0,"error_message":null,"executed_at":"2026-11-04 08:07:39"}
{"module_id":"empty3","status":"ok","duration_ms":0,"error_message":null,"executed_at":"2026-11-04 08:07:39"}
{"module_id":"empty3","status":"ok","duration_ms":0,"error_message":null,"executed_at":"2026-11-04 08:07:39"}
```

The real API also returns `id` and `tenant`; the excerpt above preserves the maintainer's abbreviated rows. Integer millisecond rounding explains why a roughly 181 µs execution can appear as `0 ms`.

## Derivations

```text
Execution mean = 0.004159642 seconds / 23
               = 0.000180854 seconds
               = 180.854 microseconds

Pool-wait mean = 0.000033361 seconds / 23
               ≈ 0.000001450478 seconds
               ≈ 1.450478 microseconds

Client mean   = 0.122 seconds / 20
               = 0.00610 seconds
               = 6.10 milliseconds

Execution mean / client mean ≈ 2.9648%
```

The roughly 97.04% remainder is **outside the measured execution interval**. It can include HTTP parsing/routing, authentication/lookup/quota work, response framing, TCP setup, curl process startup, and scheduling. The supplied information does not quantify each component, so attributing all of it specifically to curl would be unjustified.

Pool wait is not included in execution duration. Do not add the client, execution, and build bars as though they were consecutive phases of one request.

## What the execution timer includes

The timer in `WasmModule::execute_quota` starts just before `execute_impl` and stops when its future returns. That interval includes:

- Choosing the effective memory limit and creating `Store<WasmHostState>`.
- Setting epoch/memory limits and creating WASI state.
- `Plugin::instantiate_async`.
- Serializing the input JSON to a string.
- `call_run` and guest execution.
- Parsing the returned string as JSON.

It excludes module lookup, tenant/module/global permit acquisition, HTTP extraction/routing, audit insertion, and recording the execution histogram. Global-pool wait has its own histogram. There is no persistent instance pool: a fresh Store is created for each accepted request.

## What this benchmark demonstrates

- In this specific local, sequential, empty-guest setup, the recorded mean execution interval was sub-millisecond.
- Reusing the Engine/Component while creating a fresh Store can still yield a small measured interval for a trivial guest.
- The global pool did not report meaningful waiting in this sequential test with two permits.
- Metrics and audit provide a way to inspect the measurement rather than relying only on shell wall time.

## What it does not demonstrate

- Throughput at concurrency, sustained requests/second, or behavior with many tenants.
- p95/p99 latency, long-run variance, or behavior under CPU/memory pressure.
- HTTP/TCP guest work, Rhai load, large payloads, audit-disk latency, or cold JIT behavior across many distinct modules.
- That 6.10 ms is the service's network latency for persistent HTTP connections.
- That a different machine, release, compiler, or component will produce the same result.
- Security isolation or production readiness.

The report states zero pool-saturation events. The source does not expose a dedicated saturation counter; low wait and sequential arrival are consistent with that report, but a future reproduction should record pool availability/in-flight and rejection metrics explicitly.

## Reproduce responsibly

Use a disposable/local instance and record the exact commit, toolchain, flags, config, CPU/memory limits, and guest hash. Source builds are disabled by default and require Bubblewrap plus a prepared Cargo cache/toolchain. For a safer latency-only reproduction, compile the [offline example](examples/empty-plugin/Cargo.toml) locally and upload the component instead of enabling remote compilation.

Set authentication if configured:

```bash
BASE=http://127.0.0.1:9090
AUTH=()
if [ -n "${WRAITH_TOKEN:-}" ]; then
  AUTH=(-H "Authorization: Bearer $WRAITH_TOKEN")
fi
```

Warmup:

```bash
for i in 1 2 3; do
  curl --fail -sS -o /dev/null -X POST \
    "$BASE/modules/empty3/execute" "${AUTH[@]}" \
    -H 'Content-Type: application/json' -d '{}'
done
```

Measure the supplied 20-process wall-time method:

```bash
time (
  for i in $(seq 1 20); do
    curl --fail -sS -o /dev/null -X POST \
      "$BASE/modules/empty3/execute" "${AUTH[@]}" \
      -H 'Content-Type: application/json' -d '{}'
  done
)
```

Inspect metrics and audit:

```bash
curl --fail -sS "$BASE/metrics" "${AUTH[@]}" | grep 'empty3'
curl --fail -sS "$BASE/audit/logs" "${AUTH[@]}"
```

Counters are cumulative across the process lifetime. For a rigorous rerun, record before/after deltas or use a fresh process with no other executions of that module. Three warmups plus 20 measured requests should add 23 to the relevant execution count, not necessarily make the absolute counter equal 23.

To reproduce the **source-build** portion, enable the build route only in a trusted test environment, package a standalone crate with `Cargo.toml`, `src/`, and `wit/` at archive root, then:

```bash
curl --fail -X POST "$BASE/modules/build" "${AUTH[@]}" \
  -H 'X-Module-Id: empty3' \
  -H 'Content-Type: application/octet-stream' \
  --data-binary @empty3-source.tar.gz
```

Record returned `build_ms`, artifact size/hash, compiler version, cache state, and sandbox network setting. The original 8,988 ms cold build is not reproducible from timing alone without those details.

## Reporting checklist for future benchmarks

Include commit and dirty-tree status; binary hash; Rust/Wasmtime versions; CPU model/count and memory; OS/kernel; virtualization; build profile/linker; config; guest source/hash; warmup policy; request count/concurrency; connection reuse; payload size; client implementation; before/after metrics; audit errors; and percentile methodology. Report failed requests rather than silently dropping them. Keep build time, execution interval, pool wait, and end-to-end latency separate.
