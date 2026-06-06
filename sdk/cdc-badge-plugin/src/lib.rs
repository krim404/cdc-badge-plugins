//! \file
//! \brief Safe Rust bindings for the CDC Badge OS WASM plugin host API.
//!
//! Mirrors `sdk/host_api.h`. Host functions live in the `cdc` WASM import
//! module. This crate provides:
//!   - Raw `extern "C"` declarations under `ffi`
//!   - Safe high-level wrappers in the top-level modules
//!   - The `plugin_main!` macro to wire up the lifecycle exports

#![no_std]

extern crate alloc;

use core::ffi::c_int;

/// \brief Major part of the host API level this SDK targets.
pub const HOST_API_LEVEL_MAJOR: u16 = 0;
/// \brief Minor part of the host API level this SDK targets.
pub const HOST_API_LEVEL_MINOR: u16 = 8;

/// \brief Unified error type for every fallible host API call.
///
/// Variants mirror the `HOST_ERR_*` codes in `sdk/host_api.h`. Any other code
/// the host returns maps to [`Error::Other`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// Unspecified failure (`HOST_ERR_GENERIC`).
    Generic,
    /// An argument was invalid (`HOST_ERR_INVALID_ARG`).
    InvalidArg,
    /// The plugin lacks the required capability (`HOST_ERR_NO_CAPABILITY`).
    NoCapability,
    /// The requested item does not exist (`HOST_ERR_NOT_FOUND`).
    NotFound,
    /// The operation timed out (`HOST_ERR_TIMEOUT`).
    Timeout,
    /// Allocation failed (`HOST_ERR_NO_MEMORY`).
    NoMemory,
    /// The resource is busy (`HOST_ERR_BUSY`).
    Busy,
    /// The operation is not supported (`HOST_ERR_NOT_SUPPORTED`).
    NotSupported,
    /// The retained-memory pool is full (`HOST_ERR_RMEM_FULL`).
    RmemFull,
    /// A host return code outside the documented `HOST_ERR_*` set.
    Other(c_int),
}

impl Error {
    /// \brief Map a non-zero host return code to an [`Error`].
    /// \param code Raw host return code; `0`/`HOST_OK` maps to [`Error::Generic`].
    pub fn from_code(code: c_int) -> Self {
        match code {
            ffi::HOST_ERR_GENERIC => Error::Generic,
            ffi::HOST_ERR_INVALID_ARG => Error::InvalidArg,
            ffi::HOST_ERR_NO_CAPABILITY => Error::NoCapability,
            ffi::HOST_ERR_NOT_FOUND => Error::NotFound,
            ffi::HOST_ERR_TIMEOUT => Error::Timeout,
            ffi::HOST_ERR_NO_MEMORY => Error::NoMemory,
            ffi::HOST_ERR_BUSY => Error::Busy,
            ffi::HOST_ERR_NOT_SUPPORTED => Error::NotSupported,
            ffi::HOST_ERR_RMEM_FULL => Error::RmemFull,
            ffi::HOST_OK => Error::Generic,
            other => Error::Other(other),
        }
    }

    /// \brief The raw host return code this error maps to.
    pub fn code(self) -> c_int {
        match self {
            Error::Generic => ffi::HOST_ERR_GENERIC,
            Error::InvalidArg => ffi::HOST_ERR_INVALID_ARG,
            Error::NoCapability => ffi::HOST_ERR_NO_CAPABILITY,
            Error::NotFound => ffi::HOST_ERR_NOT_FOUND,
            Error::Timeout => ffi::HOST_ERR_TIMEOUT,
            Error::NoMemory => ffi::HOST_ERR_NO_MEMORY,
            Error::Busy => ffi::HOST_ERR_BUSY,
            Error::NotSupported => ffi::HOST_ERR_NOT_SUPPORTED,
            Error::RmemFull => ffi::HOST_ERR_RMEM_FULL,
            Error::Other(c) => c,
        }
    }
}

/// \brief Result alias used throughout the SDK: `Ok` on `HOST_OK`, otherwise
///        an [`Error`].
pub type Result<T> = core::result::Result<T, Error>;

/// \brief Map a raw host return code to `Result<()>`.
///
/// `HOST_OK` (0) becomes `Ok(())`; any other value becomes
/// `Err(Error::from_code(code))`.
pub fn check(code: c_int) -> Result<()> {
    if code == ffi::HOST_OK {
        Ok(())
    } else {
        Err(Error::from_code(code))
    }
}

pub mod ble;
pub mod canvas;
pub mod cmd;
pub mod crypto;
pub mod display;
pub mod event;
pub mod ffi;
pub mod fs;
pub mod gpio;
pub mod http;
pub mod i18n;
pub mod i2c;
pub mod keypad;
pub mod lockscreen;
pub mod log;
pub mod nvs;
pub mod pixel_strip;
pub mod power;
pub mod random;
pub mod rmem;
pub mod sao;
pub mod secure_element;
pub mod socket;
pub mod sysinfo;
pub mod time;
pub mod ui;
pub mod usb;
pub mod wifi;

#[cfg(all(feature = "allocator", target_arch = "wasm32"))]
#[global_allocator]
static GLOBAL: dlmalloc::GlobalDlmalloc = dlmalloc::GlobalDlmalloc;

/// \brief Emit the `plugin_required_api_major` / `_minor` exports the
///        host reads at load time.
///
/// Every plugin's `src/lib.rs` must invoke this exactly once at the top
/// of the file.
#[macro_export]
macro_rules! plugin_main {
    () => {
        #[no_mangle]
        pub extern "C" fn plugin_required_api_major() -> u16 {
            $crate::HOST_API_LEVEL_MAJOR
        }
        #[no_mangle]
        pub extern "C" fn plugin_required_api_minor() -> u16 {
            $crate::HOST_API_LEVEL_MINOR
        }
    };
}

#[cfg(all(feature = "panic_handler", target_arch = "wasm32"))]
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    use alloc::format;
    let msg = format!("{}", info);
    crate::log::error("PANIC", &msg);
    loop {}
}
