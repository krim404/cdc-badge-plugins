//! \file
//! \brief Safe Rust bindings for the CDC Badge OS WASM plugin host API.
//!
//! Mirrors `sdk/host_api.h`. Host functions live in the `cdc` WASM import
//! module. This crate provides:
//!   - Raw `extern "C"` declarations under `ffi`
//!   - Safe high-level wrappers in the top-level modules
//!   - The `plugin_main!` macro to wire up the lifecycle exports

#![no_std]

extern crate alloc;

/// \brief Major part of the host API level this SDK targets.
pub const HOST_API_LEVEL_MAJOR: u16 = 0;
/// \brief Minor part of the host API level this SDK targets.
pub const HOST_API_LEVEL_MINOR: u16 = 6;

pub mod ble;
pub mod canvas;
pub mod cmd;
pub mod crypto;
pub mod display;
pub mod event;
pub mod ffi;
pub mod gpio;
pub mod http;
pub mod i18n;
pub mod i2c;
pub mod keypad;
pub mod lockscreen;
pub mod log;
pub mod nvs;
pub mod pixel_strip;
pub mod power;
pub mod random;
pub mod rmem;
pub mod sao;
pub mod secure_element;
pub mod sysinfo;
pub mod time;
pub mod ui;
pub mod usb;
pub mod wifi;

#[cfg(all(feature = "allocator", target_arch = "wasm32"))]
#[global_allocator]
static GLOBAL: dlmalloc::GlobalDlmalloc = dlmalloc::GlobalDlmalloc;

/// \brief Emit the `plugin_required_api_major` / `_minor` exports the
///        host reads at load time.
///
/// Every plugin's `src/lib.rs` must invoke this exactly once at the top
/// of the file.
#[macro_export]
macro_rules! plugin_main {
    () => {
        #[no_mangle]
        pub extern "C" fn plugin_required_api_major() -> u16 {
            $crate::HOST_API_LEVEL_MAJOR
        }
        #[no_mangle]
        pub extern "C" fn plugin_required_api_minor() -> u16 {
            $crate::HOST_API_LEVEL_MINOR
        }
    };
}

#[cfg(all(feature = "panic_handler", target_arch = "wasm32"))]
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    use alloc::format;
    let msg = format!("{}", info);
    crate::log::error("PANIC", &msg);
    loop {}
}
