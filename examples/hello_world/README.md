# hello_world

Smallest possible CDC Badge plugin. Logs to the serial console on init / enter / exit and pushes a localised toast when the user opens it.

Used as the smoke test for the plugin runtime: if this works, WAMR + the plugin manager + the basic host API are wired up correctly.

## Build

```bash
cargo build --release --target wasm32-unknown-unknown -p hello_world
wasm-opt -Oz target/wasm32-unknown-unknown/release/hello_world.wasm -o hello_world.wasm
```

## Install

```bash
python tools/upload_plugin.py --wasm hello_world.wasm --meta examples/hello_world/meta.json
```
