# home_assistant (reference port)

Full WASM port of the native `mod_homeassistant`, targeting host API v0.5.

## What it does

- Calls `GET /api/states` and lists every `light.*` / `switch.*` entity in alphabetical order with its current state.
- Pressing **Y** on an entity opens a confirm dialog. **Y** again calls `POST /api/services/<domain>/toggle` and shows a success / error toast.

## Configuration

Before starting the plugin, configure URL + long-lived access token on the
badge:

- The URL is stored in NVS under namespace `plugin_ha`, key `url`.
- The token is stored in the named rmem slot `ha_token` (allocated from the
  plugin pool the first time it is written). The slot persists across reboot
  and plugin reinstall as long as `"ha_token"` stays declared in
  `capabilities.rmem`.

Until the on-badge setup view ships, seed both via the host serial console.

## Build

```bash
cargo build --release --target wasm32-unknown-unknown -p home_assistant
wasm-opt -Oz target/wasm32-unknown-unknown/release/home_assistant.wasm -o ha.wasm
```

## Install

```bash
python tools/upload_plugin.py --wasm ha.wasm --meta examples/home_assistant/meta.json
```

Open **Tools → Plugins → Home Assistant** on the badge.

## Capabilities used

```json
"capabilities": {
  "wifi":           true,
  "http":           true,
  "rmem":           ["ha_token"],
  "nvs_namespace":  "plugin_ha",
  "ui_exclusive":   true
}
```

The plugin manifest also declares `prerequisites.wifi_connected` with a 15 s timeout - the host establishes WiFi before `plugin_on_enter` runs and tears it down again on exit.
