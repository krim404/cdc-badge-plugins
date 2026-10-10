//! \file
//! \brief In-memory stand-ins for the `host_nvs_*`, `host_rmem_*` and
//!        `host_random` imports, for the native `store` tests only.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::CStr;
use std::os::raw::{c_char, c_int};

const ERR_GENERIC: c_int = -1;
const ERR_NOT_FOUND: c_int = -4;
/// Payload bytes one fake rmem slot holds.
const RMEM_SLOT_LEN: usize = 1024;

thread_local! {
    static NVS: RefCell<HashMap<String, Vec<u8>>> = RefCell::new(HashMap::new());
    static RMEM: RefCell<HashMap<String, Vec<u8>>> = RefCell::new(HashMap::new());
}

pub fn reset() {
    NVS.with(|n| n.borrow_mut().clear());
    RMEM.with(|r| r.borrow_mut().clear());
}

pub fn nvs_has(key: &str) -> bool {
    NVS.with(|n| n.borrow().contains_key(key))
}

pub fn rmem_set(name: &str, data: &[u8]) {
    RMEM.with(|r| r.borrow_mut().insert(name.to_string(), data.to_vec()));
}

unsafe fn key(p: *const c_char) -> String {
    CStr::from_ptr(p).to_string_lossy().into_owned()
}

unsafe fn copy_out(src: &[u8], out: *mut u8, cap: usize) -> c_int {
    if src.len() > cap {
        return ERR_GENERIC;
    }
    core::ptr::copy_nonoverlapping(src.as_ptr(), out, src.len());
    src.len() as c_int
}

#[no_mangle]
extern "C" fn host_nvs_get_blob(k: *const c_char, buf: *mut u8, buf_size: usize) -> c_int {
    let k = unsafe { key(k) };
    NVS.with(|n| match n.borrow().get(&k) {
        Some(v) => unsafe { copy_out(v, buf, buf_size) },
        None => ERR_NOT_FOUND,
    })
}

#[no_mangle]
extern "C" fn host_nvs_set_blob(k: *const c_char, buf: *const u8, len: usize) -> c_int {
    let k = unsafe { key(k) };
    let v = unsafe { core::slice::from_raw_parts(buf, len) }.to_vec();
    NVS.with(|n| n.borrow_mut().insert(k, v));
    0
}

#[no_mangle]
extern "C" fn host_nvs_get_str(k: *const c_char, buf: *mut c_char, buf_size: usize) -> c_int {
    let k = unsafe { key(k) };
    NVS.with(|n| match n.borrow().get(&k) {
        Some(v) if v.len() < buf_size => unsafe {
            core::ptr::copy_nonoverlapping(v.as_ptr(), buf as *mut u8, v.len());
            *buf.add(v.len()) = 0;
            0
        },
        Some(_) => ERR_GENERIC,
        None => ERR_NOT_FOUND,
    })
}

#[no_mangle]
extern "C" fn host_nvs_set_str(k: *const c_char, value: *const c_char) -> c_int {
    let k = unsafe { key(k) };
    let v = unsafe { CStr::from_ptr(value) }.to_bytes().to_vec();
    NVS.with(|n| n.borrow_mut().insert(k, v));
    0
}

#[no_mangle]
extern "C" fn host_nvs_get_u32(k: *const c_char, out: *mut u32) -> c_int {
    let k = unsafe { key(k) };
    NVS.with(|n| match n.borrow().get(&k) {
        Some(v) if v.len() == 4 => {
            unsafe { *out = u32::from_le_bytes([v[0], v[1], v[2], v[3]]) };
            0
        }
        _ => ERR_NOT_FOUND,
    })
}

#[no_mangle]
extern "C" fn host_nvs_set_u32(k: *const c_char, value: u32) -> c_int {
    let k = unsafe { key(k) };
    NVS.with(|n| n.borrow_mut().insert(k, value.to_le_bytes().to_vec()));
    0
}

#[no_mangle]
extern "C" fn host_nvs_erase(k: *const c_char) -> c_int {
    let k = unsafe { key(k) };
    NVS.with(|n| n.borrow_mut().remove(&k));
    0
}

#[no_mangle]
extern "C" fn host_nvs_erase_all() -> c_int {
    NVS.with(|n| n.borrow_mut().clear());
    0
}

#[no_mangle]
extern "C" fn host_rmem_read_named(name: *const u8, buf: *mut u8, buf_size: i32) -> c_int {
    let k = unsafe { key(name as *const c_char) };
    RMEM.with(|r| match r.borrow().get(&k) {
        Some(v) => unsafe { copy_out(v, buf, buf_size as usize) },
        None => ERR_NOT_FOUND,
    })
}

#[no_mangle]
extern "C" fn host_rmem_write_named(name: *const u8, buf: *const u8, len: i32) -> c_int {
    if len as usize > RMEM_SLOT_LEN {
        return ERR_GENERIC;
    }
    let k = unsafe { key(name as *const c_char) };
    let v = unsafe { core::slice::from_raw_parts(buf, len as usize) }.to_vec();
    RMEM.with(|r| r.borrow_mut().insert(k, v));
    0
}

#[no_mangle]
extern "C" fn host_rmem_erase_named(name: *const u8) -> c_int {
    let k = unsafe { key(name as *const c_char) };
    RMEM.with(|r| r.borrow_mut().remove(&k));
    0
}

#[no_mangle]
extern "C" fn host_random(buf: *mut u8, len: usize) -> c_int {
    let out = unsafe { core::slice::from_raw_parts_mut(buf, len) };
    let mut x: u32 = 0x9e37_79b9;
    for b in out.iter_mut() {
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        *b = x as u8;
    }
    0
}

#[no_mangle]
extern "C" fn host_log(_level: u8, _tag: *const c_char, _msg: *const c_char) {}
