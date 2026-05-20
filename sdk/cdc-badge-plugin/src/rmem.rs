//! \file
//! \brief TROPIC01 R-Memory access addressed by name.
//!
//! Only names listed in the plugin's manifest `capabilities.rmem` are
//! reachable; the host injects the capability check on every call.
//! Multiple plugins declaring the same name share the same physical slot
//! by design (intentional, common scope).

use alloc::vec::Vec;
use core::ffi::c_int;

/// \brief Maximum number of bytes a slot name may carry, excluding the
///        trailing NUL.
pub const RMEM_NAME_MAX: usize = 15;

/// \brief Generic rmem error returned by the helpers in this module.
#[derive(Debug, Clone, Copy)]
pub struct RmemError;

fn cstr(name: &str) -> Result<[u8; RMEM_NAME_MAX + 1], RmemError> {
    if name.is_empty() || name.len() > RMEM_NAME_MAX {
        return Err(RmemError);
    }
    let mut buf = [0u8; RMEM_NAME_MAX + 1];
    buf[..name.len()].copy_from_slice(name.as_bytes());
    Ok(buf)
}

/// \brief Read the contents of a named rmem slot.
/// \param name    Slot name declared in `capabilities.rmem`, 1-15 chars.
/// \param max_len Maximum number of payload bytes to read.
/// \return The payload bytes, or `Err(RmemError)` if the slot is unknown,
///         empty, or capability-denied.
pub fn read(name: &str, max_len: usize) -> Result<Vec<u8>, RmemError> {
    let n = cstr(name)?;
    let mut buf = Vec::<u8>::with_capacity(max_len);
    let rc = unsafe {
        buf.set_len(max_len);
        host_rmem_read_named(n.as_ptr(), buf.as_mut_ptr(), max_len as i32)
    };
    if rc < 0 {
        return Err(RmemError);
    }
    buf.truncate(rc as usize);
    Ok(buf)
}

/// \brief Write a payload to a named rmem slot.
///
/// On first write the host allocates a physical slot from the plugin
/// pool; subsequent writes overwrite that same slot.
/// \param name Slot name declared in `capabilities.rmem`, 1-15 chars.
/// \param data Payload bytes.
/// \return `Ok(())` on success, `Err(RmemError)` on capability denial or
///         a full plugin pool.
pub fn write(name: &str, data: &[u8]) -> Result<(), RmemError> {
    let n = cstr(name)?;
    let rc = unsafe { host_rmem_write_named(n.as_ptr(), data.as_ptr(), data.len() as i32) };
    if rc == 0 {
        Ok(())
    } else {
        Err(RmemError)
    }
}

/// \brief Erase the contents of a named rmem slot.
/// \param name Slot name declared in `capabilities.rmem`.
/// \return `Ok(())` on success, `Err(RmemError)` if the slot does not
///         exist or is capability-denied.
pub fn erase(name: &str) -> Result<(), RmemError> {
    let n = cstr(name)?;
    let rc = unsafe { host_rmem_erase_named(n.as_ptr()) };
    if rc == 0 {
        Ok(())
    } else {
        Err(RmemError)
    }
}

/// \brief Whether a named slot currently holds a value.
/// \param name Slot name declared in `capabilities.rmem`.
/// \return `true` if the slot has been written to since allocation.
pub fn is_used(name: &str) -> bool {
    let Ok(n) = cstr(name) else { return false };
    unsafe { host_rmem_name_used(n.as_ptr()) != 0 }
}

/// \brief Size of one rmem slot as reported by the firmware.
/// \return Slot capacity in bytes (header + payload).
pub fn slot_size() -> u16 {
    unsafe { host_rmem_slot_size() }
}

#[link(wasm_import_module = "cdc")]
extern "C" {
    fn host_rmem_read_named(name: *const u8, buf: *mut u8, buf_size: i32) -> c_int;
    fn host_rmem_write_named(name: *const u8, buf: *const u8, len: i32) -> c_int;
    fn host_rmem_erase_named(name: *const u8) -> c_int;
    fn host_rmem_name_used(name: *const u8) -> c_int;
    fn host_rmem_slot_size() -> u16;
}
