//! \file
//! \brief Plugin-side log helpers that route to `host_log` on the badge.

use crate::ffi;
use alloc::ffi::CString;

fn log_with(level: u8, tag: &str, msg: &str) {
    let tag_c = match CString::new(tag) {
        Ok(c) => c,
        Err(_) => return,
    };
    let msg_c = match CString::new(msg) {
        Ok(c) => c,
        Err(_) => return,
    };
    unsafe {
        ffi::host_log(level, tag_c.as_ptr(), msg_c.as_ptr());
    }
}

/// \brief Log at ERROR level.
/// \param tag Short module/area tag shown in the badge log.
/// \param msg Message body.
pub fn error(tag: &str, msg: &str) {
    log_with(ffi::LOG_LEVEL_ERROR, tag, msg);
}

/// \brief Log at WARN level.
/// \param tag Short module/area tag shown in the badge log.
/// \param msg Message body.
pub fn warn(tag: &str, msg: &str) {
    log_with(ffi::LOG_LEVEL_WARN, tag, msg);
}

/// \brief Log at INFO level.
/// \param tag Short module/area tag shown in the badge log.
/// \param msg Message body.
pub fn info(tag: &str, msg: &str) {
    log_with(ffi::LOG_LEVEL_INFO, tag, msg);
}

/// \brief Log at DEBUG level.
/// \param tag Short module/area tag shown in the badge log.
/// \param msg Message body.
pub fn debug(tag: &str, msg: &str) {
    log_with(ffi::LOG_LEVEL_DEBUG, tag, msg);
}

/// \brief Log at VERBOSE level.
/// \param tag Short module/area tag shown in the badge log.
/// \param msg Message body.
pub fn verbose(tag: &str, msg: &str) {
    log_with(ffi::LOG_LEVEL_VERBOSE, tag, msg);
}
