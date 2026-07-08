//! \file
//! \brief SAO add-on EEPROM access on the expansion port.
//!
//! Reads and writes the I2C EEPROM of an attached "Shitty Add-On". Access
//! is gated by the manifest capability for the SAO port.

use crate::{check, Result};
use alloc::vec::Vec;
use core::ffi::c_int;

/// \brief Read `len` bytes from the SAO EEPROM starting at `offset`.
/// \param offset Byte offset into the EEPROM.
/// \param len    Number of bytes to read.
/// \return The bytes read, or `Err` on failure.
pub fn eeprom_read(offset: u16, len: usize) -> Result<Vec<u8>> {
    let mut buf = Vec::<u8>::with_capacity(len);
    let rc = unsafe {
        buf.set_len(len);
        host_sao_eeprom_read(offset, buf.as_mut_ptr(), len)
    };
    check(rc)?;
    Ok(buf)
}

/// \brief Write bytes to the SAO EEPROM starting at `offset`.
///
/// Mutates the attached add-on's persistent EEPROM contents.
/// \param offset Byte offset into the EEPROM.
/// \param data   Bytes to write.
/// \return `Ok(())` on success, `Err` on failure.
pub fn eeprom_write(offset: u16, data: &[u8]) -> Result<()> {
    check(unsafe { host_sao_eeprom_write(offset, crate::slice_ptr(data), data.len()) })
}

#[link(wasm_import_module = "cdc")]
extern "C" {
    fn host_sao_eeprom_read(offset: u16, buf: *mut u8, len: usize) -> c_int;
    fn host_sao_eeprom_write(offset: u16, buf: *const u8, len: usize) -> c_int;
}
