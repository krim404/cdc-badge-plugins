//! \file
//! \brief HTTP client wrapper over the host's chunked transport.
//!
//! Plugins build a request through [`Request::open`], optionally set
//! headers and body, then call [`Request::perform`] to send it. The
//! response body is read with [`Request::read_to_string`]. The transport
//! handle is closed automatically when the `Request` is dropped.

use crate::{check, Error, Result};
use alloc::ffi::CString;
use alloc::string::String;
use alloc::vec::Vec;
use core::ffi::{c_char, c_int};

pub const GET: u8 = 0;
pub const POST: u8 = 1;
pub const PUT: u8 = 2;
pub const DELETE: u8 = 3;

/// \brief A pending or in-flight HTTP request. Closes its host handle on drop.
pub struct Request {
    handle: i32,
}

impl Request {
    /// \brief Open a new HTTP request handle.
    /// \param method     One of `GET`, `POST`, `PUT`, `DELETE`.
    /// \param url        Absolute URL including scheme.
    /// \param timeout_ms Connect + response timeout in milliseconds.
    /// \return The request on success, `Err` on URL/host failure.
    pub fn open(method: u8, url: &str, timeout_ms: u32) -> Result<Self> {
        let url_c = CString::new(url).map_err(|_| Error::InvalidArg)?;
        let rc = unsafe { host_http_open(method, url_c.as_ptr(), timeout_ms) };
        if rc <= 0 {
            return Err(Error::from_code(rc));
        }
        Ok(Self { handle: rc })
    }

    /// \brief Add a request header.
    /// \param key   Header name (case-insensitive).
    /// \param value Header value.
    /// \return `Ok(())` on success, `Err` on encoding failure.
    pub fn header(&self, key: &str, value: &str) -> Result<()> {
        let k = CString::new(key).map_err(|_| Error::InvalidArg)?;
        let v = CString::new(value).map_err(|_| Error::InvalidArg)?;
        check(unsafe { host_http_set_header(self.handle, k.as_ptr(), v.as_ptr()) })
    }

    /// \brief Set the request body.
    /// \param body Raw bytes to send; the Content-Length header is added
    ///             automatically by the host.
    /// \return `Ok(())` on success, `Err` on failure.
    pub fn body(&self, body: &[u8]) -> Result<()> {
        check(unsafe { host_http_set_body(self.handle, crate::slice_ptr(body), body.len()) })
    }

    /// \brief Send the request and read the response status.
    /// \return The HTTP status code on success, `Err` on network failure or
    ///         timeout.
    pub fn perform(&self) -> Result<i32> {
        check(unsafe { host_http_perform(self.handle) })?;
        Ok(unsafe { host_http_status(self.handle) })
    }

    /// \brief Response Content-Length advertised by the server.
    /// \return The byte count, or `0` when unknown or chunked.
    pub fn content_length(&self) -> usize {
        unsafe { host_http_content_length(self.handle) }
    }

    /// \brief Read the entire response body into a UTF-8 string.
    ///
    /// Internally streams the response in 1 KiB chunks; suitable for
    /// JSON, XML, plain text. Use a chunked manual read for very large
    /// payloads.
    /// \return The decoded body, or `Err` on network failure or non-UTF-8
    ///         content.
    pub fn read_to_string(&self) -> Result<String> {
        let mut out = Vec::new();
        let mut buf = [0u8; 1024];
        loop {
            let n = unsafe { host_http_read_chunk(self.handle, buf.as_mut_ptr(), buf.len()) };
            if n < 0 {
                return Err(Error::from_code(n));
            }
            if n == 0 {
                break;
            }
            out.extend_from_slice(&buf[..n as usize]);
        }
        String::from_utf8(out).map_err(|_| Error::Generic)
    }
}

impl Drop for Request {
    fn drop(&mut self) {
        unsafe {
            host_http_close(self.handle);
        }
    }
}

#[link(wasm_import_module = "cdc")]
extern "C" {
    fn host_http_open(method: u8, url: *const c_char, timeout_ms: u32) -> c_int;
    fn host_http_set_header(h: c_int, k: *const c_char, v: *const c_char) -> c_int;
    fn host_http_set_body(h: c_int, body: *const u8, len: usize) -> c_int;
    fn host_http_perform(h: c_int) -> c_int;
    fn host_http_status(h: c_int) -> c_int;
    fn host_http_read_chunk(h: c_int, buf: *mut u8, buf_size: usize) -> c_int;
    fn host_http_content_length(h: c_int) -> usize;
    fn host_http_close(h: c_int) -> c_int;
}
