//! \file
//! \brief I2C master access on the expansion bus.
//!
//! `bus = 0` is the internal bus (charger + IO expander) and is not
//! accessible to plugins. `bus = 1` is the expansion bus shared with the
//! SAO EEPROM.

use crate::{check, Result};
use alloc::vec::Vec;
use core::ffi::c_int;

/// \brief Write a buffer to an I2C device.
/// \param bus  Bus index; must be exposed to the plugin via `capabilities.i2c_bus`.
/// \param addr 7-bit device address.
/// \param data Payload to send.
/// \return `Ok(())` on success, `Err` on failure or NACK.
pub fn write(bus: u8, addr: u8, data: &[u8]) -> Result<()> {
    check(unsafe { host_i2c_write(bus, addr, data.as_ptr(), data.len()) })
}

/// \brief Read `len` bytes from an I2C device.
/// \param bus  Bus index.
/// \param addr 7-bit device address.
/// \param len  Number of bytes to read.
/// \return The bytes read, or `Err` on failure.
pub fn read(bus: u8, addr: u8, len: usize) -> Result<Vec<u8>> {
    let mut buf = Vec::<u8>::with_capacity(len);
    let rc = unsafe {
        buf.set_len(len);
        host_i2c_read(bus, addr, buf.as_mut_ptr(), len)
    };
    check(rc)?;
    Ok(buf)
}

/// \brief Write then read in a single I2C transaction.
/// \param bus      Bus index.
/// \param addr     7-bit device address.
/// \param write    Bytes to send before the restart.
/// \param read_len Number of bytes to read after the restart.
/// \return The bytes read, or `Err` on failure.
pub fn write_read(bus: u8, addr: u8, write: &[u8], read_len: usize) -> Result<Vec<u8>> {
    let mut buf = Vec::<u8>::with_capacity(read_len);
    let rc = unsafe {
        buf.set_len(read_len);
        host_i2c_write_read(
            bus,
            addr,
            write.as_ptr(),
            write.len(),
            buf.as_mut_ptr(),
            read_len,
        )
    };
    check(rc)?;
    Ok(buf)
}

/// \brief Scan the bus for responding 7-bit addresses.
/// \param bus Bus index.
/// \return Vector of detected addresses, or `Err` on failure.
pub fn scan(bus: u8) -> Result<Vec<u8>> {
    let mut buf = [0u8; 128];
    let mut count: usize = buf.len();
    let rc = unsafe { host_i2c_scan(bus, buf.as_mut_ptr(), &mut count) };
    check(rc)?;
    Ok(buf[..count].to_vec())
}

#[link(wasm_import_module = "cdc")]
extern "C" {
    fn host_i2c_write(bus: u8, addr: u8, data: *const u8, len: usize) -> c_int;
    fn host_i2c_read(bus: u8, addr: u8, data: *mut u8, len: usize) -> c_int;
    fn host_i2c_write_read(
        bus: u8,
        addr: u8,
        wr: *const u8,
        wr_len: usize,
        rd: *mut u8,
        rd_len: usize,
    ) -> c_int;
    fn host_i2c_scan(bus: u8, found_addrs: *mut u8, count: *mut usize) -> c_int;
}
