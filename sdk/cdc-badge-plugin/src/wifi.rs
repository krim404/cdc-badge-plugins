//! \file
//! \brief WiFi request and release helpers.
//!
//! Usually not called directly because plugins put `wifi_connected` in
//! their manifest prerequisites and the host brings the connection up
//! before `plugin_on_enter`. Use these only when a plugin needs to drop
//! and re-acquire WiFi mid-session.

use crate::{check, Error, Result};
use alloc::string::String;
use alloc::vec::Vec;
use core::ffi::c_int;

/// \brief One access point from a completed scan.
#[derive(Debug, Clone)]
pub struct ScanResult {
    pub ssid: String,
    pub bssid: [u8; 6],
    pub rssi: i8,
    pub channel: u8,
    pub auth_mode: u8,
}

#[repr(C)]
struct WifiScanRaw {
    ssid: [u8; 33],
    bssid: [u8; 6],
    rssi: i8,
    channel: u8,
    auth_mode: u8,
}

/// \brief Request the host to bring WiFi up.
/// \param timeout_ms How long to wait for the connection in milliseconds.
/// \return `Ok(())` on success, `Err` on timeout or failure.
pub fn request(timeout_ms: u32) -> Result<()> {
    check(unsafe { host_wifi_request(timeout_ms) })
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

fn read_cstr(max: usize, f: impl FnOnce(*mut u8, usize) -> c_int) -> Option<String> {
    let mut buf = Vec::<u8>::with_capacity(max);
    let rc = unsafe {
        buf.set_len(max);
        f(buf.as_mut_ptr(), max)
    };
    if rc != 0 {
        return None;
    }
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    buf.truncate(end);
    String::from_utf8(buf).ok()
}

/// \brief Read the SSID of the currently joined network.
/// \return The SSID, or `None` when not connected.
pub fn ssid() -> Option<String> {
    read_cstr(33, |p, n| unsafe { host_wifi_ssid(p as *mut _, n) })
}

/// \brief Read the current IPv4 address as dotted decimal.
/// \return The address string, or `None` when not connected.
pub fn ip() -> Option<String> {
    read_cstr(16, |p, n| unsafe { host_wifi_ip(p as *mut _, n) })
}

/// \brief Current access-point signal strength.
/// \return RSSI in dBm.
pub fn rssi() -> i8 {
    unsafe { host_wifi_rssi() }
}

/// \brief Read the station MAC address.
/// \return The 6-byte MAC, or `None` on failure.
pub fn mac() -> Option<[u8; 6]> {
    let mut out = [0u8; 6];
    let rc = unsafe { host_wifi_mac(out.as_mut_ptr()) };
    if rc == 0 {
        Some(out)
    } else {
        None
    }
}

/// \brief Start an asynchronous WiFi scan.
/// \return `Ok(())` on success, `Err` on failure.
pub fn start_scan() -> Result<()> {
    check(unsafe { host_wifi_start_scan() })
}

/// \brief Whether the scan started by [`start_scan`] has finished.
pub fn scan_done() -> bool {
    unsafe { host_wifi_scan_done() != 0 }
}

/// \brief Read the results of the last completed scan.
/// \param max Maximum number of access points to return.
/// \return The discovered access points, or `Err` on failure.
pub fn scan_results(max: usize) -> Result<Vec<ScanResult>> {
    let mut raw = Vec::<WifiScanRaw>::with_capacity(max);
    let mut count = max;
    let rc = unsafe {
        raw.set_len(max);
        host_wifi_scan_results(raw.as_mut_ptr(), &mut count)
    };
    if rc != 0 {
        return Err(Error::from_code(rc));
    }
    raw.truncate(count);
    let out = raw
        .iter()
        .map(|r| {
            let end = r.ssid.iter().position(|&b| b == 0).unwrap_or(r.ssid.len());
            ScanResult {
                ssid: String::from_utf8_lossy(&r.ssid[..end]).into_owned(),
                bssid: r.bssid,
                rssi: r.rssi,
                channel: r.channel,
                auth_mode: r.auth_mode,
            }
        })
        .collect();
    Ok(out)
}

#[link(wasm_import_module = "cdc")]
extern "C" {
    fn host_wifi_request(timeout_ms: u32) -> c_int;
    fn host_wifi_release() -> c_int;
    fn host_wifi_is_connected() -> c_int;
    fn host_wifi_ssid(out: *mut core::ffi::c_char, out_size: usize) -> c_int;
    fn host_wifi_ip(out: *mut core::ffi::c_char, out_size: usize) -> c_int;
    fn host_wifi_rssi() -> i8;
    fn host_wifi_mac(out: *mut u8) -> c_int;
    fn host_wifi_start_scan() -> c_int;
    fn host_wifi_scan_done() -> c_int;
    fn host_wifi_scan_results(out: *mut WifiScanRaw, count: *mut usize) -> c_int;
}
