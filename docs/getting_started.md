# Getting Started

This guide walks through writing, building, and installing your first plugin for CDC Badge OS.

## 1. Prerequisites

- Rust (stable) with the `wasm32-unknown-unknown` target:
  ```bash
  rustup target add wasm32-unknown-unknown
  ```
- Optional but recommended: [wasm-opt](https://github.com/WebAssembly/binaryen) to shrink the build.
- A CDC Badge running firmware with host API level `0.6` or higher.

## 2. Clone & build

```bash
git clone https://github.com/krim404/cdc-badge-plugins
cd cdc-badge-plugins
cargo build --release --target wasm32-unknown-unknown -p hello_world
ls target/wasm32-unknown-unknown/release/hello_world.wasm
```

Optionally shrink the result with `wasm-opt`:

```bash
wasm-opt -Oz target/wasm32-unknown-unknown/release/hello_world.wasm -o hello_world.wasm
```

## 3. Start your own plugin

Copy `sdk/plugin_template_rust/` to a new directory, rename `my_plugin` in the four places that mention it, and edit `meta.json`.

The minimum lifecycle exports your plugin must provide are listed in `sdk/plugin_lifecycle.h`.

## 4. Install on the badge

Two options:

**A) Web installer** (no toolchain on the host):
1. Open <https://krim404.github.io/cdc-badge-plugins/> in Chrome or Edge.
2. Connect the badge via USB and click "Connect via Serial".
3. Pick your plugin from the catalog and click "Install".

**B) Python tool** (from the cdc-badge-os repo):
```bash
python tools/upload.py --wasm hello_world.wasm --meta path/to/meta.json
```

## 5. Run it

On the badge, open the main menu, scroll to **Plugins**, and start your plugin.

## Where to go next

- [Host API reference](host_api_reference.md)
- [Manifest schema](manifest_schema.md)
- [Plugin lifecycle](plugin_lifecycle.md)
- [Capabilities](capabilities.md)
- [Rust SDK guide](rust_sdk_guide.md)
