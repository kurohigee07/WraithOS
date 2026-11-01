# Orchestrator

Unified core engine: Wasmtime component-model plugins, Rhai rule scripts,
static musl binary (x86_64 + aarch64).

## Build
    cargo zigbuild --release --target x86_64-unknown-linux-musl
    cargo zigbuild --release --target aarch64-unknown-linux-musl

## Run
    ./target/x86_64-unknown-linux-musl/release/orchestrator

Reads `./modules/*.wasm` on startup. Graceful shutdown on SIGINT/SIGTERM.

## Status
v0.1.0-mvp — see git tag history for milestone progression (M1..M7).
