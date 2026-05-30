//! \file
//! \brief Sandboxed vFAT file storage for plugins.
//!
//! Each plugin gets a private folder on the badge's plugins FAT partition. The
//! host confines every path to that folder, so a plugin can only ever touch
//! its own files. Requires the `vfat` capability in the manifest. `name` is a
//! bare filename ([A-Za-z0-9._-], no path separators, no leading dot).

use crate::{check, ffi, Error, Result};
use alloc::ffi::CString;
use alloc::string::String;
use alloc::vec::Vec;

/// \brief Create or overwrite a file with raw bytes.
/// \param name File name inside the plugin's folder.
/// \param data Payload bytes.
/// \return `Ok(())` on success, `Err` on failure.
pub fn write(name: &str, data: &[u8]) -> Result<()> {
    let n = CString::new(name).map_err(|_| Error::InvalidArg)?;
    check(unsafe { ffi::host_fs_write(n.as_ptr(), data.as_ptr(), data.len()) })
}

/// \brief Create or overwrite a file with UTF-8 text.
pub fn write_str(name: &str, text: &str) -> Result<()> {
    write(name, text.as_bytes())
}

/// \brief Read a file into a byte buffer.
/// \param name    File name inside the plugin's folder.
/// \param max_len Maximum number of bytes to read.
/// \return The bytes, or `None` if the file is missing / unreadable.
pub fn read(name: &str, max_len: usize) -> Option<Vec<u8>> {
    let n = CString::new(name).ok()?;
    let mut buf = Vec::<u8>::with_capacity(max_len);
    let rc = unsafe {
        buf.set_len(max_len);
        ffi::host_fs_read(n.as_ptr(), buf.as_mut_ptr(), max_len)
    };
    if rc < 0 {
        return None;
    }
    buf.truncate(rc as usize);
    Some(buf)
}

/// \brief Read a file as a UTF-8 string.
pub fn read_str(name: &str, max_len: usize) -> Option<String> {
    String::from_utf8(read(name, max_len)?).ok()
}

/// \brief Delete a file. \return `Ok(())` on success, `Err` if missing.
pub fn remove(name: &str) -> Result<()> {
    let n = CString::new(name).map_err(|_| Error::InvalidArg)?;
    check(unsafe { ffi::host_fs_remove(n.as_ptr()) })
}

/// \brief Get the size of a file in bytes.
/// \return The size, or `None` if the file does not exist.
pub fn size(name: &str) -> Option<usize> {
    let n = CString::new(name).ok()?;
    let rc = unsafe { ffi::host_fs_size(n.as_ptr()) };
    if rc < 0 {
        None
    } else {
        Some(rc as usize)
    }
}

/// \brief List the plugin's own files.
/// \param max_bytes Capacity for the '\n'-separated name list from the host.
/// \return The file names, or `None` on failure.
pub fn list(max_bytes: usize) -> Option<Vec<String>> {
    let mut buf = Vec::<u8>::with_capacity(max_bytes);
    let rc = unsafe {
        buf.set_len(max_bytes);
        ffi::host_fs_list(buf.as_mut_ptr() as *mut _, max_bytes)
    };
    if rc < 0 {
        return None;
    }
    let got = core::cmp::min(rc as usize, max_bytes);
    buf.truncate(got);
    Some(
        buf.split(|&b| b == b'\n')
            .filter(|s| !s.is_empty())
            .filter_map(|s| core::str::from_utf8(s).ok().map(String::from))
            .collect(),
    )
}

/// \brief Open one of the plugin's files in the on-screen text viewer
///        (the same scrollable view the file explorer uses). Handy for a
///        bundled readme / help page.
/// \return `Ok(())` on success, `Err` on failure.
pub fn view(name: &str) -> Result<()> {
    let n = CString::new(name).map_err(|_| Error::InvalidArg)?;
    check(unsafe { ffi::host_fs_view(n.as_ptr()) })
}
