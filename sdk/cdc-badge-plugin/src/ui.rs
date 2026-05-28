//! \file
//! \brief Plugin-side UI view helpers.
//!
//! All push helpers return immediately; the actual view is rendered by the
//! host on the next frame. Views that produce user input (lists, confirms,
//! T9, slider) fire the plugin's `plugin_on_action` callback when the user
//! confirms or cancels. The accompanying payload is then read via
//! `consume_input_int` or `consume_input_text`.

use crate::ffi::{self, UiItem};
use alloc::ffi::CString;
use alloc::vec;
use alloc::vec::Vec;

/// \brief Target codepage for `to_display_with()`. Mirrors HOST_STR_TARGET_*.
pub use ffi::{HOST_STR_TARGET_CP437, HOST_STR_TARGET_LATIN1};

/// \brief Convert a UTF-8/HTML web string into single-byte display
///        characters in the codepage of the \p target font.
///
/// Decodes HTML named/numeric entities and collapses UTF-8 multibyte
/// sequences into the single-byte layout the chosen font expects.
/// Unknown codepoints are dropped. The returned buffer has no trailing
/// NUL; pass it (or its `.as_slice()`) to any push helper that accepts
/// `impl AsRef<[u8]>`.
/// \param input  Source string from the web (RSS, JSON, HA API, ...).
/// \param target One of `HOST_STR_TARGET_CP437` (default for the
///               GFX builtin glcdfont) or `HOST_STR_TARGET_LATIN1`
///               (FreeMonoBold*pt8b fonts).
pub fn to_display_with(input: &str, target: u32) -> Vec<u8> {
    let in_c = match CString::new(input.as_bytes()) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    let cap = input.len() + 32;
    let mut buf: Vec<u8> = vec![0u8; cap];
    let rc = unsafe {
        ffi::host_str_to_display(
            in_c.as_ptr(),
            buf.as_mut_ptr() as *mut _,
            cap,
            target,
        )
    };
    if rc != 0 {
        return Vec::new();
    }
    let nul = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    buf.truncate(nul);
    buf
}

/// \brief Convenience wrapper: `to_display_with(input, HOST_STR_TARGET_CP437)`.
///
/// Use this whenever you hand text to a plugin push helper - those views
/// render with the GFX builtin glcdfont after the splash so CP437 is the
/// right codepage. For the FreeMonoBold*pt8b fonts call
/// `to_display_with(s, HOST_STR_TARGET_LATIN1)` instead.
pub fn to_display(input: &str) -> Vec<u8> {
    to_display_with(input, HOST_STR_TARGET_CP437)
}

fn to_cstring<L: AsRef<[u8]>>(label: L) -> Option<CString> {
    CString::new(label.as_ref().to_vec()).ok()
}

pub use ffi::{
    UI_ICON_ALERT, UI_ICON_ANGLE, UI_ICON_ARROW_DOWN, UI_ICON_ARROW_LEFT,
    UI_ICON_ARROW_RIGHT, UI_ICON_ARROW_UP, UI_ICON_BACK, UI_ICON_BAR, UI_ICON_BULLET,
    UI_ICON_CIRCLE, UI_ICON_CLUB, UI_ICON_COVER, UI_ICON_DIAMOND, UI_ICON_ERROR,
    UI_ICON_FEMALE, UI_ICON_HEART, UI_ICON_INFO, UI_ICON_INVERSE_BULLET,
    UI_ICON_INVERSE_CIRCLE, UI_ICON_LEFTRIGHT, UI_ICON_LIGHT, UI_ICON_MALE,
    UI_ICON_MUSIC, UI_ICON_NONE, UI_ICON_NOTES, UI_ICON_PARAGRAPH, UI_ICON_PLAY,
    UI_ICON_REMOVE, UI_ICON_REVERSE_PLAY, UI_ICON_SCENE, UI_ICON_SECTION,
    UI_ICON_SENSOR, UI_ICON_SPADE, UI_ICON_SUCCESS, UI_ICON_SUN, UI_ICON_SWITCH,
    UI_ICON_TASK, UI_ICON_TRIANGLE_DOWN, UI_ICON_TRIANGLE_UP, UI_ICON_UPDOWN,
    UI_ICON_UPDOWN_BAR,
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
/// Fires `action_id` in `plugin_on_action` with `idx=1` on confirm,
/// `idx=0` on cancel.
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
                self.items.as_ptr(),
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
                self.items.as_ptr(),
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
                self.items.as_ptr(),
                self.items.len() as u16,
                self.select_action,
            );
        }
    }
}

/// \brief Pop views until the plugin's own root view is on top.
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

    /// \brief Set the action fired when the user confirms the value.
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
/// returns the same packed integer. On N the view pops silently.
pub fn push_color_picker(r: u8, g: u8, b: u8, action_id: u32) {
    unsafe {
        ffi::host_ui_push_color_picker(r, g, b, action_id);
    }
}

/// \brief Read the integer payload of the most recent slider / number
///        entry view that produced a confirmation event.
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
    host_fn: unsafe extern "C" fn(*const core::ffi::c_char, *const core::ffi::c_char, u16, u32) -> core::ffi::c_int,
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
/// Fires `action_id` with `idx=1` on confirm, `idx=0` on cancel. Read the
/// entered text with `consume_input_text`.
/// \param title     Input header text.
/// \param initial   Optional pre-filled buffer; `None` starts empty.
/// \param max_len   Maximum number of characters the user may enter.
/// \param action_id Action id echoed back to the plugin.
pub fn push_t9_input(title: &str, initial: Option<&str>, max_len: u16, action_id: u32) {
    push_text_input(ffi::host_ui_push_t9_input, title, initial, max_len, action_id);
}

/// \brief Push a password input view (masked T9).
///
/// `initial` is rarely useful here; pass `None` unless you have a concrete
/// edit-existing flow.
/// \param title     Input header text.
/// \param initial   Optional pre-filled buffer; `None` starts empty.
/// \param max_len   Maximum number of characters the user may enter.
/// \param action_id Action id echoed back to the plugin.
pub fn push_password(title: &str, initial: Option<&str>, max_len: u16, action_id: u32) {
    push_text_input(ffi::host_ui_push_password, title, initial, max_len, action_id);
}

/// \brief Read the text payload of the most recent T9 / password view that
///        produced a confirmation event.
/// \param max_len Buffer capacity; allocates `max_len + 1` bytes.
/// \return The entered text, or `None` if there is no pending input.
pub fn consume_input_text(max_len: usize) -> Option<alloc::string::String> {
    let cap = max_len.saturating_add(1);
    let mut buf = Vec::<u8>::with_capacity(cap);
    let rc = unsafe {
        buf.set_len(cap);
        ffi::host_ui_consume_input_text(buf.as_mut_ptr() as *mut core::ffi::c_char, cap)
    };
    if rc < 0 {
        return None;
    }
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    buf.truncate(end);
    alloc::string::String::from_utf8(buf).ok()
}
