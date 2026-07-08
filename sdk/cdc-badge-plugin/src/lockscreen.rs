//! \file
//! \brief Lockscreen quick-action slot for background plugins.
//!
//! When the user opens the lockscreen context menu (KEY_BACK) and selects
//! the registered item, the plugin's `plugin_on_action(action_id, 0, 0)`
//! fires.

use crate::{check, Error, Result};
use alloc::ffi::CString;
use core::ffi::{c_char, c_int};

#[link(wasm_import_module = "cdc")]
extern "C" {
    fn host_lockscreen_register_action(label_key: *const c_char, action_id: u32) -> c_int;
    fn host_lockscreen_unregister_action() -> c_int;
    fn host_lockscreen_alert(text: *const c_char, icon: u8, action_id: u32) -> c_int;
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
    unsafe {
        host_lockscreen_unregister_action();
    }
}

/// \brief Raise a persistent Y/N alert over the current screen, lock screen
///        included, that stays until the user answers.
///
/// Works from a background plugin: the answer is routed back even while the
/// plugin's own view is not in front. On confirm the plugin's
/// `plugin_on_action(action_id, 1, 0)` fires; on cancel `(action_id, 0, 0)`.
/// Only one alert can be pending at a time.
/// \param text      Message to show (UTF-8; HTML entities are decoded).
/// \param icon      One of the `crate::ui::UI_ICON_*` glyph ids.
/// \param action_id Action id dispatched with the answer (`idx` = 1 yes / 0 no).
/// \return `Ok(())` on success, `Err` if the text cannot be encoded or the host
///         refused the call (e.g. another modal is already on screen).
pub fn alert(text: &str, icon: u8, action_id: u32) -> Result<()> {
    let t = CString::new(text).map_err(|_| Error::InvalidArg)?;
    check(unsafe { host_lockscreen_alert(t.as_ptr(), icon, action_id) })
}
