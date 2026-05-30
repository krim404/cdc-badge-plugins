//! \file
//! \brief Lockscreen quick-action slot for background plugins.
//!
//! When the user opens the lockscreen context menu (KEY_BACK) and selects
//! the registered item, the plugin's `plugin_on_action(action_id, 0, 0)`
//! fires.

use crate::{check, Error, Result};
use core::ffi::{c_char, c_int};
use alloc::ffi::CString;

#[link(wasm_import_module = "cdc")]
extern "C" {
    fn host_lockscreen_register_action(label_key: *const c_char, action_id: u32) -> c_int;
    fn host_lockscreen_unregister_action() -> c_int;
}

/// \brief Register an i18n key as the lockscreen quick-action label.
/// \param label_key i18n string key resolved on each render.
/// \param action_id Action id dispatched when the user picks the item.
/// \return `Ok(())` on success, `Err` if the label key cannot be encoded or
///         the host refused the call.
pub fn register(label_key: &str, action_id: u32) -> Result<()> {
    let k = CString::new(label_key).map_err(|_| Error::InvalidArg)?;
    check(unsafe { host_lockscreen_register_action(k.as_ptr(), action_id) })
}

/// \brief Drop a previously registered lockscreen quick-action.
pub fn unregister() {
    unsafe { host_lockscreen_unregister_action(); }
}
