//! \file
//! \brief Low-level direct framebuffer drawing.
//!
//! Opt-in via the manifest capability `display_lowlevel`. Bypasses the
//! view system entirely; call [`flush`] to push pixels to the panel. Most
//! plugins should use [`crate::canvas`] instead.

use alloc::ffi::CString;
use core::ffi::{c_char, c_int};

/// \brief Generic display error returned by the helpers in this module.
#[derive(Debug, Clone, Copy)]
pub struct DisplayError;

/// \brief Display width in pixels.
pub fn width() -> u16 {
    unsafe { host_display_width() }
}

/// \brief Display height in pixels.
pub fn height() -> u16 {
    unsafe { host_display_height() }
}

/// \brief Clear the framebuffer to the background colour.
pub fn clear() -> Result<(), DisplayError> {
    rc(unsafe { host_display_clear() })
}

/// \brief Set a single pixel.
pub fn draw_pixel(x: i16, y: i16, color: u16) -> Result<(), DisplayError> {
    rc(unsafe { host_display_draw_pixel(x, y, color) })
}

/// \brief Draw a line between two points.
pub fn draw_line(x0: i16, y0: i16, x1: i16, y1: i16, color: u16) -> Result<(), DisplayError> {
    rc(unsafe { host_display_draw_line(x0, y0, x1, y1, color) })
}

/// \brief Draw a rectangle outline.
pub fn draw_rect(x: i16, y: i16, w: i16, h: i16, color: u16) -> Result<(), DisplayError> {
    rc(unsafe { host_display_draw_rect(x, y, w, h, color) })
}

/// \brief Draw a filled rectangle.
pub fn fill_rect(x: i16, y: i16, w: i16, h: i16, color: u16) -> Result<(), DisplayError> {
    rc(unsafe { host_display_fill_rect(x, y, w, h, color) })
}

/// \brief Draw text using the default GFX font.
pub fn draw_text(x: i16, y: i16, text: &str, size: u8, color: u16) -> Result<(), DisplayError> {
    let c = CString::new(text).map_err(|_| DisplayError)?;
    rc(unsafe { host_display_draw_text(x, y, c.as_ptr(), size, color) })
}

/// \brief Push the framebuffer to the panel.
/// \param refresh_mode Panel-specific refresh mode.
pub fn flush(refresh_mode: u8) -> Result<(), DisplayError> {
    rc(unsafe { host_display_flush(refresh_mode) })
}

/// \brief Whether the panel is still processing a previous refresh.
pub fn is_busy() -> bool {
    unsafe { host_display_is_busy() != 0 }
}

fn rc(code: c_int) -> Result<(), DisplayError> {
    if code == 0 {
        Ok(())
    } else {
        Err(DisplayError)
    }
}

#[link(wasm_import_module = "cdc")]
extern "C" {
    fn host_display_width() -> u16;
    fn host_display_height() -> u16;
    fn host_display_clear() -> c_int;
    fn host_display_draw_pixel(x: i16, y: i16, color: u16) -> c_int;
    fn host_display_draw_line(x0: i16, y0: i16, x1: i16, y1: i16, color: u16) -> c_int;
    fn host_display_draw_rect(x: i16, y: i16, w: i16, h: i16, color: u16) -> c_int;
    fn host_display_fill_rect(x: i16, y: i16, w: i16, h: i16, color: u16) -> c_int;
    fn host_display_draw_text(x: i16, y: i16, text: *const c_char, size: u8, color: u16) -> c_int;
    fn host_display_flush(refresh_mode: u8) -> c_int;
    fn host_display_is_busy() -> c_int;
}
