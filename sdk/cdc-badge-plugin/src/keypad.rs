//! \file
//! \brief Direct polling of the 12-button keypad.
//!
//! Most plugins receive key input through view callbacks or the event bus;
//! these helpers are for plugins that poll the pad directly (games, custom
//! canvases).

use core::ffi::c_int;

pub const KEY_0: u8 = 0;
pub const KEY_1: u8 = 1;
pub const KEY_2: u8 = 2;
pub const KEY_3: u8 = 3;
pub const KEY_4: u8 = 4;
pub const KEY_5: u8 = 5;
pub const KEY_6: u8 = 6;
pub const KEY_7: u8 = 7;
pub const KEY_8: u8 = 8;
pub const KEY_9: u8 = 9;
pub const KEY_Y: u8 = 10;
pub const KEY_N: u8 = 11;

/// \brief Whether `key` (one of the `KEY_*` constants) is currently held.
pub fn is_pressed(key: u8) -> bool {
    unsafe { host_key_pressed(key) }
}

/// \brief Pop the next queued key press, if any.
/// \return The `KEY_*` code, or `None` when the queue is empty.
pub fn consume_next() -> Option<u8> {
    let mut out: u8 = 0;
    let rc = unsafe { host_key_consume_next(&mut out) };
    if rc == 0 {
        Some(out)
    } else {
        None
    }
}

#[link(wasm_import_module = "cdc")]
extern "C" {
    fn host_key_pressed(key: u8) -> bool;
    fn host_key_consume_next(out_key: *mut u8) -> c_int;
}
