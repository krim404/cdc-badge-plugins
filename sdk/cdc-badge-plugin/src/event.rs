//! \file
//! \brief EventBus subscription and publishing.
//!
//! Plugins subscribe to one or more event types from the bitmask constants
//! below. When the event fires, the host invokes the plugin's
//! `plugin_on_action(action_id, ...)` callback.

use crate::ffi;

pub const KEY_PRESSED: u32 = 1 << 0;
pub const KEY_RELEASED: u32 = 1 << 1;
pub const KEY_LONG_PRESS: u32 = 1 << 2;
pub const POWER_USB_CONN: u32 = 1 << 3;
pub const POWER_USB_DISCONN: u32 = 1 << 4;
pub const POWER_CHARGING: u32 = 1 << 5;
pub const POWER_BATT_LOW: u32 = 1 << 6;
pub const POWER_BATT_CRIT: u32 = 1 << 7;
pub const SYSTEM_UNLOCK: u32 = 1 << 8;
pub const SYSTEM_LOCK: u32 = 1 << 9;
pub const SYSTEM_SLEEP: u32 = 1 << 10;
pub const SYSTEM_WAKE: u32 = 1 << 11;
pub const BLE_CONNECTED: u32 = 1 << 12;
pub const BLE_DISCONNECTED: u32 = 1 << 13;
pub const TIMER_TICK: u32 = 1 << 14;
pub const LANGUAGE_CHANGED: u32 = 1 << 15;
pub const MODULE_EVENT: u32 = 1 << 16;

/// \brief Subscribe to one or more event types.
/// \param event_mask Bitwise-OR of the event constants in this module.
/// \param action_id  Action id passed back via `plugin_on_action` when an
///                   event fires.
/// \return Subscription id used with [`unsubscribe`], or `None` on error.
pub fn subscribe(event_mask: u32, action_id: u32) -> Option<u32> {
    let rc = unsafe { ffi::host_event_subscribe(event_mask, action_id) };
    if rc >= 0 {
        Some(rc as u32)
    } else {
        None
    }
}

/// \brief Drop a previous subscription.
/// \param subscription_id Id returned by [`subscribe`].
pub fn unsubscribe(subscription_id: u32) {
    unsafe {
        ffi::host_event_unsubscribe(subscription_id);
    }
}

/// \brief Publish a module event to other plugins / native modules.
/// \param subtype Plugin-defined event subtype.
/// \param value   Plugin-defined payload value.
pub fn publish_module_event(subtype: u32, value: u32) {
    unsafe {
        ffi::host_event_publish(subtype, value);
    }
}
