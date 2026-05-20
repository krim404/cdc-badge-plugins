//! \file
//! \brief WiFi request and release helpers.
//!
//! Usually not called directly because plugins put `wifi_connected` in
//! their manifest prerequisites and the host brings the connection up
//! before `plugin_on_enter`. Use these only when a plugin needs to drop
//! and re-acquire WiFi mid-session.

use core::ffi::c_int;

/// \brief Generic WiFi error returned by the helpers in this module.
#[derive(Debug, Clone, Copy)]
pub struct WifiError;

/// \brief Request the host to bring WiFi up.
/// \param timeout_ms How long to wait for the connection in milliseconds.
/// \return `Ok(())` on success, `Err(WifiError)` on timeout or failure.
pub fn request(timeout_ms: u32) -> Result<(), WifiError> {
    let rc = unsafe { host_wifi_request(timeout_ms) };
    if rc == 0 {
        Ok(())
    } else {
        Err(WifiError)
    }
}

/// \brief Release the WiFi reservation taken with [`request`].
pub fn release() {
    unsafe {
        host_wifi_release();
    }
}

/// \brief Whether the badge currently has an active WiFi connection.
/// \return `true` if associated with an access point.
pub fn is_connected() -> bool {
    unsafe { host_wifi_is_connected() != 0 }
}

#[link(wasm_import_module = "cdc")]
extern "C" {
    fn host_wifi_request(timeout_ms: u32) -> c_int;
    fn host_wifi_release() -> c_int;
    fn host_wifi_is_connected() -> c_int;
}
