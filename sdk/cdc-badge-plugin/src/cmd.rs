//! \file
//! \brief Plugin command channel.
//!
//! The host forwards a command string (e.g. from the `PLUGIN CMD <id> <args>`
//! serial subcommand) by firing the optional `plugin_on_cmd(len)` export. The
//! plugin pulls the bytes with [`consume`] inside that handler.

use crate::ffi;
use alloc::vec::Vec;

/// \brief Read the command string buffered by the host for this plugin.
/// \param max_len Buffer capacity; allocates `max_len + 1` bytes.
/// \return The command, or `None` if there is no pending command.
pub fn consume(max_len: usize) -> Option<alloc::string::String> {
    let cap = max_len.saturating_add(1);
    let mut buf = Vec::<u8>::with_capacity(cap);
    let rc = unsafe {
        buf.set_len(cap);
        ffi::host_cmd_consume(buf.as_mut_ptr() as *mut core::ffi::c_char, cap)
    };
    if rc < 0 {
        return None;
    }
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    buf.truncate(end);
    alloc::string::String::from_utf8(buf).ok()
}
