//! \file
//! \brief Plugin-drawn canvas view with inline widgets.
//!
//! Push a canvas, then draw into the body and add interactive widgets
//! (slider, T9 text input, button). The host owns input handling for
//! focused widgets; the plugin owns all rendering.
//!
//! Event flow:
//! - Every key press not consumed by a focused widget fires the canvas
//!   `key_action_id` with `idx = focused_widget`, `user_data = key_code` (the
//!   ASCII code of the key). Read the pressed key from `user_data`.
//! - Widget events (changed / committed / cancelled) fire
//!   `widget_action_id` with `idx = widget_id`, `user_data` one of
//!   [`WIDGET_CHANGED`], [`WIDGET_COMMITTED`], [`WIDGET_CANCELLED`].

use crate::{check, ffi, Result};
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

/// \brief Adafruit-GFX 6x8 builtin; CP437 codepoints for umlauts.
pub const FONT_BUILTIN: u8 = 0;
/// \brief FreeMonoBold 9pt; Latin-1 indexed.
pub const FONT_BOLD_9PT: u8 = 1;
/// \brief FreeMonoBold 12pt; Latin-1 indexed.
pub const FONT_BOLD_12PT: u8 = 2;
/// \brief FreeMonoBold 18pt; ASCII only.
pub const FONT_BOLD_18PT: u8 = 3;
/// \brief FreeMonoBold 24pt; ASCII only.
pub const FONT_BOLD_24PT: u8 = 4;

/// \brief Fill ink: nothing drawn (white).
pub const SHADE_NONE: u8 = 0;
/// \brief Fill ink: ~25% dithered grey.
pub const SHADE_LIGHT: u8 = 64;
/// \brief Fill ink: ~50% dithered grey.
pub const SHADE_MEDIUM: u8 = 128;
/// \brief Fill ink: ~75% dithered grey.
pub const SHADE_DARK: u8 = 192;
/// \brief Fill ink: solid black (default).
pub const SHADE_SOLID: u8 = 255;

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

/// \brief Switch the canvas font to one of the `FONT_*` ids.
///
/// Persists until the next [`clear`] or `set_font` call.
/// \param font_id One of the `FONT_*` constants.
/// \return `Ok(())` on success, `Err` for an out-of-range id.
pub fn set_font(font_id: u8) -> Result<()> {
    check(unsafe { ffi::host_view_canvas_set_font(font_id) })
}

/// \brief Pick the largest candidate font whose rendered `text` fits within
///        `max_width_px`.
///
/// Pure measurement; does not change canvas state. Pair with [`set_font`]
/// to apply the result. Candidates are evaluated in array order, so sort
/// them largest to smallest; the last entry is the fallback.
/// \param text         String to measure.
/// \param max_width_px Pixel budget.
/// \param candidates   `FONT_*` ids to consider.
/// \return The chosen font id, or `None` on invalid input.
pub fn pick_font_that_fits(text: &str, max_width_px: i16, candidates: &[u8]) -> Option<u8> {
    let c = CString::new(text).ok()?;
    let mut out: u8 = 0;
    let rc = unsafe {
        ffi::host_text_pick_font_that_fits(
            c.as_ptr(),
            max_width_px,
            candidates.as_ptr(),
            candidates.len() as u32,
            &mut out,
        )
    };
    if rc == ffi::HOST_OK {
        Some(out)
    } else {
        None
    }
}

/// \brief Set text color (true = white text for use over black bg).
pub fn set_text_inverted(inverted: bool) {
    unsafe { ffi::host_view_canvas_set_text_color(inverted) };
}

/// \brief Set the fill ink for subsequent filled shapes (rect, circle, triangle).
///
/// `SHADE_NONE` (0) draws nothing, `SHADE_SOLID` (255, the default) fills solid
/// black; values between dither an ordered-grey approximation (~64 levels) so the
/// 1-bpp panel can fake grey fills. Outlines, lines, text and bitmaps stay solid.
pub fn set_shade(shade: u8) {
    unsafe { ffi::host_view_canvas_set_shade(shade) };
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

/// \brief Draw a single pixel.
pub fn draw_pixel(x: i16, y: i16) {
    unsafe { ffi::host_view_canvas_draw_pixel(x, y) };
}

/// \brief Draw a line between two points.
pub fn draw_line(x0: i16, y0: i16, x1: i16, y1: i16) {
    unsafe { ffi::host_view_canvas_draw_line(x0, y0, x1, y1) };
}

/// \brief Draw a circle (filled = solid, else outline), radius `r` centred at (x, y).
pub fn draw_circle(x: i16, y: i16, r: i16, filled: bool) {
    unsafe { ffi::host_view_canvas_draw_circle(x, y, r, filled) };
}

/// \brief Draw a triangle through three points (filled = solid, else outline).
pub fn draw_triangle(x0: i16, y0: i16, x1: i16, y1: i16, x2: i16, y2: i16, filled: bool) {
    unsafe { ffi::host_view_canvas_draw_triangle(x0, y0, x1, y1, x2, y2, filled) };
}

/// \brief Draw a rounded rectangle (filled = solid, else outline), corner radius `r`.
pub fn draw_round_rect(x: i16, y: i16, w: i16, h: i16, r: i16, filled: bool) {
    unsafe { ffi::host_view_canvas_draw_round_rect(x, y, w, h, r, filled) };
}

/// \brief Draw a 1-bpp bitmap; set bits render black, unset bits are transparent.
///
/// Rows are byte-padded (stride = `(w + 7) / 8`), MSB first. `data` must hold
/// at least `stride * h` bytes; the host copies it, so it may be reused after.
pub fn draw_bitmap(x: i16, y: i16, w: i16, h: i16, data: &[u8]) {
    unsafe { ffi::host_view_canvas_draw_bitmap(x, y, w, h, data.as_ptr(), data.len()) };
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

/// \brief Set the action fired on a canvas long-press (0 to disable).
///
/// Registering a non-zero action opts the canvas into deferred short-press
/// input: a tap fires the key callback on release while a hold fires this
/// action (idx = 0, user_data = the ASCII key code) and suppresses the tap.
pub fn set_long_press_action(action_id: u32) {
    unsafe { ffi::host_view_canvas_set_long_press_action(action_id) };
}
