# Plugin template (Rust)

Copy this directory to start a new plugin.

1. Rename `my_plugin` everywhere (`Cargo.toml`, `meta.json`, source).
2. Edit `meta.json` - description, icon, capabilities, prerequisites.
3. Implement the lifecycle exports in `src/lib.rs`.
4. Build:
   ```bash
   cargo build --release --target wasm32-unknown-unknown
   wasm-opt -Oz target/wasm32-unknown-unknown/release/my_plugin.wasm -o my_plugin.wasm
   ```
5. Install via the upload tool or the web flasher.
