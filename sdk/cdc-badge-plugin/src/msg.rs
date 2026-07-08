//! \file
//! \brief Badge-to-badge message transfer: register typed-payload handlers and
//!        push payloads to nearby badges.
//!
//! A plugin registers one or more MIME types it can receive with
//! [`register_handler`]. When a transfer of that type completes (after the
//! local user consented and the encrypted transfer finished), the firmware
//! fires the `action_id` passed at registration; the `plugin_on_action` handler
//! then pulls the bytes with [`consume`]. To send, hand a typed payload to
//! [`send_interactive`], which opens the firmware-owned peer picker and
//! consent/progress UI. Sending requires the `ble` capability plus at least one
//! `message_types` entry in the manifest.
//!
//! Payload bytes are opaque. For text MIME types they are UTF-8; render them
//! through the normal `ui` functions, which convert to the display codepage.

use crate::{check, Error, Result};
use alloc::ffi::CString;
use alloc::string::String;
use alloc::vec::Vec;
use core::ffi::{c_char, c_int};

/// Maximum payload bytes in one transfer (`HOST_MSG_PAYLOAD_MAX`,
/// generated from the header by build.rs so it cannot drift).
pub const PAYLOAD_MAX: usize = crate::ffi::HOST_MSG_PAYLOAD_MAX as usize;
/// MIME buffer size including the NUL (`HOST_MSG_MIME_MAX`, generated from the header).
pub const MIME_MAX: usize = crate::ffi::HOST_MSG_MIME_MAX as usize;

/// Send flag (mirrors `HOST_MSG_FLAG_PERSIST`): remember the verified pairing
/// for this runtime session. The first send still prompts once; follow-up sends
/// to the same peer reconnect silently. Trust is dropped on reboot.
pub const FLAG_PERSIST: u32 = 0x01;

/// \brief A payload pulled from a completed inbound transfer.
pub struct Received {
    pub mime: String,
    pub data: Vec<u8>,
}

/// \brief Register that this plugin handles an inbound MIME type.
///
/// On a completed transfer of `mime_type` the firmware fires `action_id` via
/// `plugin_on_action`; read the payload with [`consume`].
pub fn register_handler(mime_type: &str, action_id: u32) -> Result<()> {
    let c = CString::new(mime_type).map_err(|_| Error::InvalidArg)?;
    check(unsafe { host_msg_register_handler(c.as_ptr(), action_id) })
}

/// \brief Drop a handler registered with [`register_handler`].
pub fn unregister_handler(mime_type: &str) -> Result<()> {
    let c = CString::new(mime_type).map_err(|_| Error::InvalidArg)?;
    check(unsafe { host_msg_unregister_handler(c.as_ptr()) })
}

/// \brief Pull the payload delivered by the most recent inbound message action.
/// \param max_len Maximum payload bytes to read.
/// \return The received message, or `None` when nothing is pending.
pub fn consume(max_len: usize) -> Option<Received> {
    let cap = max_len.saturating_add(1);
    let mut buf = Vec::<u8>::with_capacity(cap);
    let mut mime = Vec::<u8>::with_capacity(MIME_MAX);
    let n = unsafe {
        buf.set_len(cap);
        mime.set_len(MIME_MAX);
        host_msg_consume(
            buf.as_mut_ptr(),
            cap,
            mime.as_mut_ptr() as *mut c_char,
            MIME_MAX,
        )
    };
    if n < 0 {
        return None;
    }
    buf.truncate(n as usize);
    let mend = mime.iter().position(|&b| b == 0).unwrap_or(mime.len());
    mime.truncate(mend);
    Some(Received {
        mime: String::from_utf8_lossy(&mime).into_owned(),
        data: buf,
    })
}

/// \brief Consume and decode the payload as UTF-8 text.
/// \return `Some((mime, text))` or `None`.
pub fn consume_text(max_len: usize) -> Option<(String, String)> {
    let r = consume(max_len)?;
    Some((r.mime, String::from_utf8_lossy(&r.data).into_owned()))
}

/// \brief Send a typed payload via the firmware peer picker + consent UI.
/// \param flags Bitwise OR of `FLAG_*` (`0` for the default behaviour).
pub fn send_interactive_with(mime_type: &str, data: &[u8], flags: u32) -> Result<()> {
    let c = CString::new(mime_type).map_err(|_| Error::InvalidArg)?;
    check(unsafe {
        host_msg_send_interactive(c.as_ptr(), crate::slice_ptr(data), data.len(), flags)
    })
}

/// \brief Send a typed payload via the firmware peer picker + consent UI.
pub fn send_interactive(mime_type: &str, data: &[u8]) -> Result<()> {
    send_interactive_with(mime_type, data, 0)
}

/// \brief Convenience: send a UTF-8 string as `text/plain`.
/// \param flags Bitwise OR of `FLAG_*` (`0` for the default behaviour).
pub fn send_text_interactive_with(text: &str, flags: u32) -> Result<()> {
    send_interactive_with("text/plain", text.as_bytes(), flags)
}

/// \brief Convenience: send a UTF-8 string as `text/plain`.
pub fn send_text_interactive(text: &str) -> Result<()> {
    send_interactive_with("text/plain", text.as_bytes(), 0)
}

/// \brief Send directly to a known peer address (the peer still consents).
/// \param flags Bitwise OR of `FLAG_*` (`0` for the default behaviour).
pub fn send_with(
    addr: [u8; 6],
    addr_type: u8,
    mime_type: &str,
    data: &[u8],
    flags: u32,
) -> Result<()> {
    let c = CString::new(mime_type).map_err(|_| Error::InvalidArg)?;
    check(unsafe {
        host_msg_send(
            addr.as_ptr(),
            addr_type,
            c.as_ptr(),
            crate::slice_ptr(data),
            data.len(),
            flags,
        )
    })
}

/// \brief Send directly to a known peer address (the peer still consents).
pub fn send(addr: [u8; 6], addr_type: u8, mime_type: &str, data: &[u8]) -> Result<()> {
    send_with(addr, addr_type, mime_type, data, 0)
}

#[link(wasm_import_module = "cdc")]
extern "C" {
    fn host_msg_register_handler(mime: *const c_char, action_id: u32) -> c_int;
    fn host_msg_unregister_handler(mime: *const c_char) -> c_int;
    fn host_msg_consume(
        buf: *mut u8,
        buf_size: usize,
        mime_out: *mut c_char,
        mime_size: usize,
    ) -> c_int;
    fn host_msg_send_interactive(
        mime: *const c_char,
        data: *const u8,
        len: usize,
        flags: u32,
    ) -> c_int;
    fn host_msg_send(
        addr: *const u8,
        addr_type: u8,
        mime: *const c_char,
        data: *const u8,
        len: usize,
        flags: u32,
    ) -> c_int;
}
