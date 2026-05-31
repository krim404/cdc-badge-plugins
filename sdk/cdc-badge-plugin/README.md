# cdc-badge-plugin

Safe Rust bindings for the [CDC Badge OS](https://github.com/krim404/cdc-badge-os) plugin host API.

Plugins are `cdylib` libraries with `#[no_mangle]` lifecycle exports, compiled to `wasm32-unknown-unknown`.

```toml
[dependencies]
cdc-badge-plugin = { git = "https://github.com/krim404/cdc-badge-plugins", features = ["panic_handler"] }
```

See the [Rust SDK guide](../../docs/rust_sdk_guide.md) for the lifecycle skeleton, the crate modules, and error handling. See [Getting Started](../../docs/getting_started.md) for build and install steps.
