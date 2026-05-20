//! \file
//! \brief Plugin-drawn canvas view with inline widgets.
//!
//! Push a canvas, then draw into the body and add interactive widgets
//! (slider, T9 text input, button). The host owns input handling for
//! focused widgets; the plugin owns all rendering.
//!
//! Event flow:
//! - Every key press not consumed by a focused widget fires the canvas
//!   `key_action_id` with `idx = key_code`, `user_data = focused_widget`.
//! - Widget events (changed / committed / cancelled) fire
//!   `widget_action_id` with `idx = widget_id`, `user_data` one of
//!   [`WIDGET_CHANGED`], [`WIDGET_COMMITTED`], [`WIDGET_CANCELLED`].

use crate::ffi;
use alloc::ffi::CString;
use alloc::string::String;
use alloc::vec::Vec;
use core::ffi::c_char;

pub const WIDGET_CHANGED: u32 = 1;
pub const WIDGET_COMMITTED: u32 = 2;
pub const WIDGET_CANCELLED: u32 = 3;

pub const ALIGN_LEFT: u8 = 0;
pub const ALIGN_CENTER: u8 = 1;
pub const ALIGN_RIGHT: u8 = 2;

/// \brief Push a fresh canvas view onto the badge view stack.
///
/// \param title Header title (empty for no header bar).
/// \param key_action_id    Action id fired on plugin key events.
/// \param widget_action_id Action id fired on widget events.
pub fn push(title: &str, key_action_id: u32, widget_action_id: u32) {
    let c = CString::new(title).unwrap_or_default();
    unsafe {
        ffi::host_view_canvas_push(c.as_ptr(), key_action_id, widget_action_id);
    }
}

/// \brief Read the drawable body area in pixels.
pub fn body_size() -> (u16, u16) {
    let mut w: u16 = 0;
    let mut h: u16 = 0;
    unsafe {
        ffi::host_view_canvas_get_body_size(&mut w, &mut h);
    }
    (w, h)
}

/// \brief Set the footer hint of the canvas view.
pub fn set_footer(hint: &str) {
    if let Ok(c) = CString::new(hint) {
        unsafe {
            ffi::host_view_canvas_set_footer(c.as_ptr());
        }
    }
}

/// \brief Erase the body to white (queued until `commit`).
pub fn clear() {
    unsafe { ffi::host_view_canvas_clear() };
}

/// \brief Set the text size used by subsequent text draws (1..3).
pub fn set_text_size(size: u8) {
    unsafe { ffi::host_view_canvas_set_text_size(size) };
}

/// \brief Set text color (true = white text for use over black bg).
pub fn set_text_inverted(inverted: bool) {
    unsafe { ffi::host_view_canvas_set_text_color(inverted) };
}

/// \brief Draw text at (x, y).
pub fn draw_text(x: i16, y: i16, text: &str) {
    if let Ok(c) = CString::new(text) {
        unsafe {
            ffi::host_view_canvas_draw_text(x, y, c.as_ptr());
        }
    }
}

/// \brief Draw text aligned within (x, y, w).
pub fn draw_text_aligned(x: i16, y: i16, w: i16, text: &str, align: u8) {
    if let Ok(c) = CString::new(text) {
        unsafe {
            ffi::host_view_canvas_draw_text_aligned(x, y, w, c.as_ptr(), align);
        }
    }
}

/// \brief Draw a rectangle (filled = solid black, else outline).
pub fn draw_rect(x: i16, y: i16, w: i16, h: i16, filled: bool) {
    unsafe { ffi::host_view_canvas_draw_rect(x, y, w, h, filled) };
}

/// \brief Invert pixels inside a rectangle.
pub fn invert_rect(x: i16, y: i16, w: i16, h: i16) {
    unsafe { ffi::host_view_canvas_invert_rect(x, y, w, h) };
}

/// \brief Draw a horizontal line.
pub fn hline(x: i16, y: i16, w: i16) {
    unsafe { ffi::host_view_canvas_hline(x, y, w) };
}

/// \brief Draw a vertical line.
pub fn vline(x: i16, y: i16, h: i16) {
    unsafe { ffi::host_view_canvas_vline(x, y, h) };
}

/// \brief Flush queued draws to the display.
///
/// \param full_refresh true forces an anti-ghosting full refresh, false
///                     triggers a partial refresh.
pub fn commit(full_refresh: bool) {
    unsafe { ffi::host_view_canvas_commit(full_refresh) };
}

/// \brief Add a slider widget. Host steps the value on left/right keys.
pub fn add_slider(widget_id: u32, min: i32, max: i32, initial: i32, step: i32) {
    unsafe { ffi::host_view_canvas_add_slider(widget_id, min, max, initial, step) };
}

/// \brief Add a T9 text-input widget.
pub fn add_text(widget_id: u32, max_len: u16, initial: Option<&str>) {
    let init = initial.and_then(|s| CString::new(s).ok());
    let ptr = init.as_ref().map_or(core::ptr::null(), |c| c.as_ptr());
    unsafe { ffi::host_view_canvas_add_text(widget_id, max_len, ptr) };
}

/// \brief Add a button widget (Y on focus fires committed event).
pub fn add_button(widget_id: u32) {
    unsafe { ffi::host_view_canvas_add_button(widget_id) };
}

/// \brief Remove a widget from the canvas.
pub fn remove_widget(widget_id: u32) {
    unsafe { ffi::host_view_canvas_remove_widget(widget_id) };
}

/// \brief Overwrite a slider widget's value (clamped to range).
pub fn set_value(widget_id: u32, value: i32) {
    unsafe { ffi::host_view_canvas_set_value(widget_id, value) };
}

/// \brief Read a slider widget's current value.
pub fn get_value(widget_id: u32) -> Option<i32> {
    let mut out: i32 = 0;
    let rc = unsafe { ffi::host_view_canvas_get_value(widget_id, &mut out) };
    if rc == ffi::HOST_OK { Some(out) } else { None }
}

/// \brief Overwrite a text widget's buffer.
pub fn set_text(widget_id: u32, text: &str) {
    if let Ok(c) = CString::new(text) {
        unsafe { ffi::host_view_canvas_set_text(widget_id, c.as_ptr()) };
    }
}

/// \brief Read a text widget's current buffer.
pub fn get_text(widget_id: u32, max_len: usize) -> Option<String> {
    let cap = max_len.saturating_add(1);
    let mut buf = Vec::<u8>::with_capacity(cap);
    let rc = unsafe {
        buf.set_len(cap);
        ffi::host_view_canvas_get_text(widget_id, buf.as_mut_ptr() as *mut c_char, cap)
    };
    if rc < 0 {
        return None;
    }
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    buf.truncate(end);
    String::from_utf8(buf).ok()
}

/// \brief Focus a widget (0 to clear focus and route all keys to plugin).
pub fn set_focus(widget_id: u32) {
    unsafe { ffi::host_view_canvas_set_focus(widget_id) };
}

/// \brief Read the currently focused widget id (0 if none).
pub fn get_focus() -> u32 {
    let mut out: u32 = 0;
    unsafe { ffi::host_view_canvas_get_focus(&mut out) };
    out
}

/// \brief Configure the host's key-repeat timing for held keys.
pub fn set_key_repeat(initial_ms: u16, repeat_ms: u16) {
    unsafe { ffi::host_view_canvas_set_key_repeat(initial_ms, repeat_ms) };
}
