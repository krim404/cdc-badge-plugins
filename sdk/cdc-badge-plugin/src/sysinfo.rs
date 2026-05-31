//! \file
//! \brief Firmware identity and feature gating.

use alloc::string::String;
use alloc::vec::Vec;
use core::ffi::{c_char, c_int};

/// \brief Whether the firmware was built with the given feature id enabled.
/// \param feature_id Firmware feature identifier.
pub fn feature_enabled(feature_id: u16) -> bool {
    unsafe { host_feature_enabled(feature_id) }
}

fn read_str(max: usize, f: impl FnOnce(*mut c_char, usize) -> c_int) -> Option<String> {
    let mut buf = Vec::<u8>::with_capacity(max);
    let rc = unsafe {
        buf.set_len(max);
        f(buf.as_mut_ptr() as *mut c_char, max)
    };
    if rc != 0 {
        return None;
    }
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    buf.truncate(end);
    String::from_utf8(buf).ok()
}

/// \brief Read the firmware semver string.
/// \return The version string, or `None` on failure.
pub fn firmware_version() -> Option<String> {
    read_str(32, |p, n| unsafe { host_get_firmware_version(p, n) })
}

/// \brief Read the build profile name (e.g. "release", "debug").
/// \return The profile string, or `None` on failure.
pub fn build_profile() -> Option<String> {
    read_str(32, |p, n| unsafe { host_get_build_profile(p, n) })
}

/// \brief Aggregate CPU load across all cores as 0..100 percent.
///
/// Sampled on demand and refreshed at most a few times per second, so calling
/// this every frame is cheap and returns a stable value between refreshes.
/// \return Current CPU load percentage (0..100).
pub fn cpu_load() -> u8 {
    unsafe { host_cpu_load() }
}

#[link(wasm_import_module = "cdc")]
extern "C" {
    fn host_feature_enabled(feature_id: u16) -> bool;
    fn host_get_firmware_version(out: *mut c_char, out_size: usize) -> c_int;
    fn host_get_build_profile(out: *mut c_char, out_size: usize) -> c_int;
    fn host_cpu_load() -> u8;
}
