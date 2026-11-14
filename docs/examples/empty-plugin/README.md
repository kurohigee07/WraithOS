# Empty guest example

A standalone crate that implements the host's `plugin` world and
returns `{}` without any I/O. The WIT contract in `wit/` is a
deliberate copy of `orchestrator/wit/host-capabilities.wit` at the
documented baseline.

## Build

From the repository root:

    rustup target add wasm32-wasip2
    cargo build \
      --manifest-path docs/examples/empty-plugin/Cargo.toml \
      --target wasm32-wasip2 \
      --release

Output: `docs/examples/empty-plugin/target/wasm32-wasip2/release/empty_plugin.wasm`.

## Upload and run

    curl -X POST http://127.0.0.1:9090/modules \
         -H 'x-module-id: empty' \
         --data-binary @docs/examples/empty-plugin/target/wasm32-wasip2/release/empty_plugin.wasm

    curl -X POST http://127.0.0.1:9090/modules/empty/execute \
         -H 'content-type: application/json' \
         -d '{}'

Returns `{}`.

See [`docs/plugins.md`](../../plugins.md) for the guest contract and
capability model.

## Reproducibility note

No `Cargo.lock` is included — the example resolves dependencies at
build time. Record or generate a lockfile before using the output as
a reproducible release artifact. This example is behaviorally
equivalent to the benchmark guest, not byte-identical.
