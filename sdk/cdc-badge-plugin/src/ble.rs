//! \file
//! \brief Bluetooth Low Energy peripheral and central operations.
//!
//! Publish GATT services as a peripheral, or scan and talk to remote
//! devices as a central. All operations are gated by the BLE manifest
//! capability.

use alloc::string::String;
use alloc::vec::Vec;
use core::ffi::c_int;

/// \brief Generic BLE error returned by the helpers in this module.
#[derive(Debug, Clone, Copy)]
pub struct BleError;

/// \brief GATT service definition for the peripheral role.
#[repr(C)]
pub struct ServiceDef {
    pub uuid: [u8; 16],
    pub is_primary: u8,
    #[allow(dead_code)]
    reserved: [u8; 2],
    pub handle: u16,
}

impl ServiceDef {
    /// \brief Build a service definition from a 16-byte UUID.
    pub fn new(uuid: [u8; 16], is_primary: bool) -> Self {
        Self {
            uuid,
            is_primary: is_primary as u8,
            reserved: [0; 2],
            handle: 0,
        }
    }
}

/// \brief One device from a BLE central scan.
#[derive(Debug, Clone)]
pub struct ScanResult {
    pub addr: [u8; 6],
    pub rssi: i8,
    pub name: String,
}

#[repr(C)]
struct BleScanRaw {
    addr: [u8; 6],
    rssi: i8,
    name: [u8; 32],
}

/// \brief Whether the BLE stack is initialised and advertising/connectable.
pub fn is_enabled() -> bool {
    unsafe { host_ble_is_enabled() }
}

/// \brief Read the local BLE MAC address.
/// \return The 6-byte address, or `None` on failure.
pub fn mac() -> Option<[u8; 6]> {
    let mut out = [0u8; 6];
    if unsafe { host_ble_mac(out.as_mut_ptr()) } == 0 {
        Some(out)
    } else {
        None
    }
}

/// \brief Read the local BLE device name.
/// \return The name, or `None` on failure.
pub fn device_name() -> Option<String> {
    let mut buf = Vec::<u8>::with_capacity(64);
    let rc = unsafe {
        buf.set_len(64);
        host_ble_device_name(buf.as_mut_ptr() as *mut _, 64)
    };
    if rc != 0 {
        return None;
    }
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    buf.truncate(end);
    String::from_utf8(buf).ok()
}

/// \brief Signal strength of the active BLE link.
/// \return RSSI in dBm, or `0` when idle.
pub fn rssi() -> i8 {
    unsafe { host_ble_rssi() }
}

/// \brief Register a GATT service for the peripheral role.
/// \param def Service definition.
/// \return The assigned service handle, or `Err(BleError)` on failure.
pub fn register_service(def: &ServiceDef) -> Result<u32, BleError> {
    let mut handle: u32 = 0;
    let rc = unsafe { host_ble_register_service(def as *const ServiceDef, &mut handle) };
    if rc == 0 {
        Ok(handle)
    } else {
        Err(BleError)
    }
}

/// \brief Send a GATT notification on a registered characteristic.
pub fn send_notification(char_handle: u32, data: &[u8]) -> Result<(), BleError> {
    rc(unsafe { host_ble_send_notification(char_handle, data.as_ptr(), data.len()) })
}

/// \brief Send a GATT indication (acknowledged notification).
pub fn send_indication(char_handle: u32, data: &[u8]) -> Result<(), BleError> {
    rc(unsafe { host_ble_send_indication(char_handle, data.as_ptr(), data.len()) })
}

/// \brief Tear down a previously registered GATT service.
pub fn unregister_service(service_handle: u32) -> Result<(), BleError> {
    rc(unsafe { host_ble_unregister_service(service_handle) })
}

/// \brief Start an asynchronous BLE central scan.
pub fn scan_start() -> Result<(), BleError> {
    rc(unsafe { host_ble_scan_start() })
}

/// \brief Read the results of the last BLE central scan.
/// \param max Maximum number of devices to return.
/// \return The discovered devices, or `Err(BleError)` on failure.
pub fn scan_results(max: usize) -> Result<Vec<ScanResult>, BleError> {
    let mut raw = Vec::<BleScanRaw>::with_capacity(max);
    let mut count = max;
    let r = unsafe {
        raw.set_len(max);
        host_ble_scan_results(raw.as_mut_ptr(), &mut count)
    };
    if r != 0 {
        return Err(BleError);
    }
    raw.truncate(count);
    Ok(raw
        .iter()
        .map(|d| {
            let end = d.name.iter().position(|&b| b == 0).unwrap_or(d.name.len());
            ScanResult {
                addr: d.addr,
                rssi: d.rssi,
                name: String::from_utf8_lossy(&d.name[..end]).into_owned(),
            }
        })
        .collect())
}

/// \brief Initiate a BLE central connection to `addr`.
pub fn connect(addr: [u8; 6]) -> Result<(), BleError> {
    rc(unsafe { host_ble_connect(addr.as_ptr()) })
}

/// \brief Read a characteristic value from a connected peer.
/// \param conn Connection handle.
/// \param uuid 16-byte characteristic UUID.
/// \param max  Maximum number of bytes to read.
/// \return The value bytes, or `Err(BleError)` on failure.
pub fn read_char(conn: u32, uuid: [u8; 16], max: usize) -> Result<Vec<u8>, BleError> {
    let mut buf = Vec::<u8>::with_capacity(max);
    let mut len = max;
    let r = unsafe {
        buf.set_len(max);
        host_ble_read_char(conn, uuid.as_ptr(), buf.as_mut_ptr(), &mut len)
    };
    if r != 0 {
        return Err(BleError);
    }
    buf.truncate(len);
    Ok(buf)
}

/// \brief Write a characteristic value on a connected peer.
pub fn write_char(conn: u32, uuid: [u8; 16], data: &[u8]) -> Result<(), BleError> {
    rc(unsafe { host_ble_write_char(conn, uuid.as_ptr(), data.as_ptr(), data.len()) })
}

/// \brief Subscribe to notifications/indications on a peer characteristic.
/// \param action_id Plugin action fired on each incoming notification.
pub fn subscribe(conn: u32, uuid: [u8; 16], action_id: u32) -> Result<(), BleError> {
    rc(unsafe { host_ble_subscribe(conn, uuid.as_ptr(), action_id) })
}

/// \brief Tear down a BLE central connection.
pub fn disconnect(conn: u32) -> Result<(), BleError> {
    rc(unsafe { host_ble_disconnect(conn) })
}

fn rc(code: c_int) -> Result<(), BleError> {
    if code == 0 {
        Ok(())
    } else {
        Err(BleError)
    }
}

#[link(wasm_import_module = "cdc")]
extern "C" {
    fn host_ble_is_enabled() -> bool;
    fn host_ble_mac(out: *mut u8) -> c_int;
    fn host_ble_device_name(out: *mut core::ffi::c_char, out_size: usize) -> c_int;
    fn host_ble_rssi() -> i8;
    fn host_ble_register_service(def: *const ServiceDef, service_handle_out: *mut u32) -> c_int;
    fn host_ble_send_notification(char_handle: u32, data: *const u8, len: usize) -> c_int;
    fn host_ble_send_indication(char_handle: u32, data: *const u8, len: usize) -> c_int;
    fn host_ble_unregister_service(service_handle: u32) -> c_int;
    fn host_ble_scan_start() -> c_int;
    fn host_ble_scan_results(out: *mut BleScanRaw, count: *mut usize) -> c_int;
    fn host_ble_connect(addr: *const u8) -> c_int;
    fn host_ble_read_char(conn: u32, uuid: *const u8, buf: *mut u8, len: *mut usize) -> c_int;
    fn host_ble_write_char(conn: u32, uuid: *const u8, data: *const u8, len: usize) -> c_int;
    fn host_ble_subscribe(conn: u32, uuid: *const u8, action_id: u32) -> c_int;
    fn host_ble_disconnect(conn: u32) -> c_int;
}
