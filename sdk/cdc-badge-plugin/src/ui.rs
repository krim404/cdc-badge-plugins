//! \file
//! \brief Plugin-side UI view helpers.
//!
//! All push helpers return immediately; the actual view is rendered by the
//! host on the next frame. Views that produce user input (lists, confirms,
//! T9, slider) fire the plugin's `plugin_on_action` callback when the user
//! confirms or cancels. The accompanying payload is then read via
//! `consume_input_int` or `consume_input_text`.

use crate::ffi::{self, UiItem};
use crate::{check, Result};
use alloc::ffi::CString;
use alloc::vec;
use alloc::vec::Vec;

/// \brief Target codepage for `to_display_with()`. Mirrors HOST_STR_TARGET_*.
pub use ffi::{HOST_STR_TARGET_CP437, HOST_STR_TARGET_LATIN1};

/// \brief Convert a UTF-8/HTML web string into single-byte display
///        characters in the codepage of the \p target font.
///
/// No longer needed in the normal flow: every UI / canvas / display function
/// takes UTF-8 and converts internally. Pass `&str` straight to them. Kept for
/// advanced cases; do not feed its output back into those functions or the
/// text is encoded twice.
/// \param input  Source string from the web (RSS, JSON, HA API, ...).
/// \param target One of `HOST_STR_TARGET_CP437` or `HOST_STR_TARGET_LATIN1`.
pub fn to_display_with(input: &str, target: u32) -> Vec<u8> {
    let in_c = match CString::new(input.as_bytes()) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    let cap = input.len() + 32;
    let mut buf: Vec<u8> = vec![0u8; cap];
    let rc =
        unsafe { ffi::host_str_to_display(in_c.as_ptr(), buf.as_mut_ptr() as *mut _, cap, target) };
    if rc != 0 {
        return Vec::new();
    }
    let nul = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    buf.truncate(nul);
    buf
}

/// \brief Convenience wrapper: `to_display_with(input, HOST_STR_TARGET_CP437)`.
///
/// No longer needed: hand `&str` directly to the push helpers, which convert
/// UTF-8 internally.
pub fn to_display(input: &str) -> Vec<u8> {
    to_display_with(input, HOST_STR_TARGET_CP437)
}

/// \brief Convert CP437 display bytes back to a UTF-8 String.
///
/// No longer needed: `consume_input_text` and `canvas::get_text` already
/// return UTF-8.
pub fn from_display(input: &[u8]) -> alloc::string::String {
    let in_c = match CString::new(input) {
        Ok(c) => c,
        Err(_) => return alloc::string::String::new(),
    };
    let cap = input.len() * 3 + 4;
    let mut buf: Vec<u8> = vec![0u8; cap];
    let rc = unsafe { ffi::host_str_to_utf8(in_c.as_ptr(), buf.as_mut_ptr() as *mut _, cap) };
    if rc < 0 {
        return alloc::string::String::new();
    }
    let nul = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    buf.truncate(nul);
    alloc::string::String::from_utf8(buf).unwrap_or_default()
}

fn to_cstring<L: AsRef<[u8]>>(label: L) -> Option<CString> {
    CString::new(label.as_ref().to_vec()).ok()
}

pub use ffi::{
    UI_ICON_ALERT, UI_ICON_ANGLE, UI_ICON_ARROW_DOWN, UI_ICON_ARROW_LEFT, UI_ICON_ARROW_RIGHT,
    UI_ICON_ARROW_UP, UI_ICON_BACK, UI_ICON_BAR, UI_ICON_BULLET, UI_ICON_CIRCLE, UI_ICON_CLUB,
    UI_ICON_COVER, UI_ICON_DIAMOND, UI_ICON_ERROR, UI_ICON_FEMALE, UI_ICON_HEART, UI_ICON_INFO,
    UI_ICON_INVERSE_BULLET, UI_ICON_INVERSE_CIRCLE, UI_ICON_LEFTRIGHT, UI_ICON_LIGHT, UI_ICON_MALE,
    UI_ICON_MUSIC, UI_ICON_NONE, UI_ICON_NOTES, UI_ICON_PARAGRAPH, UI_ICON_PLAY, UI_ICON_REMOVE,
    UI_ICON_REVERSE_PLAY, UI_ICON_SCENE, UI_ICON_SECTION, UI_ICON_SENSOR, UI_ICON_SPADE,
    UI_ICON_SUCCESS, UI_ICON_SUN, UI_ICON_SWITCH, UI_ICON_TASK, UI_ICON_TRIANGLE_DOWN,
    UI_ICON_TRIANGLE_UP, UI_ICON_UPDOWN, UI_ICON_UPDOWN_BAR,
};

/// \brief Show a short auto-dismissing toast over the current view.
/// \param text     Toast body text.
/// \param icon     One of the `UI_ICON_*` constants.
/// \param duration_ms How long the toast stays on screen.
pub fn push_toast<T: AsRef<[u8]>>(text: T, icon: u8, duration_ms: u16) {
    if let Some(c) = to_cstring(text) {
        unsafe {
            ffi::host_ui_push_toast(c.as_ptr(), icon, duration_ms);
        }
    }
}

/// \brief Show a longer auto-dismissing message view.
/// \param text     Message body, may contain newlines.
/// \param icon     One of the `UI_ICON_*` constants.
/// \param duration_ms How long the view stays on screen.
pub fn push_message<T: AsRef<[u8]>>(text: T, icon: u8, duration_ms: u32) {
    if let Some(c) = to_cstring(text) {
        unsafe {
            ffi::host_ui_push_message(c.as_ptr(), icon, duration_ms);
        }
    }
}

/// \brief Show a Y/N confirmation dialog.
///
/// Fires `action_id` in `plugin_on_action` with `user_data=1` on confirm (Y),
/// `user_data=0` on cancel (N).
/// \param text     Prompt text.
/// \param icon     One of the `UI_ICON_*` constants.
/// \param action_id Action id echoed back to the plugin on user response.
pub fn push_confirm<T: AsRef<[u8]>>(text: T, icon: u8, action_id: u32) {
    if let Some(c) = to_cstring(text) {
        unsafe {
            ffi::host_ui_push_confirm(c.as_ptr(), icon, action_id);
        }
    }
}

/// \brief Show a modal info screen with title and multi-line body.
/// \param title    Header text shown at the top of the view.
/// \param body     Body text, may contain newlines.
pub fn push_info<T: AsRef<[u8]>, B: AsRef<[u8]>>(title: T, body: B) {
    let t = match to_cstring(title) {
        Some(c) => c,
        None => return,
    };
    let b = match to_cstring(body) {
        Some(c) => c,
        None => return,
    };
    unsafe {
        ffi::host_ui_push_info(t.as_ptr(), b.as_ptr());
    }
}

/// \brief Builder for a scrollable list view.
///
/// Items are added with `item()` and the list is pushed with `push()`.
/// At least one of `on_select` or `on_menu` must be configured for the
/// list to react to user input.
pub struct ListBuilder {
    title: CString,
    labels: Vec<CString>,
    items: Vec<UiItem>,
    select_action: u32,
    menu_action: u32,
}

impl ListBuilder {
    /// \brief Start a new list with the given title.
    /// \param title List header text.
    pub fn new<T: AsRef<[u8]>>(title: T) -> Self {
        Self {
            title: to_cstring(title).unwrap_or_default(),
            labels: Vec::new(),
            items: Vec::new(),
            select_action: 0,
            menu_action: 0,
        }
    }

    /// \brief Set the action fired when the user picks an item with Y.
    ///
    /// In `plugin_on_action` the `idx` argument carries the picked item's
    /// list index.
    /// \param action_id Action id echoed back to the plugin.
    pub fn on_select(mut self, action_id: u32) -> Self {
        self.select_action = action_id;
        self
    }

    /// \brief Set the action fired when the user presses the menu key on
    ///        the list.
    /// \param action_id Action id echoed back to the plugin.
    pub fn on_menu(mut self, action_id: u32) -> Self {
        self.menu_action = action_id;
        self
    }

    /// \brief Append an item to the list.
    /// \param label   Display label.
    /// \param item_id Plugin-defined id echoed back via `_user_data`.
    /// \param icon    One of the `UI_ICON_*` constants.
    pub fn item<L: AsRef<[u8]>>(mut self, label: L, item_id: u32, icon: u8) -> Self {
        if let Some(c) = to_cstring(label) {
            self.labels.push(c);
            let label_ptr = self.labels.last().unwrap().as_ptr();
            self.items.push(UiItem {
                label: label_ptr,
                icon,
                icon_disabled: false,
                item_id,
            });
        }
        self
    }

    /// \brief Hand the list to the host. Consumes the builder.
    pub fn push(self) {
        unsafe {
            ffi::host_ui_push_list(
                self.title.as_ptr(),
                crate::slice_ptr(&self.items),
                self.items.len() as u16,
                self.select_action,
                self.menu_action,
            );
        }
    }

    /// \brief Replace the plugin's currently-top list (if any) with this one.
    ///
    /// Use this for the "refresh after an action" pattern - the view stack
    /// stays at the same depth instead of growing on every redraw. Falls
    /// back to a plain push when there is no plugin list on top yet.
    pub fn replace(self) {
        unsafe {
            ffi::host_ui_replace_list(
                self.title.as_ptr(),
                crate::slice_ptr(&self.items),
                self.items.len() as u16,
                self.select_action,
                self.menu_action,
            );
        }
    }
}

/// \brief Pop the top-most view from the badge's view stack.
pub fn pop() {
    unsafe {
        ffi::host_ui_pop();
    }
}

/// \brief Override the footer hint of the currently-top view.
///
/// Works for any plugin-pushed view (list, confirm, t9, pin, slider, date,
/// time). Pass an empty string to fall back to the view's default hint.
pub fn set_footer<T: AsRef<[u8]>>(text: T) {
    if let Some(c) = to_cstring(text) {
        unsafe {
            ffi::host_ui_set_view_footer(c.as_ptr());
        }
    }
}

/// \brief Set the placeholder text shown in a plugin-pushed list when it
///        is empty. Pass an empty string to clear the override.
pub fn set_list_empty<T: AsRef<[u8]>>(text: T) {
    if let Some(c) = to_cstring(text) {
        unsafe {
            ffi::host_ui_set_view_empty(c.as_ptr());
        }
    }
}

/// \brief Register hide/show callbacks on the plugin's current top view.
///
/// `hide_action_id` fires (via `on_action`) when the view is covered by another
/// view or modal; `show_action_id` fires when it becomes visible again. Pass 0
/// for either id to leave that event unhooked. Lets a plugin pause/resume work
/// (scans, timers, sensors) while its view is not visible.
pub fn set_view_lifecycle(hide_action_id: u32, show_action_id: u32) {
    unsafe {
        ffi::host_ui_set_view_lifecycle(hide_action_id, show_action_id);
    }
}

/// \brief Update a single row of the plugin's current top list in place.
///
/// Redraws only the given row (partial refresh) instead of re-pushing the
/// whole list, so a list can be refreshed cell-by-cell without flicker or
/// growing the view stack. `index` is the row position; `label`, `item_id`
/// and `icon` replace the row's values. The host copies the label, so the
/// borrow only needs to live for the call. No-op when the plugin's list is
/// not the active top view or the row is off screen.
/// \param index   Row index in the list.
/// \param label   New display label.
/// \param item_id New plugin-defined id echoed back via `_user_data`.
/// \param icon    One of the `UI_ICON_*` constants.
pub fn update_list_item<L: AsRef<[u8]>>(index: u16, label: L, item_id: u32, icon: u8) {
    if let Some(c) = to_cstring(label) {
        let item = UiItem {
            label: c.as_ptr(),
            icon,
            icon_disabled: false,
            item_id,
        };
        unsafe {
            ffi::host_ui_update_list_item(index, &item);
        }
    }
}

/// \brief Insert a new row into the plugin's current top list at `index`.
///
/// Later rows shift down; the list is rebuilt host-side and partial-repainted
/// (no full re-push). `index` is clamped to the current count (append). The
/// host copies the label, so the borrow only needs to live for the call.
/// \param index   Position to insert at.
/// \param label   Display label.
/// \param item_id Plugin-defined id echoed back via `_user_data`.
/// \param icon    One of the `UI_ICON_*` constants.
pub fn insert_list_item<L: AsRef<[u8]>>(index: u16, label: L, item_id: u32, icon: u8) {
    if let Some(c) = to_cstring(label) {
        let item = UiItem {
            label: c.as_ptr(),
            icon,
            icon_disabled: false,
            item_id,
        };
        unsafe {
            ffi::host_ui_insert_list_item(index, &item);
        }
    }
}

/// \brief Remove the row at `index` from the plugin's current top list.
///
/// Later rows shift up; the list is rebuilt host-side and partial-repainted.
/// \param index Index of the row to remove.
pub fn remove_list_item(index: u16) {
    unsafe {
        ffi::host_ui_remove_list_item(index);
    }
}

/// \brief Builder for a modal context-menu overlay (popup), invoked with [3].
///
/// Use `push()` to show the overlay. On select the configured `action_id`
/// fires with `idx = item position`, `user_data = item_id`. On cancel the
/// host pops the menu automatically (no event).
pub struct ContextMenuBuilder {
    title: CString,
    labels: Vec<CString>,
    items: Vec<UiItem>,
    select_action: u32,
}

impl ContextMenuBuilder {
    pub fn new(title: &str) -> Self {
        Self {
            title: CString::new(title).unwrap_or_default(),
            labels: Vec::new(),
            items: Vec::new(),
            select_action: 0,
        }
    }

    pub fn on_select(mut self, action_id: u32) -> Self {
        self.select_action = action_id;
        self
    }

    pub fn item<L: AsRef<[u8]>>(mut self, label: L, item_id: u32, icon: u8) -> Self {
        if let Some(c) = to_cstring(label) {
            self.labels.push(c);
            let label_ptr = self.labels.last().unwrap().as_ptr();
            self.items.push(UiItem {
                label: label_ptr,
                icon,
                icon_disabled: false,
                item_id,
            });
        }
        self
    }

    pub fn push(self) {
        unsafe {
            ffi::host_ui_push_context_menu(
                self.title.as_ptr(),
                crate::slice_ptr(&self.items),
                self.items.len() as u16,
                self.select_action,
            );
        }
    }
}

/// \brief Pop back to the plugin's first view.
///
/// Pops every view the plugin pushed during or after `plugin_on_enter`, back
/// to its first view (the stack depth recorded at entry). Views below the
/// plugin are untouched. No-op if the plugin pushed nothing.
pub fn pop_to_plugin() {
    unsafe {
        ffi::host_ui_pop_to_plugin();
    }
}

/// \brief Force a redraw of the current view.
pub fn repaint() {
    unsafe {
        ffi::host_ui_repaint();
    }
}

/// \brief Builder for an integer slider view.
///
/// Push with `push()`; read the selected value in `plugin_on_action` via
/// `consume_input_int`.
pub struct SliderBuilder {
    title: CString,
    unit: CString,
    min: i32,
    max: i32,
    initial: i32,
    step: i32,
    action: u32,
}

impl SliderBuilder {
    /// \brief Start a slider with the given title and the defaults
    ///        `[0..100]`, initial `0`, step `1`, no unit.
    /// \param title Slider header text.
    pub fn new(title: &str) -> Self {
        Self {
            title: CString::new(title).unwrap_or_default(),
            unit: CString::new("").unwrap(),
            min: 0,
            max: 100,
            initial: 0,
            step: 1,
            action: 0,
        }
    }

    /// \brief Set the inclusive `[min..max]` range.
    /// \param min Lower bound.
    /// \param max Upper bound.
    pub fn range(mut self, min: i32, max: i32) -> Self {
        self.min = min;
        self.max = max;
        self
    }

    /// \brief Set the pre-selected value.
    /// \param value Must be inside the configured range.
    pub fn initial(mut self, value: i32) -> Self {
        self.initial = value;
        self
    }

    /// \brief Set the step size; values below 1 are clamped to 1.
    /// \param step Increment per knob tick.
    pub fn step(mut self, step: i32) -> Self {
        self.step = step.max(1);
        self
    }

    /// \brief Set the unit suffix shown next to the value.
    /// \param unit Unit string (e.g. `"%"`, `"min"`).
    pub fn unit(mut self, unit: &str) -> Self {
        self.unit = CString::new(unit).unwrap_or_default();
        self
    }

    /// \brief Set the action fired when the slider closes.
    ///
    /// Fires on confirm (`user_data = 1`; read the value via
    /// [`consume_input_int`]) and on cancel (`user_data = 0`, nothing pending).
    /// \param action_id Action id echoed back to the plugin.
    pub fn on_save(mut self, action_id: u32) -> Self {
        self.action = action_id;
        self
    }

    /// \brief Hand the slider view to the host. Consumes the builder.
    pub fn push(self) {
        unsafe {
            ffi::host_ui_push_slider(
                self.title.as_ptr(),
                self.min,
                self.max,
                self.initial,
                self.step,
                self.unit.as_ptr(),
                self.action,
            );
        }
    }
}

/// \brief Push the global RGB color picker.
///
/// Fires `action_id` on Y with `idx = packed RGB (0xRRGGBB)` and
/// `user_data = 1`. Read the value via [`consume_input_int`] which
/// returns the same packed integer. On N (cancel) it fires with
/// `user_data = 0` and nothing pending. The view pops itself first.
pub fn push_color_picker(r: u8, g: u8, b: u8, action_id: u32) {
    unsafe {
        ffi::host_ui_push_color_picker(r, g, b, action_id);
    }
}

/// \brief Read the integer payload of the most recent slider / number
///        entry view that produced a confirmation event. A cancelled input
///        has no payload and returns `None` (the confirm handler tells the two
///        apart via `user_data`: 1 = confirm, 0 = cancel).
/// \return The entered value, or `None` if there is no pending input.
pub fn consume_input_int() -> Option<i32> {
    let mut out: i32 = 0;
    let rc = unsafe { ffi::host_ui_consume_input_int(&mut out) };
    if rc == ffi::HOST_OK {
        Some(out)
    } else {
        None
    }
}

fn push_text_input(
    host_fn: unsafe extern "C" fn(
        *const core::ffi::c_char,
        *const core::ffi::c_char,
        u16,
        u32,
    ) -> core::ffi::c_int,
    title: &str,
    initial: Option<&str>,
    max_len: u16,
    action_id: u32,
) {
    let t = match CString::new(title) {
        Ok(c) => c,
        Err(_) => return,
    };
    let i = match initial {
        Some(s) => match CString::new(s) {
            Ok(c) => Some(c),
            Err(_) => return,
        },
        None => None,
    };
    let initial_ptr = i.as_ref().map_or(core::ptr::null(), |c| c.as_ptr());
    unsafe {
        host_fn(t.as_ptr(), initial_ptr, max_len, action_id);
    }
}

/// \brief Push a T9 text input view.
///
/// Fires `action_id` on both outcomes; the view pops itself before it fires,
/// so do not call [`pop`] in the handler. On confirm: `user_data = 1`, `idx` =
/// entered text length, read the text with [`consume_input_text`]. On cancel
/// (back): `user_data = 0`, `idx = 0` and no text is pending, so
/// [`consume_input_text`] returns `None`.
/// \param title     Input header text.
/// \param initial   Optional pre-filled buffer; `None` starts empty.
/// \param max_len   Maximum number of characters the user may enter.
/// \param action_id Action id echoed back to the plugin.
pub fn push_t9_input(title: &str, initial: Option<&str>, max_len: u16, action_id: u32) {
    push_text_input(
        ffi::host_ui_push_t9_input,
        title,
        initial,
        max_len,
        action_id,
    );
}

/// \brief Push a password input view (masked T9).
///
/// Same contract as [`push_t9_input`]: confirm fires `action_id` with
/// `user_data = 1` and `idx` = the entered text length (read the text with
/// [`consume_input_text`]); cancel fires with `user_data = 0` and nothing
/// pending. The view pops itself first, so do not call [`pop`] in the handler.
/// `initial` is rarely useful here; pass `None` unless you have a concrete
/// edit-existing flow.
/// \param title     Input header text.
/// \param initial   Optional pre-filled buffer; `None` starts empty.
/// \param max_len   Maximum number of characters the user may enter.
/// \param action_id Action id echoed back to the plugin.
pub fn push_password(title: &str, initial: Option<&str>, max_len: u16, action_id: u32) {
    push_text_input(
        ffi::host_ui_push_password,
        title,
        initial,
        max_len,
        action_id,
    );
}

/// \brief Read the text payload of the most recent T9 / password view that
///        produced a confirmation event. A cancelled input has no payload, so
///        this returns `None` (a confirm handler distinguishes the two via the
///        `user_data` argument: 1 = confirm, 0 = cancel).
/// \param max_len Maximum number of characters to read. The buffer is sized
///                for the UTF-8 worst case, so non-ASCII entries are never
///                truncated mid-string.
/// \return The entered text, or `None` if there is no pending input.
pub fn consume_input_text(max_len: usize) -> Option<alloc::string::String> {
    let cap = max_len.saturating_mul(4).saturating_add(1);
    let mut buf = Vec::<u8>::with_capacity(cap);
    let rc = unsafe {
        buf.set_len(cap);
        ffi::host_ui_consume_input_text(buf.as_mut_ptr() as *mut core::ffi::c_char, cap)
    };
    // rc is the byte count; 0 means no input is pending (not an empty entry).
    if rc <= 0 {
        return None;
    }
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    buf.truncate(end);
    alloc::string::String::from_utf8(buf).ok()
}

/// \brief Push a date picker view.
///
/// Fires `action_id` on confirm (`user_data = 1`; read the packed value via
/// [`consume_input_int`], the host encodes the picked date) and on cancel
/// (`user_data = 0`, nothing pending). The view pops itself first.
/// \param title     View header text.
/// \param day       Initial day (1-31).
/// \param month     Initial month (1-12).
/// \param year      Initial year.
/// \param action_id Action id echoed back to the plugin.
pub fn push_date(title: &str, day: u8, month: u8, year: u16, action_id: u32) {
    let Ok(t) = CString::new(title) else { return };
    unsafe {
        ffi::host_ui_push_date(t.as_ptr(), day, month, year, action_id);
    }
}

/// \brief Push a time-of-day picker view.
///
/// Fires `action_id` on confirm (`user_data = 1`; read via [`consume_input_int`])
/// and on cancel (`user_data = 0`, nothing pending). The view pops itself first.
/// \param title     View header text.
/// \param hour      Initial hour (0-23).
/// \param minute    Initial minute (0-59).
/// \param action_id Action id echoed back to the plugin.
pub fn push_time(title: &str, hour: u8, minute: u8, action_id: u32) {
    let Ok(t) = CString::new(title) else { return };
    unsafe {
        ffi::host_ui_push_time(t.as_ptr(), hour, minute, action_id);
    }
}

/// \brief Push a numeric PIN entry view.
///
/// Fires `action_id` on confirm (`user_data = 1`, `idx` = PIN length) and on
/// cancel (`user_data = 0`). The view pops itself before the action fires.
/// \param title        View header text.
/// \param max_len      Number of PIN digits.
/// \param max_attempts Allowed attempts; `0` for unlimited.
/// \param action_id    Action id echoed back to the plugin.
pub fn push_pin_entry(title: &str, max_len: u8, max_attempts: u8, action_id: u32) {
    let Ok(t) = CString::new(title) else { return };
    unsafe {
        ffi::host_ui_push_pin_entry(t.as_ptr(), max_len, max_attempts, action_id);
    }
}

/// \brief Claim exclusive UI ownership, blocking other plugins from pushing views.
/// \return `Ok(())` on success, `Err` on failure.
pub fn acquire_exclusive() -> Result<()> {
    check(unsafe { ffi::host_ui_acquire_exclusive() })
}

/// \brief Release a previously acquired exclusive UI lock.
/// \return `Ok(())` on success, `Err` on failure.
pub fn release_exclusive() -> Result<()> {
    check(unsafe { ffi::host_ui_release_exclusive() })
}

/// \brief Arm an inactivity timer for the plugin's current view.
/// \param timeout_ms Idle time before firing.
/// \param action_id  Action fired when no input arrives within `timeout_ms`.
pub fn set_inactivity(timeout_ms: u32, action_id: u32) {
    unsafe {
        ffi::host_ui_set_inactivity(timeout_ms, action_id);
    }
}

/// \brief Blink the backlight as a visual identification signal.
/// \param count     On/off cycles, clamped by the host to 1..10 (`0` = default 2).
/// \param period_ms Duration of each phase, clamped to 50..1000 (`0` = default 150).
pub fn wink(count: u8, period_ms: u16) {
    unsafe {
        ffi::host_ui_wink(count, period_ms);
    }
}
