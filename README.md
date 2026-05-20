# CDC Badge Plugins

WebAssembly plugins for [CDC Badge OS](https://github.com/krim404/cdc-badge-os) - the hardware security key based on ESP32-S3 and TROPIC01.

Plugins run inside a sandboxed WAMR runtime on the badge and interact with the firmware through a stable C-ABI host API. They are uploaded as `.wasm` files over the serial command interface or through the web flasher - no firmware rebuild required.

## Status

Pre-alpha. Host API Level `0.5`. Breaking changes between minor versions until `1.0`.

## Repository Layout

```
sdk/
  host_api.h               Canonical C ABI header (mirrored from cdc-badge-os)
  plugin_lifecycle.h       Plugin lifecycle export contract
  cdc-badge-plugin/        Rust crate with safe wrappers
  plugin_template_rust/    Starting point for a new Rust plugin
  plugin_template_c/       Starting point for a new C plugin
examples/                  Demo plugins kept short for learning
plugins/                   Shipped plugins (catalog default)
webflasher/                WebSerial-based plugin installer (also hosted on GitHub Pages)
docs/                      Getting started, API reference, manifest schema
tools/
  generate_catalog.py      Builds catalog.json from examples/ + plugins/ meta.json
  validate_manifest.py     JSON schema validator
.github/workflows/         CI for builds, releases, drift checks
```

## Plugins

### `examples/` - learning material

Compact plugins focused on one or two host API areas each. Read these first
when porting your own plugin.

| Plugin | Capabilities | What it shows |
|--------|--------------|---------------|
| `hello_world` | (none) | Lifecycle skeleton, toast output, i18n string lookup. The smallest possible plugin. |
| `battery_widget` | (none) | Polling the read-only Power API and rendering a live widget. |
| `grove_blink` | `grove` | GPIO output on the Grove port and a simple ListView toggle driven by key events. |
| `news_feed` | `wifi`, `http`, `nvs_namespace`, `ui_exclusive` | Wi-Fi prerequisite handling, HTTP fetch, Atom/RSS parsing, NVS-persisted feed URL with T9 editing, reload on key press. |

### `plugins/` - shipped in the default catalog

Full-UX plugins with persistence, settings menus, and i18n. Heavier than the
examples but still framed as reference implementations.

| Plugin | Capabilities | What it does |
|--------|--------------|--------------|
| `grove_led` | `background`, `pixel_strip`, `nvs_namespace` | Controls a WS2813 strip on the Grove port. Settings menu for count, brightness, color, and effects (rainbow, static, blink, breathing). Runs in the background. |
| `home_assistant` | `wifi`, `http`, `rmem` (token), `nvs_namespace` | Toggles a curated list of Home Assistant lights and switches via the REST API. Settings menu, favorites flow, token stored in secure memory. Long-lived access tokens are unpleasant to type on T9, so paste them over serial with `PASTE <token>` while the API-key input field is open on the badge. |

#### A note on `home_assistant`

The `home_assistant` plugin is intentionally **only an example**. It is too
large and feature-rich to belong in `examples/`, so it lives in `plugins/`, but
it is not a full Home Assistant client and never will be. Home Assistant
exposes hundreds of integrations, entity types, and dashboard concepts; porting
all of that onto a monochrome badge with a few hundred KB of WASM memory is
neither practical nor the point. The plugin implements a small, opinionated
slice (a handful of switchable entities, REST authentication, persistent
favorites) to demonstrate how a real-world networked plugin is structured.
Treat it as a recipe to adapt, not a product.

## Quickstart (Rust)

```bash
rustup target add wasm32-unknown-unknown
cargo new --bin my_plugin
# Copy sdk/plugin_template_rust/Cargo.toml as starting point
cargo build --release --target wasm32-unknown-unknown
wasm-opt -Oz target/wasm32-unknown-unknown/release/my_plugin.wasm -o my_plugin.wasm
```

Upload to the badge with the Python tool from the firmware repo, or use the [web installer](https://krim404.github.io/cdc-badge-plugins/).

## Host API Level Compatibility

| Firmware version | Host API Level |
|------------------|----------------|
| `0.6.x`          | `0.5`          |

A plugin declares the minimum host API level it needs in its `meta.json`. The badge refuses to load plugins that need a higher minor than the firmware provides, or any other major.

## Firmware build prerequisites

Plugins built with recent Rust toolchains emit bulk-memory WASM ops (`memory.copy`, `memory.fill`). The firmware's WAMR runtime must be compiled with `WAMR_BUILD_BULK_MEMORY=1` (default in ESP-IDF setups) or plugins will fail to instantiate.

## License

GPL-3.0 - see [LICENSE.md](LICENSE.md).
