//! \file
//! \brief Hello-world smoke-test plugin: shows a toast on enter.
//!
//! This is the smallest possible CDC badge plugin. Read it first if you
//! have never written one before. It demonstrates four ideas:
//!   1. how a plugin declares its lifecycle entry points,
//!   2. how text is logged from inside the plugin sandbox,
//!   3. how to show a short popup ("toast") on the badge screen,
//!   4. how translatable text is pulled from the manifest via i18n keys.

// `no_std` tells the Rust compiler "do not link the standard library".
// Plugins compile to WebAssembly and run inside the badge firmware, where
// the normal OS-provided `std` (files, threads, sockets) is not available.
// Everything you need lives in `core` (always present) and `alloc`
// (heap-backed types like `String` / `Vec`, pulled in below).
#![no_std]

// `alloc` gives us access to heap data structures even without `std`.
// You only need this when you want `String`, `Vec`, `Box`, `format!`, etc.
// `hello_world` does not allocate itself, but the SDK macros do, so we
// declare it here as a habit.
extern crate alloc;

// Bring SDK modules into scope. Each module wraps a slice of the host
// API: `log` writes to the badge log, `ui` draws UI elements, and
// `plugin_main` is the macro that wires up the WASM entry points the
// firmware expects to find.
use cdc_badge_plugin::{log, plugin_main, ui};

// `plugin_main!()` expands into the small amount of boilerplate every
// plugin needs (panic handler, global allocator, FFI shims). Forgetting
// this macro produces confusing link errors when the badge tries to load
// the .wasm file. Always call it exactly once at the top level.
plugin_main!();

// A short, fixed identifier prefixed to every log line from this plugin.
// Picking a stable tag per plugin makes it easy to filter the serial
// console output ("hello: init", "hello: enter", ...).
const TAG: &str = "hello";

/// \brief Lifecycle hook fired once when the plugin is loaded.
/// \return `0` on success.
//
// `#[no_mangle]` tells the Rust compiler "do not rename this function".
// The firmware looks up `plugin_init` by its exact textual name in the
// compiled .wasm module; without `#[no_mangle]`, Rust would mangle it
// into something like `_ZN5hello11plugin_init17hABCDE` and loading would
// fail silently.
//
// `pub extern "C"` makes the function callable from C / from the host
// runtime using the platform C ABI (calling convention). All lifecycle
// hooks must be `pub extern "C" fn name() -> i32`.
//
// The return value is the integer status. `0` means success. Anything
// non-zero tells the firmware that this lifecycle step failed; for
// `plugin_init` that aborts loading.
#[no_mangle]
pub extern "C" fn plugin_init() -> i32 {
    // Write a single line to the badge serial log. Logging is cheap and
    // the first tool you reach for when something goes wrong.
    log::info(TAG, "init");
    0
}

/// \brief Lifecycle hook fired once when the plugin is unloaded.
/// \return `0` on success.
//
// Pair every resource you grab in `plugin_init` with a release in
// `plugin_deinit`. This trivial plugin owns nothing, so the function is
// little more than a goodbye log line.
#[no_mangle]
pub extern "C" fn plugin_deinit() -> i32 {
    log::info(TAG, "deinit");
    0
}

/// \brief Lifecycle hook fired every time the user opens the plugin from
///        the badge menu.
/// \return `0` on success.
//
// `on_enter` fires *every* time the user navigates into the plugin, not
// just the first time. Heavy work that should happen once at load time
// belongs in `plugin_init`; work tied to the user opening the view (UI
// build, sensor reads, network fetches) belongs here.
#[no_mangle]
pub extern "C" fn plugin_on_enter() -> i32 {
    log::info(TAG, "enter");

    // `push_toast` displays a small, auto-dismissing popup. Three args:
    //   - the message text (here resolved from the i18n table by key),
    //   - an icon constant from `ui::UI_ICON_*`,
    //   - the display duration in milliseconds.
    //
    // `i18n::tr_key("greeting")` looks up the string `greeting` first in
    // the active language's `.lang.json`, then falls back to the English
    // value stored in `meta.json`. If the key is missing everywhere, the
    // key itself is returned, which makes mistakes easy to spot on screen.
    ui::push_toast(
        cdc_badge_plugin::i18n::tr_key("greeting"),
        ui::UI_ICON_SUCCESS,
        2500,
    );
    0
}

/// \brief Lifecycle hook fired when the user leaves the plugin view.
/// \return `0` on success.
//
// `on_exit` is the mirror of `on_enter`. Tear down anything that only
// makes sense while the user is looking at the plugin (timers, GPIO
// pulses, network requests in flight). This plugin has nothing to tear
// down, so it just logs.
#[no_mangle]
pub extern "C" fn plugin_on_exit() -> i32 {
    log::info(TAG, "exit");
    0
}
