//! \file
//! \brief EventBus subscription and publishing.
//!
//! Plugins subscribe to one or more event types from the bitmask constants
//! below. When a subscribed event fires, the host invokes the plugin's
//! `plugin_on_action(action_id, idx, user_data)` callback, where `idx` is the
//! event-type ordinal (the bit position of the matching constant: `KEY_PRESSED`
//! -> 0, `KEY_RELEASED` -> 1, ...) and `user_data` is the event payload. Note
//! the asymmetry: you subscribe with the mask bit (`1 << ordinal`) but receive
//! the bare ordinal in `idx`. For key events (`KEY_PRESSED` / `KEY_RELEASED` /
//! `KEY_LONG_PRESS`) the payload is the ASCII key code: `b'0'..=b'9'`,
//! `b'Y'` (89), `b'N'` (78).

use crate::{ffi, Error, Result};

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
pub const MODULE_EVENT: u32 = 1 << 15;
/// A FAST or FULL e-paper refresh is in progress: `plugin_on_action`'s
/// `user_data` is 1 at begin and 0 at end. Subscribe to pause animation/game
/// logic while the panel is unreadable.
pub const DISPLAY_REFRESH: u32 = 1 << 16;

/// \brief Subscribe to one or more event types.
/// \param event_mask Bitwise-OR of the event constants in this module.
/// \param action_id  Action id passed back via `plugin_on_action` when an
///                   event fires.
/// \return Subscription id used with [`unsubscribe`], or `Err` on error.
pub fn subscribe(event_mask: u32, action_id: u32) -> Result<u32> {
    let rc = unsafe { ffi::host_event_subscribe(event_mask, action_id) };
    if rc >= 0 {
        Ok(rc as u32)
    } else {
        Err(Error::from_code(rc))
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
