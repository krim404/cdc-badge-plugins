//! \file
//! \brief Routes `getrandom` (used by vodozemac) to the host hardware RNG.
//!
//! `wasm32-unknown-unknown` has no default entropy source, so getrandom's
//! `custom` backend (selected in `.cargo/config.toml`) is implemented here and
//! forwarded to `random::fill`. Only compiled for wasm; native test builds use
//! getrandom's OS backend.

use getrandom::Error;

const HOST_RNG_FAIL: u16 = 1;

/// # Safety
/// getrandom passes a pointer to `len` writable bytes, possibly uninitialized.
#[unsafe(no_mangle)]
unsafe extern "Rust" fn __getrandom_v03_custom(dest: *mut u8, len: usize) -> Result<(), Error> {
    let buf = unsafe {
        core::ptr::write_bytes(dest, 0, len);
        core::slice::from_raw_parts_mut(dest, len)
    };
    cdc_badge_plugin::random::fill(buf).map_err(|_| Error::new_custom(HOST_RNG_FAIL))
}
