# Rust SDK and CLI

The SDK is a thin HTTP client. It does not embed the orchestrator or execute Wasm locally. The CLI is a Clap application over that client.

## Install and connect

```bash
cargo install --locked --path wraith-cli
wraith --server http://127.0.0.1:9090 list
```

Global options:

| Flag | Default | Purpose |
|---|---|---|
| `--server` | `http://127.0.0.1:9090` | Base URL; trailing slash is trimmed when constructing requests |
| `--token` | None | Bearer token |
| `--tenant` | None | Use named-tenant routes instead of legacy default routes |

```bash
wraith --tenant alice --token "$WRAITH_TOKEN" list
wraith exec hello -p '{"value":42}'
wraith deploy --id hello --file ./empty_plugin.wasm
wraith delete hello
wraith logs
```

Short flags are `deploy -i <ID> -f <FILE>` and `exec -p <JSON>`. Avoid literal secrets in command lines; environment expansion can still appear in shell history or process arguments depending on your shell/tooling.

## Command behavior

- `list`: prints ID, tenant, version, and count.
- `deploy`: reads a local file and performs POST upload. It does not expose quota, capability, or signature headers; use HTTP for those features.
- `exec`: parses the payload as JSON before sending; prints pretty-printed response JSON.
- `delete`: DELETE and a confirmation line.
- `logs`: latest 50 global audit rows in a table.

There are no SDK/CLI methods for source builds, quota updates, replacement, wizard operations, or WebSocket subscription in this checkout.

## Embed the SDK

Add a path dependency while developing inside this repository:

```toml
[dependencies]
wraith-sdk = { path = "../wraith-sdk" }
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
serde_json = "1"
```

Example:

```rust
use serde_json::json;
use wraith_sdk::WraithClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = WraithClient::new("http://127.0.0.1:9090")?
        .with_token(std::env::var("WRAITH_TOKEN")?);
    let result = client.execute("hello", None, json!({"test": true})).await?;
    println!("{result:#}");
    Ok(())
}
```

For a tokenless local server, omit `with_token`. For a named tenant, pass `Some("alice")` to list/upload/execute/delete. `get_logs` has no tenant argument and reads the global audit endpoint.

## Public surface

```text
WraithClient::new(base_url) -> Result<Self, SdkError>
WraithClient::with_token(token) -> Self
list(Option<tenant>) -> Result<ListResponse, SdkError>
upload(id, Option<tenant>, Vec<u8>) -> Result<serde_json::Value, SdkError>
execute(id, Option<tenant>, serde_json::Value) -> Result<serde_json::Value, SdkError>
delete(id, Option<tenant>) -> Result<(), SdkError>
get_logs() -> Result<Vec<ExecutionLog>, SdkError>
```

The client installs the ring crypto provider once, uses a 30-second reqwest timeout, and attaches Bearer auth when configured. `new` only checks that the string starts with `http`; it is a permissive prefix check, not comprehensive URL validation. Use an exact trusted base URL.

`SdkError` distinguishes transport errors, non-success API responses (status plus body), and invalid URL prefix. Non-2xx bodies are preserved as text.

## Important current limitations

- `list` and `get_logs` deserialize successful responses with `unwrap_or`/`unwrap_or_default`. A schema mismatch can look like an empty result instead of an error. Validate critical integrations against the HTTP API directly.
- Tenant and module IDs are interpolated into paths without percent-encoding. Use validated simple IDs/names.
- Upload cannot attach signatures/capabilities/quotas.
- Authentication follows the server's actual global-plus-tenant semantics; the SDK cannot fix the single-header tenant-token limitation.
- There is no built-in retry policy. Retry idempotent reads cautiously; do not blindly retry execution of a guest with side effects.
- The SDK does not expose a streaming event client.

For exact status codes and header behavior, use the [HTTP API reference](api.md).
