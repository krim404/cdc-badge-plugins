//! \file
//! \brief Grove blink: toggle GPIO 2 (Grove SIG0) from a list view.
//!
//! Demonstrates a GPIO output capability and a simple plugin_on_action
//! loop. Y picks the toggle item, N exits the plugin.
//!
//! New concepts compared to `battery_widget`:
//!   - declaring and using a capability (`gpio_out`) in `meta.json`,
//!   - holding mutable plugin state across callbacks with a `static`,
//!   - building an interactive list view with `ui::ListBuilder`,
//!   - reacting to a user selection through `plugin_on_action`.

// Plugins always run in `no_std`. See `hello_world` for context.
#![no_std]

// `extern crate alloc` is needed because `ListBuilder` allocates strings
// for item labels behind the scenes.
extern crate alloc;

// SDK modules pulled in here:
//   - `gpio` for digital I/O pins,
//   - `log` for the serial log,
//   - `plugin_main` macro for the WASM entry-point boilerplate,
//   - `ui` for the list view and other UI primitives.
use cdc_badge_plugin::{gpio, log, plugin_main, ui};

// Required FFI plumbing.
plugin_main!();

// Log tag for serial-console filtering.
const TAG: &str = "grove_blink";

// The physical pin we drive. `gpio::pins::GROVE_0` is a friendly alias
// for the SIG0 line of the Grove connector. Using the constant instead
// of a magic number keeps the code portable if the firmware ever remaps
// the connector.
const PIN: u8 = gpio::pins::GROVE_0;

// Action IDs are arbitrary `u32` values we choose. They are tagged onto
// UI elements when we build them and arrive back in `plugin_on_action`
// so we can tell *which* element the user activated. Numbering them as
// named constants is clearer than passing raw `1`, `2`, ... through the
// code.
const ACTION_TOGGLE: u32 = 1;

// We need to remember "is the LED currently on?" between calls. The
// lifecycle hooks are separate `extern "C"` functions, so locals do not
// persist; we need a `static` for that.
//
// `static mut` is the bluntest way to do this and only safe because
// WAMR runs every plugin on a single thread - the lifecycle hooks are
// never invoked concurrently with each other. Read/write therefore
// happens inside an `unsafe { ... }` block to acknowledge the contract.
// For anything more complex than a single bool, prefer the `RefCell`
// pattern used in the `news_feed` example.
static mut STATE: bool = false;

// Tiny helper that hides the `unsafe` block behind a safe-looking
// function so the rest of the file can stay readable.
fn state() -> bool {
    unsafe { STATE }
}

/// \brief Lifecycle hook fired once when the plugin is loaded.
///
/// Configures the Grove pin as an output and drives it low.
/// \return `0` on success, `-1` if the GPIO capability is missing.
//
// Returning non-zero from `plugin_init` aborts the load: the firmware
// will refuse to keep the plugin running, which is what we want if the
// hardware capability we depend on is missing.
#[no_mangle]
pub extern "C" fn plugin_init() -> i32 {
    // Ask the host to switch the pin to output mode. This fails if the
    // plugin manifest does not declare the `gpio_out` capability for
    // this pin, or if another plugin already claimed it.
    if gpio::set_direction(PIN, gpio::Direction::Output).is_err() {
        log::error(TAG, "GPIO setup failed - missing capability?");
        return -1;
    }

    // Drive the pin low so the LED starts in a known "off" state. We
    // discard the `Result` with `let _ =` because a write right after
    // a successful `set_direction` is reliable; logging every possible
    // failure would just add noise. Watch out: in production code you
    // usually *do* want to handle write errors.
    let _ = gpio::write(PIN, false);
    0
}

/// \brief Lifecycle hook fired once when the plugin is unloaded.
///
/// Releases the GPIO so other capabilities can claim it again.
/// \return `0` on success.
//
// Always pair `set_direction` with `release` on shutdown. Without this,
// the firmware would keep the pin reserved for this plugin even after
// it is gone, and the next plugin asking for the same pin would fail.
#[no_mangle]
pub extern "C" fn plugin_deinit() -> i32 {
    gpio::release(PIN);
    0
}

// Build (or rebuild) the single-item list view that displays the
// current state. We call this both on enter and after every toggle so
// the screen text reflects reality.
fn render_menu() {
    // Pick the label based on the current state. The strings live in
    // `meta.json` under `i18n.strings.led_on` / `led_off`, so changing
    // language or wording does not require a recompile.
    let label = if state() {
        cdc_badge_plugin::i18n::tr_key("led_on")
    } else {
        cdc_badge_plugin::i18n::tr_key("led_off")
    };

    // `ListBuilder` is a fluent builder: chain methods to configure it,
    // then call a terminal method (`push`, `replace`, ...) at the end
    // to actually send it to the host.
    ui::ListBuilder::new(cdc_badge_plugin::i18n::tr_meta("name"))
        // Tag every "select" on this list with our toggle action ID so
        // `plugin_on_action` knows which list emitted it.
        .on_select(ACTION_TOGGLE)
        // Add one row. `0` here is the per-item index that will be
        // echoed back in the action, and `UI_ICON_INFO` is the icon
        // drawn next to the label.
        .item(label, 0, ui::UI_ICON_INFO)
        // `replace()` swaps the topmost UI view in place. We use it
        // instead of `push()` so repeated toggles do not stack up
        // separate views the user would have to back out of.
        .replace();
}

/// \brief Lifecycle hook fired every time the user opens the plugin.
//
// First entry: just render the menu so the user sees the current state.
#[no_mangle]
pub extern "C" fn plugin_on_enter() -> i32 {
    render_menu();
    0
}

/// \brief Lifecycle hook fired when the user leaves the plugin view.
///
/// Drives the pin back to low and resets the toggle state.
//
// Returning the hardware to a known state on exit is good hygiene: the
// next time the user enters, the on-screen label matches the LED, and
// nothing is left glowing if the user forgot to toggle it off.
#[no_mangle]
pub extern "C" fn plugin_on_exit() -> i32 {
    // Same `static mut` caveat as above: only safe because WAMR is
    // single-threaded per plugin.
    unsafe {
        STATE = false;
    }
    let _ = gpio::write(PIN, false);
    0
}

/// \brief Action dispatch for the toggle list item.
//
// The firmware calls this whenever the user activates a UI element we
// tagged with an action ID. Parameters:
//   - `action_id`: the constant we passed to `.on_select(...)` etc.,
//   - `idx`:       the per-item index from `.item(_, idx, _)` - unused
//                  here because we only have one row,
//   - `user_data`: an extra `u32` we did not set, so it stays unused.
//
// `_idx` / `_user_data` start with an underscore to silence the unused
// argument warning while keeping the parameter names for readability.
#[no_mangle]
pub extern "C" fn plugin_on_action(action_id: u32, _idx: u32, _user_data: u32) -> i32 {
    // Single action means a single arm; a `match` would be overkill
    // here. As soon as you add a second action ID, switch to `match`.
    if action_id == ACTION_TOGGLE {
        // Flip the stored state and capture the new value so we can
        // both write it to GPIO and trust it inside this function
        // without taking a second unsafe read.
        let next = unsafe {
            STATE = !STATE;
            STATE
        };
        // Drive the pin to the new level. We ignore the error for the
        // same reason as in `plugin_init`.
        let _ = gpio::write(PIN, next);
        // Refresh the list label so the screen mirrors the new state.
        render_menu();
    }
    0
}
