# HTTP API reference

Base URL in development: `http://127.0.0.1:9090`. All module routes are served by the listener configured through `metrics_bind`. There is no built-in TLS, CORS policy, API version prefix, or OpenAPI document in this checkout.

## Authentication and route conventions

`/modules…` addresses the configured default tenant. `/t/{tenant}/modules…` addresses a named tenant. A tenant with no token is open at handler level unless global authentication protects the route.

When `require_auth=true`, all exact paths except `/healthz` and `/readyz` first require the **default tenant's** Bearer token. Tenant module handlers then check their own tenant token. Consequently, a non-default tenant with a different token requires both credentials in the current single-header design; a tokenless non-default tenant is accessible with the default token. Do not assume this provides customer-to-customer isolation.

```bash
curl -H "Authorization: Bearer $WRAITH_TOKEN" http://127.0.0.1:9090/modules
```

Module IDs must contain 1–64 ASCII letters, digits, underscores, or hyphens. The module and build routers limit bodies to 10 MiB. JSON extraction has Axum's usual content-type requirements. The outer request timeout is fixed at 60 seconds.

## Endpoint inventory

| Method | Path | Response / purpose |
|---|---|---|
| GET | `/healthz` | 200 plain text `ok`; always public |
| GET | `/readyz` | JSON readiness; always public |
| GET | `/metrics` | Prometheus text exposition |
| GET | `/debug/state` | Global runtime snapshot |
| GET | `/audit/logs` | Latest 50 execution rows, all tenants |
| GET | `/modules` | Default-tenant list |
| POST | `/modules` | Raw component upload |
| PUT | `/modules/{id}` | Replace or create component |
| DELETE | `/modules/{id}` | Remove module |
| POST | `/modules/{id}/execute` | Execute JSON input |
| GET | `/modules/{id}/quota` | Read quota |
| PUT | `/modules/{id}/quota` | Replace quota and reload component |
| GET | `/t/{tenant}/modules` | Tenant list |
| POST | `/t/{tenant}/modules` | Tenant upload |
| PUT/DELETE | `/t/{tenant}/modules/{id}` | Tenant replace/delete |
| POST | `/t/{tenant}/modules/{id}/execute` | Tenant execution |
| GET/PUT | `/t/{tenant}/modules/{id}/quota` | Tenant quota |
| POST | `/modules/build` | Optional source build, default tenant |
| POST | `/t/{tenant}/modules/build` | Optional source build, named tenant |
| GET | `/ui` | Embedded dashboard |
| GET | `/static/htmx.js` | Locally served HTMX 2.0.4 |
| GET | `/ui/fragments/stats` | HTML statistics fragment |
| GET | `/ui/fragments/modules` | HTML module rows |
| GET | `/ui/fragments/logs` | HTML audit rows, latest 20 |
| GET | `/ui/fragments/metrics` | HTML metrics summary |
| GET | `/ws/events` | WebSocket upgrade; EventBus JSON messages |
| GET | `/wizard` | Embedded first-boot UI |
| GET | `/wizard/status` | Setup state JSON |
| POST | `/wizard/step1` | Hostname/password and optional encryption password |
| POST | `/wizard/step2` | Network configuration |
| POST | `/wizard/finish` | Persist setup; optional destructive LUKS path |

## Health and readiness

### `GET /healthz`

```http
HTTP/1.1 200 OK

ok
```

This is liveness, not proof that SQLite or plugins work.

### `GET /readyz`

Checks `SELECT 1` and a nonzero number of available global pool permits. A registry with zero modules is considered ready.

```json
{
  "status": "ready",
  "modules_loaded": 1,
  "wasm_pool_available": 2,
  "uptime_secs": 42
}
```

Failure is 503 with `reason: "db"` or `reason: "wasm_pool_saturated"`. Saturation is a capacity signal, not necessarily a broken process; choose orchestrator health-check semantics accordingly.

## List modules

```bash
curl --fail http://127.0.0.1:9090/modules
```

```json
{
  "count": 1,
  "modules": [{
    "tenant": "default",
    "id": "sample",
    "version": "0.0.0",
    "capabilities": [],
    "quota": {
      "max_memory_mb": 0,
      "max_concurrent": 0,
      "rate_limit_per_sec": 0
    }
  }]
}
```

`version` is currently a fixed manifest value, not package-version extraction. An empty capability list can still use the global fallback at execution time. List order is not guaranteed.

## Upload a component

```bash
curl --fail -X POST http://127.0.0.1:9090/modules \
  -H 'X-Module-Id: hello' \
  -H 'Content-Type: application/wasm' \
  -H 'X-Module-Quota: {"max_memory_mb":64,"max_concurrent":1,"rate_limit_per_sec":10}' \
  -H 'X-Module-Capabilities: [{"type":"emit-event","allowed_kinds":["hello.done"]}]' \
  --data-binary @empty_plugin.wasm
```

Returns 201:

```json
{
  "tenant": "default",
  "id": "hello",
  "status": "created",
  "quota": {"max_memory_mb":64,"max_concurrent":1,"rate_limit_per_sec":10}
}
```

Headers:

| Header | Required? | Meaning |
|---|---|---|
| `X-Module-Id` | Yes | New module ID; not inferred from filename |
| `X-Module-Quota` | No | JSON quota; omitted means registry default |
| `X-Module-Capabilities` | No | JSON array replacing the global fallback for this module |
| `X-Module-Signature` | When trusted keys are configured | Hex-encoded 64-byte Ed25519 signature over exact uploaded bytes |
| `Authorization` | Depends on auth/tenant config | Bearer token |

An existing ID returns 409; use PUT. Exceeding a tenant's module count returns **507 Insufficient Storage** with `current` and `max`. Upload writes Wasm/quota/capability files, but **does not persist the signature header**. Restart with trusted keys may therefore skip an uploaded module. Build uploads have additional signature/capability gaps; see [Security](security.md).

## Replace or create with PUT

```bash
curl --fail -X PUT http://127.0.0.1:9090/modules/hello \
  -H 'Content-Type: application/wasm' \
  --data-binary @empty_plugin.wasm
```

Returns 200 with `status: "replaced"` or `"created"`. No ID header is required because the path supplies it. Omitted quota preserves an existing module's quota; omitted capabilities use the global fallback rather than preserving the old explicit manifest. Specify capabilities intentionally on replacement.

The old in-memory module is swapped after the new component/files are prepared. Already running calls retain their old `Arc`. Disk writes are individually atomic, not a transaction across all artifacts.

## Execute

```bash
curl --fail -X POST http://127.0.0.1:9090/modules/hello/execute \
  -H 'Content-Type: application/json' \
  -d '{"input":42}'
```

Any JSON value is accepted, including an object, array, string, number, boolean, or null. It is serialized to a WIT string. The guest must return a string containing valid JSON. The response is that decoded JSON value, **not an envelope** containing `result` or `duration`.

For the empty guest, the response is `{}` with status 200. Guest errors/traps/invalid output generally become 500. A guest that catches a host error may return a successful JSON report instead.

Gate order: tenant rate → tenant concurrency → module rate → module concurrency → global pool → fresh Store/instantiation → guest.

## Delete

```bash
curl --fail -X DELETE http://127.0.0.1:9090/modules/hello
```

```json
{"tenant":"default","id":"hello","status":"deleted"}
```

Missing module: 404. Registry removal happens before disk cleanup. File deletion failures are warnings, not a rolled-back API error. Capabilities/signature siblings may remain; see the source-level lifecycle description.

## Quota

```bash
curl --fail http://127.0.0.1:9090/modules/hello/quota
curl --fail -X PUT http://127.0.0.1:9090/modules/hello/quota \
  -H 'Content-Type: application/json' \
  -d '{"max_memory_mb":32,"max_concurrent":1,"rate_limit_per_sec":2}'
```

PUT replaces the complete quota; omitted JSON fields deserialize to zero. It reads the persisted Wasm, reloads the component, writes the quota sidecar, and swaps the registry object. It is not an atomic patch of one field. A missing disk artifact can make quota update fail even if an in-memory module exists.

## Build from source

Only registered when `enable_build=true`. Body is tar or gzip tar, not multipart. `Cargo.toml` must be at archive root. The output name derives from `[package].name` with hyphens replaced by underscores; `X-Module-Id` can override the registered ID.

```bash
tar -czf source.tar.gz Cargo.toml src wit
curl --fail -X POST http://127.0.0.1:9090/modules/build \
  -H 'X-Module-Id: hello' \
  -H 'Content-Type: application/octet-stream' \
  --data-binary @source.tar.gz
```

Successful response is 201 even when replacing:

```json
{
  "tenant":"default",
  "id":"hello",
  "status":"created",
  "size_bytes":68405,
  "build_ms":8988,
  "quota":{"max_memory_mb":0,"max_concurrent":0,"rate_limit_per_sec":0},
  "warnings":[]
}
```

Values above illustrate the schema using benchmark figures, not a response from this checkout. Compiler failure is 400 with a stderr tail; another active build is 503. Only one build runs globally. Cargo timeout is 300 seconds but the HTTP layer times out at 60 seconds. Bubblewrap and an appropriate rustup/Cargo cache are required. Read [source-build restrictions](plugins.md#build-from-source) before enabling this route.

## Audit and debug state

`GET /audit/logs` returns an array ordered by descending ID, limited to 50:

```json
[{
  "id": 23,
  "tenant": "default",
  "module_id": "hello",
  "status": "ok",
  "duration_ms": 0,
  "error_message": null,
  "executed_at": "2026-11-04 08:07:39"
}]
```

There are no filter/pagination parameters. It is global across tenants when accessed. `duration_ms=0` can mean less than one millisecond. Database insertion errors are currently not surfaced through execution status.

`GET /debug/state` returns uptime, all module manifests, global pool total/available, and socket active/max/available. It is an operator endpoint, not a tenant-safe projection.

## Metrics

Important series include:

- `wasm_executions_total`
- `wasm_execution_duration_seconds`
- `wasm_pool_wait_seconds`
- `wasm_pool_in_flight`
- `socket_registry_active`
- `tenant_in_flight`
- `tenant_quota_rejected_total`
- `module_quota_rejected_total`
- `rhai_events_processed_total`
- `rhai_script_duration_seconds`
- `build_total`, `build_duration_seconds`, `build_rejected_total`

Series appear when emitted; a fresh process need not expose every name. Label sets are not fully consistent across early rejection paths. Avoid aggregating by assumptions not stated in [Benchmarks](benchmarks.md).

## WebSocket events

Connect to `/ws/events`. Each bus event is serialized as:

```json
{"kind":"sample.emitted","payload":{},"source_module":"sample"}
```

There is no replay, per-tenant filter, or application-level acknowledgement. The server reads client messages mainly to observe close/disconnect; sending an event over the socket does not publish it. Lagging subscribers can lose messages.

The embedded dashboard uses `ws://` explicitly and does not attach authentication to fetch/HTMX requests. It needs adaptation for HTTPS/Bearer deployments.

## Wizard API

`GET /wizard/status` reports `completed`, `step`, and `luks_enabled`. Step 1 accepts `hostname`, `password`, and optional `encryption_password`; step 2 accepts `network_mode` (`dhcp` or `static`), `static_ip`, `gateway`, and `dns`. Finish takes no body.

These handlers persist setup data but do not constitute a completed login/network/reboot integration. The optional encryption path formats `/dev/vda4` and can destroy data. **Do not call it on a workstation or populated disk.** See [WraithOS wizard caveats](wraithos.md#first-boot-wizard-and-luks).

## Error shapes

Most handler errors use:

```json
{"error":"human-readable explanation"}
```

| Status | Common cause |
|---|---|
| 400 | Invalid ID/JSON/component/quota/signature, source build failure |
| 401 | Missing/invalid Bearer token |
| 404 | Unknown tenant/module, or disabled build route |
| 405 | Existing path with unsupported method; disabled `/modules/build` POST can match the `{id}` route's methods |
| 408 | Outer 60-second HTTP timeout |
| 409 | Duplicate upload or already-completed wizard |
| 413 | Body exceeds route limit |
| 429 | Rate limit; includes `retry_after_ms` and integer `Retry-After` seconds, minimum 1 |
| 500 | Guest/internal error, including global pool acquire timeout |
| 503 | Tenant/module concurrency timeout, build busy, or readiness failure |
| 507 | Tenant module-count cap reached |

Axum extractor failures can use framework-generated plain-text responses rather than the custom JSON shape. Clients should handle status first, then attempt JSON parsing rather than assuming every error body is JSON.
