//! \file
//! \brief Battery widget: shows percentage, voltage, USB and charge state
//!        in a single info modal.
//!
//! Second-easiest example. Adds two ideas on top of `hello_world`:
//!   - calling host functions that return real sensor data (`power::*`),
//!   - building a multi-line string with `format!` and showing it in an
//!     info modal (instead of an auto-dismissing toast).

// No standard library: this code runs in the WAMR WASM sandbox on the
// badge. See `hello_world` for a longer explanation of `no_std`.
#![no_std]

// We use `String` and `format!` below; both live in `alloc`, so we have
// to opt into it explicitly in a `no_std` crate.
extern crate alloc;

// `format!` is the heap-allocating sibling of `println!`. It produces a
// `String`. Importing it explicitly is required in `no_std` builds.
use alloc::format;

// SDK modules used by this plugin:
//   - `log`   for serial-console logging,
//   - `power` for battery / USB / charger queries,
//   - `ui`    for screen output,
//   - `plugin_main` macro for the FFI boilerplate.
use cdc_badge_plugin::{log, plugin_main, power, ui};

// Generate the panic handler, allocator, and FFI shims. Always exactly
// once per plugin.
plugin_main!();

// Log tag prefixed to every log line from this plugin.
const TAG: &str = "battery";

/// \brief Lifecycle hook fired once when the plugin is loaded.
/// \return `0` on success.
//
// Nothing to set up: the power module talks to a shared hardware monitor
// that is always available, no GPIO claim or allocation needed.
#[no_mangle]
pub extern "C" fn plugin_init() -> i32 {
    0
}

/// \brief Lifecycle hook fired once when the plugin is unloaded.
/// \return `0` on success.
//
// Nothing to release because nothing was acquired in `plugin_init`. The
// stub is still required so the firmware can find the symbol.
#[no_mangle]
pub extern "C" fn plugin_deinit() -> i32 {
    0
}

/// \brief Lifecycle hook fired when the user leaves the plugin view.
/// \return `0` on success.
//
// The info modal is owned by the UI stack and torn down by the firmware
// when the user navigates away, so we have nothing to do here either.
#[no_mangle]
pub extern "C" fn plugin_on_exit() -> i32 {
    0
}

/// \brief Lifecycle hook fired every time the user opens the plugin.
///
/// Reads the battery state and pushes a static info screen.
/// \return `0` on success.
//
// Note: the values are captured *once* when the user enters the plugin.
// To make the screen live-updating, you would need a periodic capability
// such as `tick` or `background` and re-push the info modal each tick.
#[no_mangle]
pub extern "C" fn plugin_on_enter() -> i32 {
    // `battery_pct()` returns the state-of-charge as an integer percent
    // (0..=100). This is a single host call - cheap, but it does cross
    // the WASM/native boundary, so do not call it in a tight loop.
    let pct = power::battery_pct();

    // `battery_mv()` returns the raw cell voltage in millivolts. Useful
    // when the percent estimate looks suspicious, for example right
    // after boot before the gauge has settled.
    let mv = power::battery_mv();

    // `usb_connected()` is true when 5 V is present on the USB port.
    // This is independent of the charge state: USB can be plugged in
    // while the charger sits idle (battery full, fault, ...).
    let usb = power::usb_connected();

    // The charger reports one of several states. We collapse them into
    // a single bool "is current flowing into the battery right now?".
    // `matches!` is a compact way to check membership in a small set of
    // enum variants; it is equivalent to a `match` that returns `true`
    // for the listed arms and `false` for everything else.
    let charging = matches!(
        power::charge_status(),
        power::ChargeStatus::Fast | power::ChargeStatus::PreCharge,
    );

    // Build a multi-line message with `format!`. The `\n` inside the
    // string is a real newline; the info modal will render each line
    // separately. We convert the booleans into human-readable text here
    // because the info modal takes plain strings only.
    let body = format!(
        "{} %\n{} mV\nUSB: {}\nCharging: {}",
        pct,
        mv,
        if usb { "yes" } else { "no" },
        if charging { "yes" } else { "no" },
    );

    // Mirror the message to the serial log so the same data is visible
    // from a connected workstation, not just on the badge screen.
    log::info(TAG, &body);

    // `push_info` opens a static modal with a title bar and a body. The
    // user dismisses it with the back button. Unlike `push_toast`, it
    // stays on screen until the user closes it.
    ui::push_info("Battery", &body);

    0
}
