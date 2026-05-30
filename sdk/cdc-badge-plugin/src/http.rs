//! \file
//! \brief HTTP client wrapper over the host's chunked transport.
//!
//! Plugins build a request through [`Request::open`], optionally set
//! headers and body, then call [`Request::perform`] to send it. The
//! response body is read with [`Request::read_to_string`]. The transport
//! handle is closed automatically when the `Request` is dropped.

use alloc::ffi::CString;
use alloc::string::String;
use alloc::vec::Vec;
use core::ffi::{c_char, c_int};

pub const GET: u8 = 0;
pub const POST: u8 = 1;
pub const PUT: u8 = 2;
pub const DELETE: u8 = 3;

/// \brief Generic HTTP error returned by the helpers in this module.
#[derive(Debug, Clone, Copy)]
pub struct HttpError;

/// \brief A pending or in-flight HTTP request. Closes its host handle on drop.
pub struct Request {
    handle: i32,
}

impl Request {
    /// \brief Open a new HTTP request handle.
    /// \param method     One of `GET`, `POST`, `PUT`, `DELETE`.
    /// \param url        Absolute URL including scheme.
    /// \param timeout_ms Connect + response timeout in milliseconds.
    /// \return The request on success, `Err(HttpError)` on URL/host failure.
    pub fn open(method: u8, url: &str, timeout_ms: u32) -> Result<Self, HttpError> {
        let url_c = CString::new(url).map_err(|_| HttpError)?;
        let rc = unsafe { host_http_open(method, url_c.as_ptr(), timeout_ms) };
        if rc <= 0 {
            return Err(HttpError);
        }
        Ok(Self { handle: rc })
    }

    /// \brief Add a request header.
    /// \param key   Header name (case-insensitive).
    /// \param value Header value.
    /// \return `Ok(())` on success, `Err(HttpError)` on encoding failure.
    pub fn header(&self, key: &str, value: &str) -> Result<(), HttpError> {
        let k = CString::new(key).map_err(|_| HttpError)?;
        let v = CString::new(value).map_err(|_| HttpError)?;
        let rc = unsafe { host_http_set_header(self.handle, k.as_ptr(), v.as_ptr()) };
        if rc == 0 {
            Ok(())
        } else {
            Err(HttpError)
        }
    }

    /// \brief Set the request body.
    /// \param body Raw bytes to send; the Content-Length header is added
    ///             automatically by the host.
    /// \return `Ok(())` on success, `Err(HttpError)` on failure.
    pub fn body(&self, body: &[u8]) -> Result<(), HttpError> {
        let rc = unsafe { host_http_set_body(self.handle, body.as_ptr(), body.len()) };
        if rc == 0 {
            Ok(())
        } else {
            Err(HttpError)
        }
    }

    /// \brief Send the request and read the response status.
    /// \return The HTTP status code on success, `Err(HttpError)` on
    ///         network failure or timeout.
    pub fn perform(&self) -> Result<i32, HttpError> {
        let rc = unsafe { host_http_perform(self.handle) };
        if rc != 0 {
            return Err(HttpError);
        }
        let status = unsafe { host_http_status(self.handle) };
        Ok(status)
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
    /// \return The decoded body, or `Err(HttpError)` on network failure
    ///         or non-UTF-8 content.
    pub fn read_to_string(&self) -> Result<String, HttpError> {
        let mut out = Vec::new();
        let mut buf = [0u8; 1024];
        loop {
            let n = unsafe { host_http_read_chunk(self.handle, buf.as_mut_ptr(), buf.len()) };
            if n < 0 {
                return Err(HttpError);
            }
            if n == 0 {
                break;
            }
            out.extend_from_slice(&buf[..n as usize]);
        }
        String::from_utf8(out).map_err(|_| HttpError)
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
