//! \file
//! \brief Generic TCP / UDP client sockets for plugin network protocols.
//!
//! This module is intentionally transport-level: protocols such as MQTT can
//! build their own framing on top. Both [`TcpStream`] and [`UdpSocket`] are
//! connected to a single remote endpoint, so the read/write surface is
//! identical; a UDP socket simply fixes its peer at connect time. Calls require
//! `capabilities.socket = true` and an active network connection.

use crate::{Error, Result};
use alloc::ffi::CString;
use core::ffi::{c_char, c_int};

/// Protocol selector for `host_socket_open`; mirrors the host header.
const HOST_SOCK_TCP: u8 = 0;
const HOST_SOCK_UDP: u8 = 1;

/// Shared connected-socket handle. Closed automatically on drop.
struct Socket {
    handle: c_int,
}

impl Socket {
    fn open(proto: u8, host: &str, port: u16, timeout_ms: u32) -> Result<Self> {
        let host_c = CString::new(host).map_err(|_| Error::InvalidArg)?;
        let rc = unsafe { host_socket_open(proto, host_c.as_ptr(), port, timeout_ms) };
        if rc <= 0 {
            return Err(Error::from_code(rc));
        }
        Ok(Self { handle: rc })
    }

    fn write(&self, data: &[u8], timeout_ms: u32) -> Result<usize> {
        let rc = unsafe {
            host_socket_write(self.handle, crate::slice_ptr(data), data.len(), timeout_ms)
        };
        if rc < 0 {
            Err(Error::from_code(rc))
        } else {
            Ok(rc as usize)
        }
    }

    fn read(&self, out: &mut [u8], timeout_ms: u32) -> Result<usize> {
        let rc = unsafe { host_socket_read(self.handle, out.as_mut_ptr(), out.len(), timeout_ms) };
        if rc < 0 {
            Err(Error::from_code(rc))
        } else {
            Ok(rc as usize)
        }
    }
}

impl Drop for Socket {
    fn drop(&mut self) {
        unsafe {
            host_socket_close(self.handle);
        }
    }
}

/// \brief Connected TCP client stream. Closed automatically on drop.
pub struct TcpStream(Socket);

impl TcpStream {
    /// \brief Connect to a remote TCP endpoint.
    /// \param host Hostname or numeric IPv4/IPv6 address.
    /// \param port TCP port.
    /// \param timeout_ms Connect timeout in milliseconds.
    /// \return A live stream handle on success.
    pub fn connect(host: &str, port: u16, timeout_ms: u32) -> Result<Self> {
        Ok(Self(Socket::open(HOST_SOCK_TCP, host, port, timeout_ms)?))
    }

    /// \brief Wrap a socket handle already opened by the host (e.g. an inbound
    ///        connection from [`crate::net::accept`]). Closed on drop.
    pub(crate) fn from_handle(handle: c_int) -> Self {
        Self(Socket { handle })
    }

    /// \brief Write bytes to the stream.
    /// \param data Bytes to write.
    /// \param timeout_ms Write timeout in milliseconds.
    /// \return Number of bytes accepted by the host.
    pub fn write(&self, data: &[u8], timeout_ms: u32) -> Result<usize> {
        self.0.write(data, timeout_ms)
    }

    /// \brief Read bytes from the stream.
    /// \param out Destination buffer.
    /// \param timeout_ms Read timeout in milliseconds.
    /// \return Number of bytes read; 0 means EOF.
    pub fn read(&self, out: &mut [u8], timeout_ms: u32) -> Result<usize> {
        self.0.read(out, timeout_ms)
    }
}

/// \brief Connected UDP client socket. The peer is fixed at connect time, so
/// `write`/`read` send to and receive from that single endpoint. Closed
/// automatically on drop.
pub struct UdpSocket(Socket);

impl UdpSocket {
    /// \brief Bind a UDP socket to a fixed remote endpoint.
    /// \param host Hostname or numeric IPv4/IPv6 address.
    /// \param port UDP port.
    /// \param timeout_ms Reserved for symmetry with TCP; UDP connect is local.
    /// \return A live socket handle on success.
    pub fn connect(host: &str, port: u16, timeout_ms: u32) -> Result<Self> {
        Ok(Self(Socket::open(HOST_SOCK_UDP, host, port, timeout_ms)?))
    }

    /// \brief Send a datagram to the connected peer.
    /// \param data Bytes to send.
    /// \param timeout_ms Send timeout in milliseconds.
    /// \return Number of bytes accepted by the host.
    pub fn write(&self, data: &[u8], timeout_ms: u32) -> Result<usize> {
        self.0.write(data, timeout_ms)
    }

    /// \brief Receive a datagram from the connected peer.
    /// \param out Destination buffer.
    /// \param timeout_ms Receive timeout in milliseconds.
    /// \return Number of bytes read.
    pub fn read(&self, out: &mut [u8], timeout_ms: u32) -> Result<usize> {
        self.0.read(out, timeout_ms)
    }
}

#[link(wasm_import_module = "cdc")]
extern "C" {
    fn host_socket_open(proto: u8, host: *const c_char, port: u16, timeout_ms: u32) -> c_int;
    fn host_socket_write(handle: c_int, data: *const u8, len: usize, timeout_ms: u32) -> c_int;
    fn host_socket_read(handle: c_int, out: *mut u8, cap: usize, timeout_ms: u32) -> c_int;
    fn host_socket_close(handle: c_int) -> c_int;
}
