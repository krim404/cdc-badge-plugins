//! \file
//! \brief Minimal Rust plugin template. Replace this header and the
//!        lifecycle hooks below to start a new plugin.

#![no_std]

extern crate alloc;

use cdc_badge_plugin::{plugin_main, ui, log};

plugin_main!();

const TAG: &str = "my_plugin";

/// \brief Lifecycle hook fired once when the plugin is loaded.
/// \return `0` on success.
#[no_mangle]
pub extern "C" fn plugin_init() -> i32 {
    log::info(TAG, "init");
    0
}

/// \brief Lifecycle hook fired once when the plugin is unloaded.
/// \return `0` on success.
#[no_mangle]
pub extern "C" fn plugin_deinit() -> i32 {
    log::info(TAG, "deinit");
    0
}

/// \brief Lifecycle hook fired every time the user opens the plugin.
/// \return `0` on success.
#[no_mangle]
pub extern "C" fn plugin_on_enter() -> i32 {
    ui::push_toast("Hello!", ui::UI_ICON_SUCCESS, 1500);
    0
}

/// \brief Lifecycle hook fired when the user leaves the plugin view.
/// \return `0` on success.
#[no_mangle]
pub extern "C" fn plugin_on_exit() -> i32 { 0 }
