//! \file
//! \brief Hardware random number generator access.

use crate::ffi;

/// \brief Fill `buf` with random bytes; may fall back to a software PRNG when
///        no hardware TRNG is available.
/// \param buf Destination buffer to fill.
/// \return `true` on success.
pub fn fill(buf: &mut [u8]) -> bool {
    unsafe { ffi::host_random(buf.as_mut_ptr(), buf.len()) == 0 }
}

/// \brief Fill `buf` with hardware TRNG bytes only.
/// \param buf Destination buffer to fill.
/// \return `true` on success, `false` when no hardware TRNG is present.
pub fn fill_strict(buf: &mut [u8]) -> bool {
    unsafe { ffi::host_random_strict(buf.as_mut_ptr(), buf.len()) == 0 }
}

/// \brief Draw a single random `u32`.
/// \return A random value, or `None` if the RNG call failed.
pub fn u32() -> Option<u32> {
    let mut bytes = [0u8; 4];
    if fill(&mut bytes) {
        Some(u32::from_le_bytes(bytes))
    } else {
        None
    }
}
