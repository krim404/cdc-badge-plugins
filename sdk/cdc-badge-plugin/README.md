# cdc-badge-plugin

Safe Rust bindings for the [CDC Badge OS](https://github.com/krim404/cdc-badge-os) plugin host API.

## Usage

```toml
[dependencies]
cdc-badge-plugin = { git = "https://github.com/krim404/cdc-badge-plugins", features = ["panic_handler"] }
```

```rust
#![no_std]
#![no_main]

extern crate alloc;
use cdc_badge_plugin::{plugin_main, ui, log};

plugin_main!();

#[no_mangle]
pub extern "C" fn plugin_init() -> i32 { 0 }

#[no_mangle]
pub extern "C" fn plugin_on_enter() -> i32 {
    log::info("hello", "plugin entered");
    ui::push_toast("Hello!", ui::UI_ICON_SUCCESS, 2000);
    0
}

#[no_mangle]
pub extern "C" fn plugin_on_exit()  -> i32 { 0 }
#[no_mangle]
pub extern "C" fn plugin_deinit()   -> i32 { 0 }
```

Build:
```bash
cargo build --release --target wasm32-unknown-unknown
```

The output `.wasm` goes into a plugin bundle next to its `meta.json`.
