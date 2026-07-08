//! \file
//! \brief Raw writes to the USB-CDC serial endpoint.

use crate::{check, Result};
use core::ffi::c_int;

/// \brief Write raw bytes to the USB-CDC TX stream.
/// \param data Bytes to transmit.
/// \return `Ok(())` on success, `Err` on failure.
pub fn cdc_write(data: &[u8]) -> Result<()> {
    check(unsafe { host_usb_cdc_write(crate::slice_ptr(data), data.len()) })
}

#[link(wasm_import_module = "cdc")]
extern "C" {
    fn host_usb_cdc_write(data: *const u8, len: usize) -> c_int;
}
