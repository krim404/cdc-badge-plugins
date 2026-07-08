//! \file
//! \brief Plugin-namespaced NVS (non-volatile storage) access.
//!
//! The host automatically routes every call into a per-plugin namespace
//! derived from `capabilities.nvs_namespace`, so there is no risk of
//! colliding with other plugins or the firmware itself.

use crate::{check, ffi, Error, Result};
use alloc::ffi::CString;
use alloc::string::String;
use alloc::vec::Vec;

/// \brief Read a binary blob from NVS.
/// \param key     NVS key inside the plugin's namespace.
/// \param max_len Maximum number of bytes to read.
/// \return The stored bytes, or `None` if the key is missing or unreadable.
pub fn get_blob(key: &str, max_len: usize) -> Option<Vec<u8>> {
    let k = CString::new(key).ok()?;
    let mut buf = Vec::<u8>::with_capacity(max_len);
    let n = unsafe {
        buf.set_len(max_len);
        ffi::host_nvs_get_blob(k.as_ptr(), buf.as_mut_ptr(), max_len)
    };
    if n < 0 {
        return None;
    }
    buf.truncate(n as usize);
    Some(buf)
}

/// \brief Write a binary blob to NVS.
/// \param key   NVS key inside the plugin's namespace.
/// \param value Payload bytes.
/// \return `Ok(())` on success, `Err` on failure.
pub fn set_blob(key: &str, value: &[u8]) -> Result<()> {
    let k = CString::new(key).map_err(|_| Error::InvalidArg)?;
    check(unsafe { ffi::host_nvs_set_blob(k.as_ptr(), crate::slice_ptr(value), value.len()) })
}

/// \brief Read a UTF-8 string from NVS.
/// \param key     NVS key inside the plugin's namespace.
/// \param max_len Maximum number of bytes to read (excl. NUL).
/// \return The stored string, or `None` if missing / non-UTF-8.
pub fn get_str(key: &str, max_len: usize) -> Option<String> {
    let k = CString::new(key).ok()?;
    let mut buf = Vec::<u8>::with_capacity(max_len);
    let rc = unsafe {
        buf.set_len(max_len);
        ffi::host_nvs_get_str(k.as_ptr(), buf.as_mut_ptr() as *mut _, max_len)
    };
    if rc != 0 {
        return None;
    }
    let n = buf.iter().position(|&b| b == 0).unwrap_or(max_len);
    buf.truncate(n);
    String::from_utf8(buf).ok()
}

/// \brief Write a UTF-8 string to NVS.
/// \param key   NVS key inside the plugin's namespace.
/// \param value String value (must not contain interior NULs).
/// \return `Ok(())` on success, `Err` on failure.
pub fn set_str(key: &str, value: &str) -> Result<()> {
    let k = CString::new(key).map_err(|_| Error::InvalidArg)?;
    let v = CString::new(value).map_err(|_| Error::InvalidArg)?;
    check(unsafe { ffi::host_nvs_set_str(k.as_ptr(), v.as_ptr()) })
}

/// \brief Read a 32-bit unsigned integer from NVS.
/// \param key NVS key inside the plugin's namespace.
/// \return The stored value, or `None` if the key is missing.
pub fn get_u32(key: &str) -> Option<u32> {
    let k = CString::new(key).ok()?;
    let mut out: u32 = 0;
    let rc = unsafe { ffi::host_nvs_get_u32(k.as_ptr(), &mut out) };
    if rc == 0 {
        Some(out)
    } else {
        None
    }
}

/// \brief Write a 32-bit unsigned integer to NVS.
/// \param key   NVS key inside the plugin's namespace.
/// \param value Value to store.
/// \return `Ok(())` on success, `Err` on failure.
pub fn set_u32(key: &str, value: u32) -> Result<()> {
    let k = CString::new(key).map_err(|_| Error::InvalidArg)?;
    check(unsafe { ffi::host_nvs_set_u32(k.as_ptr(), value) })
}

/// \brief Remove a key from NVS.
/// \param key NVS key inside the plugin's namespace.
/// \return `Ok(())` on success, including the "already absent" case.
pub fn erase(key: &str) -> Result<()> {
    let k = CString::new(key).map_err(|_| Error::InvalidArg)?;
    check(unsafe { ffi::host_nvs_erase(k.as_ptr()) })
}

/// \brief Erase every key in the plugin's own NVS namespace.
///
/// Scoped to the calling plugin's `plugin_<id>` namespace by the host; it
/// cannot touch other plugins' data or firmware NVS. Destructive only to
/// this plugin's own stored values.
/// \return `Ok(())` on success, `Err` on failure.
pub fn erase_all() -> Result<()> {
    check(unsafe { ffi::host_nvs_erase_all() })
}

/// \brief Enumerate the keys stored in the plugin's namespace.
/// \param max_bytes Capacity for the NUL-separated key list returned by the host.
/// \return The keys, or `None` on failure.
pub fn list_keys(max_bytes: usize) -> Option<Vec<String>> {
    let mut buf = Vec::<u8>::with_capacity(max_bytes);
    let mut len = max_bytes;
    let rc = unsafe {
        buf.set_len(max_bytes);
        ffi::host_nvs_list_keys(buf.as_mut_ptr() as *mut _, &mut len)
    };
    if rc != 0 {
        return None;
    }
    buf.truncate(len);
    let keys = buf
        .split(|&b| b == 0)
        .filter(|s| !s.is_empty())
        .filter_map(|s| core::str::from_utf8(s).ok().map(String::from))
        .collect();
    Some(keys)
}
