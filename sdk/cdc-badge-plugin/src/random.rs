//! \file
//! \brief Hardware random number generator access.

use crate::{check, ffi, Result};

/// \brief Fill `buf` with random bytes; may fall back to a software PRNG when
///        no hardware TRNG is available.
/// \param buf Destination buffer to fill.
/// \return `Ok(())` on success, `Err` on failure.
pub fn fill(buf: &mut [u8]) -> Result<()> {
    check(unsafe { ffi::host_random(buf.as_mut_ptr(), buf.len()) })
}

/// \brief Fill `buf` with hardware TRNG bytes only.
/// \param buf Destination buffer to fill.
/// \return `Ok(())` on success, `Err` when no hardware TRNG is present.
pub fn fill_strict(buf: &mut [u8]) -> Result<()> {
    check(unsafe { ffi::host_random_strict(buf.as_mut_ptr(), buf.len()) })
}

/// \brief Draw a single random `u32`.
/// \return A random value, or `Err` if the RNG call failed.
pub fn u32() -> Result<u32> {
    let mut bytes = [0u8; 4];
    fill(&mut bytes)?;
    Ok(u32::from_le_bytes(bytes))
}
