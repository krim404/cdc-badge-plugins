//! \file
//! \brief vCard store access: read and manage the badge's own vCard and the
//!        received-card store.
//!
//! Requires the manifest capability `vcard: true`. All strings are vCard 4.0
//! UTF-8 text as stored; display strings are the parsed formatted names the
//! firmware's own contact list shows. Received cards are addressed by their
//! sorted position (the firmware's list order), 0-based. Write operations
//! re-sort the store, so re-read positions after any mutation.

use crate::{check, Error, Result};
use alloc::string::String;
use alloc::vec::Vec;
use core::ffi::{c_char, c_int};

/// Maximum vCard text length including the NUL (`HOST_VCARD_MAX_LEN`,
/// generated from the header by build.rs so it cannot drift).
pub const VCARD_MAX_LEN: usize = crate::ffi::HOST_VCARD_MAX_LEN as usize;

fn read_string(f: impl FnOnce(*mut c_char, usize) -> c_int) -> Option<String> {
    let mut buf = Vec::<u8>::with_capacity(VCARD_MAX_LEN + 1);
    let n = unsafe {
        buf.set_len(VCARD_MAX_LEN + 1);
        f(buf.as_mut_ptr() as *mut c_char, VCARD_MAX_LEN + 1)
    };
    if n <= 0 {
        return None;
    }
    buf.truncate(n as usize);
    Some(String::from_utf8_lossy(&buf).into_owned())
}

/// \brief The badge's own vCard text, or `None` when not set.
pub fn own() -> Option<String> {
    read_string(|buf, size| unsafe { host_vcard_get_own(buf, size) })
}

/// \brief Set / replace the badge's own vCard (vCard 4.0 UTF-8 text).
pub fn set_own(vcard: &str) -> Result<()> {
    if vcard.is_empty() || vcard.len() >= VCARD_MAX_LEN {
        return Err(Error::InvalidArg);
    }
    check(unsafe { host_vcard_set_own(vcard.as_ptr() as *const c_char, vcard.len()) })
}

/// \brief Number of received vCards in the store.
pub fn received_count() -> u16 {
    let n = unsafe { host_vcard_received_count() };
    if n < 0 {
        0
    } else {
        n as u16
    }
}

/// \brief The received vCard text at sorted position `index`.
pub fn received(index: u16) -> Option<String> {
    read_string(|buf, size| unsafe { host_vcard_received_get(index, buf, size) })
}

/// \brief The display name of the received vCard at sorted position `index`.
pub fn received_display(index: u16) -> Option<String> {
    read_string(|buf, size| unsafe { host_vcard_received_display(index, buf, size) })
}

/// \brief Store a new received vCard.
///
/// Fails with [`Error::Busy`] on an exact duplicate and [`Error::NoMemory`]
/// when the store is full.
pub fn received_add(vcard: &str) -> Result<()> {
    if vcard.is_empty() || vcard.len() >= VCARD_MAX_LEN {
        return Err(Error::InvalidArg);
    }
    check(unsafe { host_vcard_received_add(vcard.as_ptr() as *const c_char, vcard.len()) })
}

/// \brief Overwrite the received vCard at sorted position `index`.
pub fn received_update(index: u16, vcard: &str) -> Result<()> {
    if vcard.is_empty() || vcard.len() >= VCARD_MAX_LEN {
        return Err(Error::InvalidArg);
    }
    check(unsafe {
        host_vcard_received_update(index, vcard.as_ptr() as *const c_char, vcard.len())
    })
}

/// \brief Delete the received vCard at sorted position `index`.
pub fn received_delete(index: u16) -> Result<()> {
    check(unsafe { host_vcard_received_delete(index) })
}

#[link(wasm_import_module = "cdc")]
extern "C" {
    fn host_vcard_get_own(out: *mut c_char, out_size: usize) -> c_int;
    fn host_vcard_received_count() -> c_int;
    fn host_vcard_received_get(index: u16, out: *mut c_char, out_size: usize) -> c_int;
    fn host_vcard_received_display(index: u16, out: *mut c_char, out_size: usize) -> c_int;
    fn host_vcard_set_own(vcard: *const c_char, len: usize) -> c_int;
    fn host_vcard_received_add(vcard: *const c_char, len: usize) -> c_int;
    fn host_vcard_received_update(index: u16, vcard: *const c_char, len: usize) -> c_int;
    fn host_vcard_received_delete(index: u16) -> c_int;
}
