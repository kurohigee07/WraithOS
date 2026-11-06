# Plugin development guide

A plugin is a WebAssembly **component** implementing the host's WIT world. You can start without understanding every Wasmtime internal: compile a small Rust library, upload it, and call its `run` export over HTTP.

## The contract is the source of truth

`orchestrator/wit/host-capabilities.wit` defines:

```wit
package orchestrator:host;

interface http {
    get: func(url: string) -> result<string, string>;
}
interface tcp {
    connect: func(host: string, port: u16) -> result<u32, string>;
    read: func(handle: u32, max-bytes: u32) -> result<list<u8>, string>;
    write: func(handle: u32, data: list<u8>) -> result<u32, string>;
    close: func(handle: u32) -> result<_, string>;
}
interface events {
    emit: func(kind: string, payload-json: string) -> result<_, string>;
}
world plugin {
    import http;
    import tcp;
    import events;
    export run: func(payload: string) -> result<string, string>;
}
```

WIT strings are UTF-8. The host adds a JSON convention on top: input is serialized JSON and output must be serialized JSON. The component-model world, not a Rust trait name alone, determines compatibility.

## Complete offline example

The runnable files are in `docs/examples/empty-plugin/`. Its manifest declares a standalone workspace so it is not accidentally treated as a host workspace member.

`Cargo.toml`:

```toml
[package]
name = "empty-plugin"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]
wit-bindgen = "0.37"

[workspace]
```

`src/lib.rs`:

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

The example includes a copy of the host WIT file. If the contract changes, update that copy deliberately. Build from the repository root:

```bash
rustup target add wasm32-wasip2
cargo build --manifest-path docs/examples/empty-plugin/Cargo.toml \
  --target wasm32-wasip2 --release
```

Upload and execute:

```bash
curl --fail -X POST http://127.0.0.1:9090/modules \
  -H 'X-Module-Id: empty' \
  --data-binary @docs/examples/empty-plugin/target/wasm32-wasip2/release/empty_plugin.wasm

curl --fail -X POST http://127.0.0.1:9090/modules/empty/execute \
  -H 'Content-Type: application/json' -d '{}'
```

This example is behaviorally equivalent to the benchmark guest, not a byte-for-byte reproduction of `empty3`.

## Input and output patterns

### Echo a JSON value

```rust
fn run(payload: String) -> Result<String, String> {
    serde_json::from_str::<serde_json::Value>(&payload)
        .map_err(|e| e.to_string())?;
    Ok(payload)
}
```

This requires adding serde_json to the guest manifest. The host already validates incoming HTTP JSON, but validating again makes the guest reusable outside this host.

### Return a typed object

Use a JSON library rather than interpolating untrusted strings into JSON. The checked-in sample has a small escape helper for its limited output; a real plugin should prefer serde_json or an equivalent serializer.

```rust
#[derive(serde::Serialize)]
struct Output { ok: bool, message: String }

fn run(_payload: String) -> Result<String, String> {
    serde_json::to_string(&Output {
        ok: true,
        message: "completed".into(),
    }).map_err(|e| e.to_string())
}
```

Returning `Ok("not json")` fails at the host's result parser. Returning `Err("reason")` becomes a guest error. Neither is a special HTTP response object.

## HTTP from a guest

```rust
match orchestrator::host::http::get("https://example.com") {
    Ok(body) => Ok(format!("{{\"bytes\":{}}}", body.len())),
    Err(error) => Err(error),
}
```

The host checks the hostname allowlist before sending. It rejects non-success HTTP status and follows no redirects. `http_timeout_secs` controls the outbound client timeout. There is no guest-visible response-header API or dedicated response-size cap in this contract.

A denied host returns `Err` to the guest. You may propagate it (usually HTTP 500) or catch it and return a structured report (potentially HTTP 200). Define your application's error semantics explicitly.

## TCP from a guest

```rust
let handle = orchestrator::host::tcp::connect("example.com", 80)?;
let request = b"GET / HTTP/1.0\r\nHost: example.com\r\n\r\n";
orchestrator::host::tcp::write(handle, request)?;
let bytes = orchestrator::host::tcp::read(handle, 8192)?;
orchestrator::host::tcp::close(handle)?;
```

A read returns available bytes up to the requested maximum; it is not guaranteed to return a complete protocol message. Implement framing and handle partial reads. `write` uses `write_all` internally and reports the buffer length on success. The contract exposes raw TCP, not TLS; use the HTTP bridge for verified HTTPS.

Close handles on every path you control. The current registry is global and owner-keyed by module ID; it is not automatically cleared when an invocation ends. Avoid handle leaks and do not treat this as strong isolation between identically named modules in different tenants.

## Emit an event

```rust
orchestrator::host::events::emit(
    "job.finished",
    "{\"job_id\":\"demo\",\"ok\":true}"
)?;
```

The second argument is a JSON string. Capability kinds match exactly unless `*` is configured. The bus adds `source_module` from the host manifest; the guest does not choose that field.

Observe events at `/ws/events` or in the dashboard. Delivery is best-effort broadcast, not a durable job queue. Do not use it as the sole record of a financial or irreversible action.

## Rhai rules

An operator-configured script can react to guest events:

```rhai
log_info("event: " + event_kind);

if event_kind == "job.finished" {
    emit_event("job.observed", "{\"observer\":\"rhai\"}");
}
```

`event_payload` is a string. The current worker exposes only `log_info` and `emit_event` plus those two variables; it does not automatically provide JSON parsing, HTTP, SQL, or arbitrary Rust callbacks. Events emitted by source `rhai` are skipped by that worker to prevent self-loops.

The script is trusted code without an explicit fuel/time limit. Keep it small and controlled by the operator, not uploaded by arbitrary tenants.

## Capabilities and quotas at upload

```bash
curl --fail -X POST http://127.0.0.1:9090/modules \
  -H 'X-Module-Id: jobs' \
  -H 'X-Module-Quota: {"max_memory_mb":64,"max_concurrent":1,"rate_limit_per_sec":5}' \
  -H 'X-Module-Capabilities: [{"type":"http-get","allowed_hosts":["api.example.com"]},{"type":"emit-event","allowed_kinds":["job.finished"]}]' \
  --data-binary @jobs.wasm
```

The capability header is a permission grant, not an intersection with global policy. Only trusted uploaders should set it. Omitting it uses the global fallback. An explicitly empty array is not a durable deny-all after restart because the loader treats an empty sidecar as fallback. For no host access, use a guest that makes no host calls and enforce an operator-controlled deployment policy until this boundary is hardened.

`db-query` and `log` exist in shared capability types but are not WIT guest imports. Rhai logging uses the global log permission.

## Build from source

The optional HTTP compiler expects an archive root like:

```text
Cargo.toml
src/lib.rs
wit/host-capabilities.wit
```

Create it from inside a standalone guest directory:

```bash
tar -czf /tmp/guest-source.tar.gz Cargo.toml src wit
```

Do not archive the parent directory itself: the server looks for `Cargo.toml` at extraction root. The crate must produce `target/wasm32-wasip2/release/<package_name_with_underscores>.wasm` through a `cdylib` library.

Server prerequisites:

- `enable_build=true`; startup requires Bubblewrap.
- Cargo at `<cargo_home>/bin/cargo` and a discoverable rustup toolchain.
- The `wasm32-wasip2` target installed in that toolchain.
- With `enable_build_network=false`, all crates/build dependencies must already be available in the Cargo cache. The cache is mounted read-only; a cold dependency fetch cannot populate it in offline mode.
- With network enabled, the sandbox shares the network namespace; this increases exfiltration risk from build scripts.

The server uses `bwrap --unshare-all`, read-only system/toolchain mounts, a writable source directory, tmpfs home/tmp, and optional `--share-net`. It allows one build at a time and kills the child on drop/timeout. This is mitigation, not an audited multi-tenant compiler boundary. Tar extraction happens before the Cargo sandbox; archive trust and resource consumption still matter.

The outer HTTP timeout is 60 seconds while Cargo's timeout is 300 seconds. A client timeout does not prove compilation stopped. Poll module state/logs rather than blindly retrying and creating competing work.

**Persistence caveat:** this path does not verify `X-Module-Signature`, save a signature sibling, or persist a capabilities sidecar. It uses global capabilities. With signing enforcement enabled, a built module may be skipped on the next startup. Prefer offline compilation plus a controlled signing/deployment workflow for reproducible releases.

## Sign a component

Module signatures are raw Ed25519 over the exact `.wasm` bytes. They are separate from GPG release signatures.

Using a trusted signing tool that supports raw Ed25519 keys, produce a 64-byte signature. At upload, encode it as 128 hex characters in `X-Module-Signature`. For disk loading, place it beside the component as `<id>.wasm.sig` in raw binary form and configure the corresponding 32-byte public key as hex in `trusted_signer_keys`.

The repository includes signing helpers for **release tarballs**, not a complete raw-Wasm signing CLI. Do not feed a GPG `.asc` signature to the module verifier. See [signature lifecycle gaps](security.md#signatures).

## Compatibility and debugging checklist

- Build a component for `wasm32-wasip2`, not a core module for an unspecified target.
- Use the exact WIT package/world understood by the host.
- Export `run` through the generated guest bindings.
- Return valid JSON text on success.
- Keep guest dependencies small; linear-memory caps do not include all host overhead.
- Inspect startup warnings if a disk module is skipped.
- Check capability errors inside sample-style HTTP 200 reports.
- Treat WIT/bindgen/Wasmtime upgrades as coordinated host/guest changes and rerun integration tests.
