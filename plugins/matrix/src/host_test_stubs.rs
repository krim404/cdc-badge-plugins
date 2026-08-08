//! \file
//! \brief Native implementations of the `host_*` crypto imports, for tests only.
//!
//! On wasm the firmware provides these symbols; a native `cargo test` binary has
//! no import object, so any test touching `cdc_badge_plugin::crypto` fails to
//! link. These stubs satisfy the same contracts: return 0 (or a byte count) on
//! success, a negative code on failure, and NUL-terminate encoder output.

use base64::Engine as _;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::os::raw::{c_char, c_int};

/// Error code the SDK maps onto a generic failure.
const ERR_GENERIC: c_int = -1;

/// \brief Copy `src` into the caller's buffer and NUL-terminate it.
/// \return 0 on success, ERR_GENERIC when it does not fit. Encoders report
///         HOST_OK rather than a length; only decoders return a byte count.
unsafe fn write_cstr(src: &[u8], out: *mut c_char, out_size: usize) -> c_int {
    if src.len() + 1 > out_size {
        return ERR_GENERIC;
    }
    core::ptr::copy_nonoverlapping(src.as_ptr(), out as *mut u8, src.len());
    *out.add(src.len()) = 0;
    0
}

#[no_mangle]
extern "C" fn host_base64_encode(
    input: *const u8,
    in_len: usize,
    out: *mut c_char,
    out_size: usize,
) -> c_int {
    let data = unsafe { core::slice::from_raw_parts(input, in_len) };
    let encoded = base64::engine::general_purpose::STANDARD.encode(data);
    unsafe { write_cstr(encoded.as_bytes(), out, out_size) }
}

#[no_mangle]
extern "C" fn host_base64_decode(
    input: *const c_char,
    in_len: usize,
    out: *mut u8,
    out_size: usize,
) -> c_int {
    let text = unsafe { core::slice::from_raw_parts(input as *const u8, in_len) };
    let decoded = match base64::engine::general_purpose::STANDARD.decode(text) {
        Ok(d) => d,
        Err(_) => return ERR_GENERIC,
    };
    if decoded.len() > out_size {
        return ERR_GENERIC;
    }
    unsafe { core::ptr::copy_nonoverlapping(decoded.as_ptr(), out, decoded.len()) };
    decoded.len() as c_int
}

#[no_mangle]
extern "C" fn host_hmac_sha256(
    key: *const u8,
    klen: usize,
    data: *const u8,
    dlen: usize,
    out: *mut u8,
) -> c_int {
    let key = unsafe { core::slice::from_raw_parts(key, klen) };
    let data = unsafe { core::slice::from_raw_parts(data, dlen) };
    let mut mac = match Hmac::<Sha256>::new_from_slice(key) {
        Ok(m) => m,
        Err(_) => return ERR_GENERIC,
    };
    mac.update(data);
    let tag = mac.finalize().into_bytes();
    unsafe { core::ptr::copy_nonoverlapping(tag.as_ptr(), out, tag.len()) };
    0
}
