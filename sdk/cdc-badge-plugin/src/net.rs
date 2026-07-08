//! \file
//! \brief Inbound TCP listener: the server side the `socket` module lacks.
//!
//! The plugin picks a port with [`listen`]; the firmware binds/listens/accepts
//! on its own task and fires the registered action while connections wait. The
//! handler calls [`accept`] to take the next connection as a [`TcpStream`] and
//! drives it with the normal read/write. Requires `capabilities.net_listen`.
//! WiFi must be up (see the `wifi` module) for clients to reach the badge; the
//! listener binds regardless and simply waits. One listener per plugin.

use crate::socket::TcpStream;
use crate::{check, Result};
use core::ffi::c_int;

/// \brief Start a TCP listener on `port`; fires `action_id` while clients wait.
pub fn listen(port: u16, action_id: u32) -> Result<()> {
    check(unsafe { host_net_listen(port, action_id) })
}

/// \brief Take the next accepted connection, or `None` when none is pending.
///
/// Call from the handler fired by [`listen`]. The returned stream uses the same
/// read/write/close as an outbound socket and closes on drop. Only
/// `HOST_ERR_NOT_FOUND` (no pending connection) maps to `None` silently; any
/// other host error is logged at debug level before `None` is returned.
pub fn accept() -> Option<TcpStream> {
    let rc = unsafe { host_net_accept() };
    if rc >= 1 {
        return Some(TcpStream::from_handle(rc));
    }
    if rc != crate::ffi::HOST_ERR_NOT_FOUND {
        crate::log::debug("net", &alloc::format!("accept failed: {}", rc));
    }
    None
}

/// \brief Stop the listener (pass 0 or the listening port).
pub fn close(port: u16) -> Result<()> {
    check(unsafe { host_net_close(port) })
}

#[link(wasm_import_module = "cdc")]
extern "C" {
    fn host_net_listen(port: u16, action_id: u32) -> c_int;
    fn host_net_accept() -> c_int;
    fn host_net_close(port: u16) -> c_int;
}
