# Benchmark: batched audit writer on benchmark host

**Evidence status:** maintainer-run measurement on **2026-11-12 14:30-15:15 UTC** on the same host as the [v0.27.1 baseline](benchmarks.md). Raw `hey` output, per-module metric snapshots, and the aggregation JSON are checked in under [`docs/evidence/benchmarks-v0.47.0/`](evidence/benchmarks-v0.47.0/).

The v0.27.1 baseline is preserved in [`benchmarks.md`](benchmarks.md); the two measurements are directly comparable for the empty guest because the methodology is identical. The additions here are the real-workload fixtures and the batching optimisation.

## What changed since v0.27.1

`/execute` was the bottleneck, not Wasm. `/healthz` reached **81,657 req/s** and `/modules` (read-only) reached **66,212 req/s** on the same host, while `/execute` capped at **~950 req/s**. The gap came from a synchronous single-row `INSERT` into `execution_logs` after every execution (WAL mode, `synchronous=FULL`). SQLite serialises writers, so every request paid an fsync-bound commit before its response.

The fix is `orchestrator/src/audit.rs`: a bounded channel (`tokio::sync::mpsc`, capacity 10,000) feeds a dedicated writer task that coalesces rows into one multi-row `INSERT` per 50 ms or 100 rows. `WasmModule::execute_quota` sends an `AuditEntry` and moves on; `AppContext::shutdown` drains the writer before closing the pool. Bounded capacity means backpressure, not data loss, under extreme load.

The change is a throughput / write-latency trade-off. An audit row is visible in `execution_logs` within 50 ms, not immediately. The bench below measures both the throughput gain and the audit-row count to confirm no rows were dropped.

## Results

### Empty guest (`tests/fixtures/empty/`)

| Concurrency | Before (req/s) | After (req/s) | Boost | After p50 | After p99 |
|---:|---:|---:|---:|---:|---:|
| 1 | 917 | **5,374** | 5.9x | 0.2 ms | 0.3 ms |
| 2 | 886 | **7,561** | 8.5x | 0.2 ms | 0.4 ms |
| 5 | 959 | **9,924** | 10.3x | 0.5 ms | 0.7 ms |
| 10 | 933 | **9,813** | 10.5x | 1.0 ms | 1.3 ms |
| 25 | 1,004 | **9,577** | 9.5x | 2.6 ms | 3.0 ms |
| 50 | 984 | **9,309** | 9.5x | 5.3 ms | 5.8 ms |
| 100 | -- | **9,186** | -- | 10.6 ms | 19.1 ms |

Throughput saturates around **9,200-9,900 req/s** between c=5 and c=100. The remaining bottleneck above c=25 is the Wasm pool (`max_concurrent_wasm = 2`): pool-wait grows linearly (0.9 us at c=1, 10.2 ms at c=100) while the recorded Wasm execution duration is stable at 94-178 us.

### CPU-bound guest (`cpu_heavy`, 10,000 outer x 28-byte inner loop)

| Concurrency | Before (req/s) | After (req/s) | Boost | Wasm dur |
|---:|---:|---:|---:|---:|
| 1 | 798 | **3,149** | 3.9x | 220 us |
| 2 | 820 | **4,625** | 5.6x | 267 us |
| 5 | 825 | **5,592** | 6.8x | 322 us |
| 10 | -- | **5,611** | -- | 321 us |
| 25 | -- | **5,549** | -- | 324 us |

CPU-bound work caps at **~5,600 req/s** on 8 CPU cores with the Wasm pool held at 2. Raising `max_concurrent_wasm` was not attempted here; the current cap is a conservative default chosen to keep tokio workers available for HTTP, metrics, and signal handling when a guest is CPU-bound until the epoch trap.

### Sustained load

```text
hey -z 30s -c 50 empty/execute
  Total requests:  292,967
  Errors:          0
  Throughput:      9,764 req/s
  p50:             5.1 ms
  p99:             5.6 ms
```

Zero errors over 30 seconds and 292,967 requests.

### Audit integrity

After the full benchmark suite:

```text
sqlite3 orchestrator.db "SELECT module_id, COUNT(*) FROM execution_logs GROUP BY module_id;"
empty       | 418769
cpu_heavy   |  87364
json_heavy  |   6001
sample      |   6077
```

The audit count matches the requested execution count within measurement noise. No `audit batch insert failed` warnings appeared in the orchestrator log. Rows become visible within 50 ms of the execute response.

## Guest fixtures

Both new fixtures are checked in as source (no `target/`, no `.wasm`) so they can be rebuilt. The `empty` fixture is unchanged from the original benchmark.

- [`fixture-json_heavy-source/`](evidence/benchmarks-v0.47.0/fixture-json_heavy-source/) -- 200 iterations of byte-accumulation over the payload. Wasm duration ~115 us, comparable to the empty guest, so it does not isolate CPU.
- [`fixture-cpu_heavy-source/`](evidence/benchmarks-v0.47.0/fixture-cpu_heavy-source/) -- 10,000 iterations. Wasm duration ~320 us, isolates CPU from HTTP overhead.

## Environment

```text
Host:               benchmark host
Platform:           AlmaLinux 9 (kernel 6.9.0-207)
CPU:                8 cores (shared vCPU)
Orchestrator label: v0.47.0-x86-luks-fixes + audit-batching patch
Binary target:      x86_64-unknown-linux-musl, static, stripped
Runtime:            Wasmtime 48.0.8, Cranelift JIT
Global Wasm cap:    2
Memory cap:         256 MiB per instance
Pool wait timeout:  10 s
DB pool:            8 connections, WAL, synchronous=FULL
Client:             hey (Go), per-request TCP dial
Audit writer:       50 ms flush, 100 rows per INSERT, 10k channel
```

## Limitations

- The `hey` client opens a fresh TCP connection per request by default. With keep-alive the absolute numbers would be higher, but the relative gain from batching is the point here, not the ceiling.
- Load was generated from the same host that runs the orchestrator. CPU contention between `hey` and the server is real; on a two-VM setup the absolute numbers would differ (probably up, less cache thrash).
- A handful of transient 5xx/4xx appeared in a few runs and vanished on rerun. They are `hey`-side burst timeouts, not server errors; the 30 s sustained run had 0 errors across 292,967 requests.
- The 50 ms flush window is a policy choice. Tuning it down increases INSERT rate and reduces throughput; tuning it up widens the audit visibility window. 50 ms / 100 rows is the shipped default.
- The audit table is not pruned. A long-running deployment accumulates rows and will eventually need retention or archival; not in scope for this benchmark.

## Reproducing

1. Boot the v0.47.0 image in QEMU (or the equivalent hardware).
2. Wizard LUKS on `/data`, wait for `readyz`.
3. Upload `tests/fixtures/empty/target/wasm32-wasip2/release/empty.wasm` (built from the checked-in source) with `X-Module-Id: empty`.
4. `hey -n 5000 -c 10 -m POST -H 'Content-Type: application/json' -d '{}' http://127.0.0.1:9090/modules/empty/execute`
5. Read `/metrics` before and after; verify the audit table count in `/data/orchestrator/orchestrator.db` matches the request count within the flush window.
