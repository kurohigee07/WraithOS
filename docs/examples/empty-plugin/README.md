# Network-free guest example

This standalone crate implements the host's `plugin` world and returns `{}` without I/O. The WIT file is a deliberate copy of `orchestrator/wit/host-capabilities.wit` at the documented baseline.

```bash
rustup target add wasm32-wasip2
cargo build --manifest-path docs/examples/empty-plugin/Cargo.toml --target wasm32-wasip2 --release
```

Run the second command from the repository root. Upload the resulting `empty_plugin.wasm` using `X-Module-Id`, then execute it with a JSON body. See the [plugin guide](../../plugins.md).

This is behaviorally equivalent to the benchmark guest, not a byte-identical reproduction. No Cargo lockfile is supplied for this independently resolved example; record/generate a lockfile before using it as a reproducible release artifact.
