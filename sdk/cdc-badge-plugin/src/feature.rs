//! \file
//! \brief External features: invoke a named feature provided by another
//!        installed plugin, or provide one yourself.
//!
//! A provider declares `provides: ["thermo_print"]` under manifest
//! capabilities and calls [`register_provider`] for each entry in
//! `plugin_init`. A caller invokes the feature with [`use_ext_feature`]: the
//! firmware switches the provider to the foreground (like "Open with"),
//! fires its handler action, and the provider pulls the job bytes with
//! [`consume_job`]. The provider reports the outcome exactly once via
//! [`report_result`], which fires the caller's `status_action_id` with the
//! status code in `user_data`.
//!
//! `Ok` from [`use_ext_feature`] means *job accepted*, not done. The
//! foreground switch unloads a caller without `background: true` - its
//! status action is then dropped. When no provider is installed the firmware
//! shows a "no plugin for this feature" modal and the call returns
//! [`Error::NotFound`].

use crate::{check, Error, Result};
use alloc::ffi::CString;
use alloc::string::String;
use alloc::vec::Vec;
use core::ffi::{c_char, c_int};

/// Maximum job payload bytes (`HOST_EXT_FEATURE_PAYLOAD_MAX`, generated from
/// the header by build.rs so it cannot drift).
pub const PAYLOAD_MAX: usize = crate::ffi::HOST_EXT_FEATURE_PAYLOAD_MAX as usize;
/// Feature-name buffer size including the NUL (`HOST_EXT_FEATURE_NAME_MAX`).
pub const NAME_MAX: usize = crate::ffi::HOST_EXT_FEATURE_NAME_MAX as usize;

/// Provider status: job completed successfully.
pub const STATUS_DONE: i32 = crate::ffi::HOST_EXT_FEATURE_STATUS_DONE;
/// Provider status: generic failure. Provider-defined codes are >= 1.
pub const STATUS_ERROR: i32 = crate::ffi::HOST_EXT_FEATURE_STATUS_ERROR;

/// \brief A job pulled from the handler action fired for a provided feature.
pub struct Job {
    pub feature: String,
    pub data: Vec<u8>,
}

/// \brief True when an installed (and enabled) plugin provides `feature`.
pub fn feature_available(feature: &str) -> bool {
    let Ok(c) = CString::new(feature) else {
        return false;
    };
    unsafe { host_ext_feature_available(c.as_ptr()) == 0 }
}

/// \brief Invoke `feature` with a payload; the provider runs in the foreground.
///
/// Returns immediately after the job is accepted. The outcome arrives later
/// as `plugin_on_action(status_action_id, 0, status)` - only while this
/// caller is still loaded (declare `background: true` to survive the
/// foreground switch). Pass `status_action_id = 0` for fire-and-forget.
pub fn use_ext_feature(feature: &str, payload: &[u8], status_action_id: u32) -> Result<()> {
    let c = CString::new(feature).map_err(|_| Error::InvalidArg)?;
    check(unsafe {
        host_ext_feature_use(
            c.as_ptr(),
            crate::slice_ptr(payload),
            payload.len(),
            status_action_id,
        )
    })
}

/// \brief Register this plugin as the live handler for a feature it provides.
///
/// Call from `plugin_init` for every manifest `provides` entry. An incoming
/// job fires `plugin_on_action(action_id, 0, len)`; pull it with
/// [`consume_job`]. The handler firing right after `plugin_on_enter` is what
/// distinguishes "opened for a job" from "opened by the user".
pub fn register_provider(feature: &str, action_id: u32) -> Result<()> {
    let c = CString::new(feature).map_err(|_| Error::InvalidArg)?;
    check(unsafe { host_ext_feature_register_handler(c.as_ptr(), action_id) })
}

/// \brief Pull the job that fired the current handler action.
///
/// Valid only during that action dispatch.
/// \param max_len Maximum payload bytes to read.
pub fn consume_job(max_len: usize) -> Option<Job> {
    let mut buf = Vec::<u8>::with_capacity(max_len);
    let mut name = [0u8; NAME_MAX];
    let n = unsafe {
        buf.set_len(max_len);
        host_ext_feature_consume(
            buf.as_mut_ptr(),
            max_len,
            name.as_mut_ptr() as *mut c_char,
            NAME_MAX,
        )
    };
    if n < 0 {
        return None;
    }
    buf.truncate(n as usize);
    let end = name.iter().position(|&b| b == 0).unwrap_or(name.len());
    if end == 0 {
        return None; // no job delivered in this dispatch
    }
    Some(Job {
        feature: String::from_utf8_lossy(&name[..end]).into_owned(),
        data: buf,
    })
}

/// \brief Report the outcome of the job this provider is handling.
///
/// Fires the caller's status action (if the caller is still loaded) and
/// frees the job slot. Call exactly once per received job.
/// \param status [`STATUS_DONE`] or a provider-defined error code >= 1.
pub fn report_result(status: i32) -> Result<()> {
    check(unsafe { host_ext_feature_result(status) })
}

#[link(wasm_import_module = "cdc")]
extern "C" {
    fn host_ext_feature_available(feature: *const c_char) -> c_int;
    fn host_ext_feature_use(
        feature: *const c_char,
        data: *const u8,
        len: usize,
        status_action_id: u32,
    ) -> c_int;
    fn host_ext_feature_register_handler(feature: *const c_char, action_id: u32) -> c_int;
    fn host_ext_feature_consume(
        buf: *mut u8,
        buf_size: usize,
        feature_out: *mut c_char,
        feature_size: usize,
    ) -> c_int;
    fn host_ext_feature_result(status_code: i32) -> c_int;
}
