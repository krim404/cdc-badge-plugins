//! \file
//! \brief Bluetooth Low Energy peripheral (GATT server) and central operations.
//!
//! A plugin publishes one GATT service with characteristics as a peripheral, or
//! scans, connects and talks to a remote device as a central. All operations
//! require the `ble` manifest capability.
//!
//! The BLE stack delivers inbound events asynchronously: a write to one of the
//! plugin's characteristics, or a central read/notification/discovery result,
//! fires the `action_id` the plugin passed for that operation. The plugin's
//! `plugin_on_action` handler then pulls the payload with the matching
//! `consume_*` call. Reserved system service UUIDs (FIDO, HID, vCard, Nordic
//! UART, GPG and the Bluetooth SIG 16-bit range) are refused, and only one BLE
//! connection exists at a time (central and peripheral share it).

use crate::{check, Error, Result};
use alloc::string::String;
use alloc::vec::Vec;
use core::ffi::{c_char, c_int};

/// \brief GATT characteristic property flags (mirror `BLE_PROP_*`).
pub const PROP_READ: u8 = 0x02;
pub const PROP_WRITE_NO_RSP: u8 = 0x04;
pub const PROP_WRITE: u8 = 0x08;
pub const PROP_NOTIFY: u8 = 0x10;
pub const PROP_INDICATE: u8 = 0x20;

/// \brief One characteristic of a plugin GATT service (peripheral role).
#[repr(C)]
pub struct CharDef {
    pub uuid: [u8; 16],
    pub properties: u8,
    reserved: [u8; 3],
    pub write_action_id: u32,
    /// Assigned by the host on a successful [`register_service`].
    pub char_handle: u32,
}

impl CharDef {
    /// \brief Define a characteristic.
    /// \param uuid            128-bit UUID (little-endian byte order).
    /// \param properties      Bitmask of the `PROP_*` constants.
    /// \param write_action_id Action fired on each inbound write (0 = none).
    pub fn new(uuid: [u8; 16], properties: u8, write_action_id: u32) -> Self {
        Self { uuid, properties, reserved: [0; 3], write_action_id, char_handle: 0 }
    }
}

#[repr(C)]
struct ServiceDefRaw {
    uuid: [u8; 16],
    num_chars: u8,
    reserved: [u8; 3],
    service_handle: u32,
}

/// \brief A device discovered by a central scan.
#[derive(Debug, Clone)]
pub struct ScanResult {
    pub addr: [u8; 6],
    pub addr_type: u8,
    pub rssi: i8,
    pub name: String,
}

#[repr(C)]
struct ScanResultRaw {
    addr: [u8; 6],
    addr_type: u8,
    rssi: i8,
    name: [u8; 32],
}

/// \brief A characteristic discovered on a connected peer (central role).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct RemoteChar {
    pub uuid: [u8; 16],
    pub value_handle: u16,
    pub properties: u8,
    reserved: u8,
}

/* ----------------------------------------------------------------- state -- */

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
        host_ble_device_name(buf.as_mut_ptr() as *mut c_char, 64)
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

/* ------------------------------------------------------ peripheral (GATT) -- */

/// \brief Register the plugin's GATT service and its characteristics.
///
/// On success each `chars[i].char_handle` is filled in. The service UUID must
/// not be a reserved system UUID and the plugin service slot must be free.
/// \param uuid  128-bit primary service UUID (little-endian byte order).
/// \param chars Characteristics to expose (1-6).
/// \return The service handle (for [`unregister_service`]), or `Err`.
pub fn register_service(uuid: [u8; 16], chars: &mut [CharDef]) -> Result<u32> {
    if chars.is_empty() || chars.len() > 6 {
        return Err(Error::InvalidArg);
    }
    let mut def = ServiceDefRaw {
        uuid,
        num_chars: chars.len() as u8,
        reserved: [0; 3],
        service_handle: 0,
    };
    check(unsafe {
        host_ble_register_service(&mut def, chars.as_mut_ptr(), chars.len() as u32)
    })?;
    Ok(def.service_handle)
}

/// \brief Tear down the plugin's registered GATT service.
pub fn unregister_service(service_handle: u32) -> Result<()> {
    check(unsafe { host_ble_unregister_service(service_handle) })
}

/// \brief Notify subscribers of a value on a registered characteristic.
pub fn notify(char_handle: u32, data: &[u8]) -> Result<()> {
    check(unsafe { host_ble_send_notification(char_handle, data.as_ptr(), data.len()) })
}

/// \brief Indicate (acknowledged notify) a value on a registered characteristic.
pub fn indicate(char_handle: u32, data: &[u8]) -> Result<()> {
    check(unsafe { host_ble_send_indication(char_handle, data.as_ptr(), data.len()) })
}

/// \brief Pull the inbound write queued for `char_handle`.
///
/// Call from the characteristic's write action handler (the action fires with
/// `idx` = char handle, `user_data` = connection handle).
/// \return Number of bytes copied into `buf` (`0` when nothing is pending).
pub fn consume_write(char_handle: u32, buf: &mut [u8]) -> Result<usize> {
    let n = unsafe { host_ble_consume_write(char_handle, buf.as_mut_ptr(), buf.len()) };
    if n < 0 {
        Err(Error::from_code(n))
    } else {
        Ok(n as usize)
    }
}

/* ---------------------------------------------------------- central role -- */

/// \brief Start a central scan for `duration_ms` milliseconds.
pub fn scan_start(duration_ms: u32) -> Result<()> {
    check(unsafe { host_ble_scan_start(duration_ms) })
}

/// \brief Whether the scan started by [`scan_start`] has finished.
pub fn scan_done() -> bool {
    unsafe { host_ble_scan_done() }
}

/// \brief Read the results of the last central scan.
/// \param max Maximum number of devices to return.
pub fn scan_results(max: usize) -> Result<Vec<ScanResult>> {
    let mut raw = Vec::<ScanResultRaw>::with_capacity(max);
    let mut count = max;
    let rc = unsafe {
        raw.set_len(max);
        host_ble_scan_results(raw.as_mut_ptr(), &mut count)
    };
    if rc != 0 {
        return Err(Error::from_code(rc));
    }
    raw.truncate(count);
    Ok(raw
        .iter()
        .map(|d| {
            let end = d.name.iter().position(|&b| b == 0).unwrap_or(d.name.len());
            ScanResult {
                addr: d.addr,
                addr_type: d.addr_type,
                rssi: d.rssi,
                name: String::from_utf8_lossy(&d.name[..end]).into_owned(),
            }
        })
        .collect())
}

/// \brief Initiate a central connection. Completion arrives as a
///        `BLE_CONNECTED` event; read the handle with [`conn_handle`].
/// \param addr      Peer address (6 bytes).
/// \param addr_type 0 = public, 1 = random.
pub fn connect(addr: [u8; 6], addr_type: u8) -> Result<()> {
    check(unsafe { host_ble_connect(addr.as_ptr(), addr_type) })
}

/// \brief Current connection handle (central or peripheral), or `0` when idle.
pub fn conn_handle() -> u32 {
    unsafe { host_ble_conn_handle() }
}

/// \brief Disconnect a connection.
pub fn disconnect(conn: u32) -> Result<()> {
    check(unsafe { host_ble_disconnect(conn) })
}

/// \brief Discover the characteristics of one service on a connected peer.
///        Completion fires `action_id`; read them with [`consume_discovery`].
pub fn discover(conn: u32, uuid: [u8; 16], action_id: u32) -> Result<()> {
    check(unsafe { host_ble_discover(conn, uuid.as_ptr(), action_id) })
}

/// \brief Pull discovered characteristics after a discovery action fires.
/// \param max Maximum number of characteristics to return.
pub fn consume_discovery(max: usize) -> Result<Vec<RemoteChar>> {
    let mut raw = Vec::<RemoteChar>::with_capacity(max);
    let mut count = max;
    let rc = unsafe {
        raw.set_len(max);
        host_ble_consume_discovery(raw.as_mut_ptr(), &mut count)
    };
    if rc != 0 {
        return Err(Error::from_code(rc));
    }
    raw.truncate(count);
    Ok(raw)
}

/// \brief Start reading a peer characteristic by value handle. Completion fires
///        `action_id`; read the value with [`consume_read`].
pub fn read_char(conn: u32, value_handle: u16, action_id: u32) -> Result<()> {
    check(unsafe { host_ble_read_char(conn, value_handle, action_id) })
}

/// \brief Pull the value delivered by the last read action.
/// \return Number of bytes copied into `buf`.
pub fn consume_read(buf: &mut [u8]) -> Result<usize> {
    let n = unsafe { host_ble_consume_read(buf.as_mut_ptr(), buf.len()) };
    if n < 0 {
        Err(Error::from_code(n))
    } else {
        Ok(n as usize)
    }
}

/// \brief Write a value to a peer characteristic by value handle.
pub fn write_char(conn: u32, value_handle: u16, data: &[u8], with_response: bool) -> Result<()> {
    check(unsafe {
        host_ble_write_char(conn, value_handle, data.as_ptr(), data.len(), with_response as u8)
    })
}

/// \brief Subscribe to notifications on a peer characteristic (by CCCD handle).
///        Each notification fires `action_id`; read it with
///        [`consume_notification`].
pub fn subscribe(conn: u32, cccd_handle: u16, action_id: u32) -> Result<()> {
    check(unsafe { host_ble_subscribe(conn, cccd_handle, action_id) })
}

/// \brief Pull the next queued inbound notification.
/// \return `Some((value_handle, bytes_copied))`, or `None` when the queue is
///         empty.
pub fn consume_notification(buf: &mut [u8]) -> Result<Option<(u16, usize)>> {
    let mut vh: u16 = 0;
    let n = unsafe { host_ble_consume_notification(&mut vh, buf.as_mut_ptr(), buf.len()) };
    if n >= 0 {
        Ok(Some((vh, n as usize)))
    } else if n == Error::NotFound.code() {
        Ok(None)
    } else {
        Err(Error::from_code(n))
    }
}

#[link(wasm_import_module = "cdc")]
extern "C" {
    fn host_ble_is_enabled() -> bool;
    fn host_ble_mac(out: *mut u8) -> c_int;
    fn host_ble_device_name(out: *mut c_char, out_size: usize) -> c_int;
    fn host_ble_rssi() -> i8;

    fn host_ble_register_service(def: *mut ServiceDefRaw, chars: *mut CharDef, num: u32) -> c_int;
    fn host_ble_unregister_service(service_handle: u32) -> c_int;
    fn host_ble_send_notification(char_handle: u32, data: *const u8, len: usize) -> c_int;
    fn host_ble_send_indication(char_handle: u32, data: *const u8, len: usize) -> c_int;
    fn host_ble_consume_write(char_handle: u32, buf: *mut u8, buf_size: usize) -> c_int;

    fn host_ble_scan_start(duration_ms: u32) -> c_int;
    fn host_ble_scan_done() -> bool;
    fn host_ble_scan_results(out: *mut ScanResultRaw, count: *mut usize) -> c_int;
    fn host_ble_connect(addr: *const u8, addr_type: u8) -> c_int;
    fn host_ble_conn_handle() -> u32;
    fn host_ble_disconnect(conn: u32) -> c_int;
    fn host_ble_discover(conn: u32, uuid: *const u8, action_id: u32) -> c_int;
    fn host_ble_consume_discovery(out: *mut RemoteChar, count: *mut usize) -> c_int;
    fn host_ble_read_char(conn: u32, value_handle: u16, action_id: u32) -> c_int;
    fn host_ble_consume_read(buf: *mut u8, buf_size: usize) -> c_int;
    fn host_ble_write_char(conn: u32, value_handle: u16, data: *const u8, len: usize,
                           with_response: u8) -> c_int;
    fn host_ble_subscribe(conn: u32, cccd_handle: u16, action_id: u32) -> c_int;
    fn host_ble_consume_notification(value_handle_out: *mut u16, buf: *mut u8,
                                     buf_size: usize) -> c_int;
}
