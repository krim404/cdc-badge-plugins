# Rust SDK Guide

The `cdc-badge-plugin` crate provides safe Rust wrappers over the host API. A
plugin is a `cdylib` with `#[no_mangle]` lifecycle exports, not a binary. For
the build and install flow see [getting_started.md](getting_started.md).

## Cargo dependency

```toml
[dependencies]
cdc-badge-plugin = { git = "https://github.com/krim404/cdc-badge-plugins", features = ["panic_handler"] }
```

The `panic_handler` feature pulls in a `#[panic_handler]` that logs the panic message via `host_log` and then traps the WASM module. Disable it if your plugin provides its own handler.

## Lifecycle skeleton

```rust
#![no_std]
extern crate alloc;

use cdc_badge_plugin::{plugin_main, ui, log};

plugin_main!();

#[no_mangle] pub extern "C" fn plugin_init()   -> i32 { 0 }
#[no_mangle] pub extern "C" fn plugin_deinit() -> i32 { 0 }
#[no_mangle] pub extern "C" fn plugin_on_exit()-> i32 { 0 }

#[no_mangle]
pub extern "C" fn plugin_on_enter() -> i32 {
    ui::push_toast("Hello!", ui::UI_ICON_SUCCESS, 1500);
    0
}
```

`plugin_main!()` expands to the two API-level exports that the host checks at load time.

## Modules in the crate

Highlights below. The crate also has `canvas`, `cmd`, `crypto`, `display`,
`fs`, `gpio`, `http`, `i2c`, `keypad`, `lockscreen`, `pixel_strip`, `random`,
`rmem`, `sao`, `secure_element`, `socket`, `sysinfo`, `usb`, and `wifi`. See the
generated crate docs (`cargo doc -p cdc-badge-plugin --open`) for the full API.

| Module | Use |
|--------|-----|
| `ffi` | Raw `extern "C"` declarations. Stick to the safe wrappers above this unless you really need it. |
| `log` | `log::info(tag, msg)`, etc. |
| `ui` | Toast/message/confirm/list/info builders. `ui::ListBuilder` makes lists ergonomic. |
| `time` | `time::uptime_ms()`, `time::local_time()`, ... |
| `power` | Battery level, USB state, charge status. |
| `nvs` | Per-plugin namespaced key/value. `get_blob/set_blob/get_str/set_str/get_u32/set_u32`. |
| `i18n` | `i18n::tr_key(...)`, `i18n::current_language()` |
| `event` | Subscribe to EventBus events with an action id. |
| `ble` | ⚠️ **WIP, untested on hardware.** GATT peripheral (publish a service with characteristics) and central (scan/connect/discover/read/write/subscribe). Inbound events fire an action id; pull the payload with the matching `consume_*`. Needs `ble: true`. |

## Error handling

Every fallible host call returns `cdc_badge_plugin::Result<T>`, i.e.
`core::result::Result<T, cdc_badge_plugin::Error>`. `Error` is one enum across
all modules, mapping the `HOST_ERR_*` codes (`InvalidArg`, `NoCapability`,
`NotFound`, `Timeout`, `NoMemory`, `Busy`, `NotSupported`, `RmemFull`,
`Generic`, and `Other(code)` for anything else). Use `?` to propagate:

```rust
use cdc_badge_plugin::{nvs, Result};

fn save(count: u32) -> Result<()> {
    nvs::set_u32("count", count)?;
    Ok(())
}
```

Pure lookups that may legitimately have no value still return `Option<T>`
(`nvs::get_*`, `wifi::ssid`, `keypad::consume_next`, ...); infallible reads
(`time::uptime_ms`, `power::battery_pct`, `display::width`, ...) return the
value directly.

## Things to know

- Allocation: `alloc::String` and `alloc::Vec` are available via `extern crate alloc;`. The crate bundles `dlmalloc` as `#[global_allocator]` behind the default-on `allocator` feature - disable the feature only if you wire up your own.
- Strings crossing into host calls go through `CString`. Avoid storing them in static state unless you `Box::leak` them.
- Don't unwrap on host calls in production - the host can return `HOST_ERR_NO_CAPABILITY` even for things you think your manifest covers.
