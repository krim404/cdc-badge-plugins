//! \file
//! \brief Translation lookup for plugin-declared and core i18n strings.
//!
//! Strings declared in the manifest's `i18n` block are registered
//! automatically by the host at plugin load and can be looked up here by
//! their key. The host writes the resolved string into a caller-provided
//! buffer; the returned `&'static str` is owned by the plugin runtime and
//! valid for the lifetime of the plugin instance.

use crate::ffi;
use alloc::boxed::Box;
use alloc::ffi::CString;
use alloc::string::String;
use core::ffi::c_char;

/// \brief Languages the host can report back as the current selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    En,
    De,
    Other(u8),
}

/// \brief Read the currently active UI language.
/// \return The host's current language, wrapped as a [`Language`] variant.
pub fn current_language() -> Language {
    match unsafe { ffi::host_i18n_current_language() } {
        0 => Language::En,
        1 => Language::De,
        n => Language::Other(n),
    }
}

const SCRATCH_CAP: usize = 128;

fn lookup(
    host_fn: unsafe extern "C" fn(*const c_char, *mut c_char, u32) -> core::ffi::c_int,
    key: &str,
) -> &'static str {
    let Ok(k) = CString::new(key) else {
        return "";
    };
    let mut buf = [0u8; SCRATCH_CAP];
    let n = unsafe {
        host_fn(
            k.as_ptr(),
            buf.as_mut_ptr() as *mut c_char,
            buf.len() as u32,
        )
    };
    if n <= 0 {
        return "";
    }
    let n = (n as usize).min(buf.len() - 1);
    let owned: String = String::from_utf8_lossy(&buf[..n]).into_owned();
    Box::leak(owned.into_boxed_str())
}

/// \brief Look up a string from the plugin's i18n table.
/// \param key Manifest-declared key (e.g. `"save"`).
/// \return The localized string, or `""` if the key is unknown.
pub fn tr_key(key: &str) -> &'static str {
    lookup(ffi::host_i18n_tr_key, key)
}

/// \brief Look up a manifest metadata field (e.g. `name`, `description`).
/// \param field Metadata field name from `i18n.meta.*`.
/// \return The localized string, or `""` if the field is unknown.
pub fn tr_meta(field: &str) -> &'static str {
    lookup(ffi::host_i18n_tr_meta, field)
}

/// \brief Look up a core OS i18n string by its host key.
/// \param key Core string key.
/// \return The localized string, or `""` if the key is unknown.
pub fn tr_core(key: &str) -> &'static str {
    lookup(ffi::host_i18n_tr_core, key)
}
