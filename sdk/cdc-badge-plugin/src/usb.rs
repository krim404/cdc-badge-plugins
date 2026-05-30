//! \file
//! \brief Raw writes to the USB-CDC serial endpoint.

use core::ffi::c_int;

/// \brief Generic USB-CDC error returned by the helpers in this module.
#[derive(Debug, Clone, Copy)]
pub struct UsbError;

/// \brief Write raw bytes to the USB-CDC TX stream.
/// \param data Bytes to transmit.
/// \return `Ok(())` on success, `Err(UsbError)` on failure.
pub fn cdc_write(data: &[u8]) -> Result<(), UsbError> {
    let rc = unsafe { host_usb_cdc_write(data.as_ptr(), data.len()) };
    if rc == 0 {
        Ok(())
    } else {
        Err(UsbError)
    }
}

#[link(wasm_import_module = "cdc")]
extern "C" {
    fn host_usb_cdc_write(data: *const u8, len: usize) -> c_int;
}
