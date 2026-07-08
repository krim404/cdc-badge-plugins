//! \file
//! \brief Offscreen surfaces: compose 1-bpp images of arbitrary size with the
//!        canvas primitives (no capability required).
//!
//! A surface is an offscreen render target independent of the view stack -
//! e.g. 384 px wide for a thermal print band while the display is only
//! 296x128. The draw API mirrors the `canvas` module (same fonts, shades and
//! primitives); coordinates are raw surface pixels. Export yields packed rows
//! (MSB-first, set bit = black - the same layout as the `qr` and `image`
//! modules) or an encoded grayscale JPEG.
//!
//! A plugin can hold at most 2 surfaces of up to 64 KiB pixel data each; the
//! [`Surface`] handle destroys its surface on drop.

use crate::{check, Error, Result};
use alloc::ffi::CString;
use alloc::vec::Vec;
use core::ffi::{c_char, c_int};

/// Maximum surfaces a plugin may hold at once (`HOST_SURFACE_MAX_PER_PLUGIN`,
/// generated from the header by build.rs so it cannot drift).
pub const MAX_PER_PLUGIN: usize = crate::ffi::HOST_SURFACE_MAX_PER_PLUGIN as usize;
/// Maximum packed pixel bytes per surface (`HOST_SURFACE_MAX_BYTES`).
pub const MAX_BYTES: usize = crate::ffi::HOST_SURFACE_MAX_BYTES as usize;

/// \brief Text alignment for [`Surface::draw_text_aligned`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left = 0,
    Center = 1,
    Right = 2,
}

/// \brief An exported surface raster: packed 1-bpp rows, MSB-first, set bit = black.
pub struct Raster {
    pub data: Vec<u8>,
    pub stride_bytes: u16,
    pub width_px: u16,
    pub height_px: u16,
}

/// \brief An offscreen 1-bpp render target. Destroyed on drop.
pub struct Surface {
    handle: u32,
    width: u16,
    height: u16,
}

impl Surface {
    /// \brief Create a `w x h` surface, cleared to white.
    /// \param w Width in pixels, 1..1024.
    /// \param h Height in pixels, 1..2048; `((w+7)/8) * h` <= [`MAX_BYTES`].
    pub fn create(w: u16, h: u16) -> Result<Surface> {
        let rc = unsafe { host_surface_create(w, h) };
        if rc <= 0 {
            return Err(Error::from_code(rc));
        }
        Ok(Surface {
            handle: rc as u32,
            width: w,
            height: h,
        })
    }

    pub fn width(&self) -> u16 {
        self.width
    }
    pub fn height(&self) -> u16 {
        self.height
    }
    /// \brief Raw handle (for [`crate::sprite::Sprite::from_surface`] etc.).
    pub fn handle(&self) -> u32 {
        self.handle
    }
    /// \brief Bytes per packed row, `(width + 7) / 8`.
    pub fn stride_bytes(&self) -> u16 {
        self.width.div_ceil(8)
    }

    /// \brief Clear to white.
    pub fn clear(&self) -> Result<()> {
        check(unsafe { host_surface_clear(self.handle) })
    }

    /// \brief Select the font for subsequent text calls (`ui::FONT_*` id).
    pub fn set_font(&self, font_id: u8) -> Result<()> {
        check(unsafe { host_surface_set_font(self.handle, font_id) })
    }

    /// \brief Set the integer text scale (1..4).
    pub fn set_text_size(&self, size: u8) -> Result<()> {
        check(unsafe { host_surface_set_text_size(self.handle, size) })
    }

    /// \brief Draw text white-on-black (`true`) or black-on-white (`false`).
    pub fn set_text_inverted(&self, inverted: bool) -> Result<()> {
        check(unsafe { host_surface_set_text_color(self.handle, inverted as u8) })
    }

    /// \brief Set the fill shade for filled shapes: 0 white .. 255 solid black.
    pub fn set_shade(&self, shade: u8) -> Result<()> {
        check(unsafe { host_surface_set_shade(self.handle, shade) })
    }

    /// \brief Draw UTF-8 text with the current font/size at (x, y).
    pub fn draw_text(&self, x: i16, y: i16, text: &str) -> Result<()> {
        let c = CString::new(text).map_err(|_| Error::InvalidArg)?;
        check(unsafe { host_surface_draw_text(self.handle, x, y, c.as_ptr()) })
    }

    /// \brief Draw UTF-8 text aligned within a `w`-wide box starting at x.
    pub fn draw_text_aligned(
        &self,
        x: i16,
        y: i16,
        w: i16,
        text: &str,
        align: Align,
    ) -> Result<()> {
        let c = CString::new(text).map_err(|_| Error::InvalidArg)?;
        check(unsafe {
            host_surface_draw_text_aligned(self.handle, x, y, w, c.as_ptr(), align as u8)
        })
    }

    /// \brief Measure UTF-8 text with the current font/size.
    /// \return `(width, height)` in pixels.
    pub fn measure_text(&self, text: &str) -> Result<(u16, u16)> {
        let c = CString::new(text).map_err(|_| Error::InvalidArg)?;
        let mut w: u16 = 0;
        let mut h: u16 = 0;
        check(unsafe { host_surface_measure_text(self.handle, c.as_ptr(), &mut w, &mut h) })?;
        Ok((w, h))
    }

    pub fn draw_pixel(&self, x: i16, y: i16) -> Result<()> {
        check(unsafe { host_surface_draw_pixel(self.handle, x, y) })
    }

    pub fn draw_line(&self, x0: i16, y0: i16, x1: i16, y1: i16) -> Result<()> {
        check(unsafe { host_surface_draw_line(self.handle, x0, y0, x1, y1) })
    }

    /// \brief Rectangle; filled ones use the current shade.
    pub fn draw_rect(&self, x: i16, y: i16, w: i16, h: i16, filled: bool) -> Result<()> {
        check(unsafe { host_surface_draw_rect(self.handle, x, y, w, h, filled as u8) })
    }

    /// \brief Circle centered at (x, y); filled ones use the current shade.
    pub fn draw_circle(&self, x: i16, y: i16, r: i16, filled: bool) -> Result<()> {
        check(unsafe { host_surface_draw_circle(self.handle, x, y, r, filled as u8) })
    }

    /// \brief Triangle; filled ones use the current shade.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_triangle(
        &self,
        x0: i16,
        y0: i16,
        x1: i16,
        y1: i16,
        x2: i16,
        y2: i16,
        filled: bool,
    ) -> Result<()> {
        check(unsafe {
            host_surface_draw_triangle(self.handle, x0, y0, x1, y1, x2, y2, filled as u8)
        })
    }

    /// \brief Rounded rectangle with corner radius r.
    pub fn draw_round_rect(
        &self,
        x: i16,
        y: i16,
        w: i16,
        h: i16,
        r: i16,
        filled: bool,
    ) -> Result<()> {
        check(unsafe { host_surface_draw_round_rect(self.handle, x, y, w, h, r, filled as u8) })
    }

    pub fn hline(&self, x: i16, y: i16, w: i16) -> Result<()> {
        check(unsafe { host_surface_hline(self.handle, x, y, w) })
    }

    pub fn vline(&self, x: i16, y: i16, h: i16) -> Result<()> {
        check(unsafe { host_surface_vline(self.handle, x, y, h) })
    }

    /// \brief Blit a packed 1-bpp bitmap (MSB-first, set bit = black) at (x, y).
    pub fn draw_bitmap(&self, x: i16, y: i16, w: i16, h: i16, data: &[u8]) -> Result<()> {
        check(unsafe {
            host_surface_draw_bitmap(self.handle, x, y, w, h, crate::slice_ptr(data), data.len())
        })
    }

    /// \brief Export the packed pixel rows.
    pub fn export(&self) -> Result<Raster> {
        let stride = self.stride_bytes() as usize;
        let bytes = stride * self.height as usize;
        let mut buf = Vec::<u8>::with_capacity(bytes);
        let mut stride_out: u16 = 0;
        let n = unsafe {
            buf.set_len(bytes);
            host_surface_export(self.handle, buf.as_mut_ptr(), bytes, &mut stride_out)
        };
        if n < 0 {
            return Err(Error::from_code(n));
        }
        buf.truncate(n as usize);
        Ok(Raster {
            data: buf,
            stride_bytes: stride_out,
            width_px: self.width,
            height_px: self.height,
        })
    }

    /// \brief Encode the surface as a grayscale JPEG.
    /// \param quality 1..100 (0 = 85).
    pub fn export_jpg(&self, quality: u8) -> Result<Vec<u8>> {
        // A dithered 1-bpp JPEG rarely exceeds ~1.5 bits per pixel; retry once
        // with the exact size reported by the host when the guess is short.
        let mut cap = (self.width as usize * self.height as usize) / 4 + 4096;
        for _ in 0..2 {
            let mut buf = Vec::<u8>::with_capacity(cap);
            let mut len: u32 = 0;
            let rc = unsafe {
                buf.set_len(cap);
                host_surface_export_jpg(self.handle, quality, buf.as_mut_ptr(), cap, &mut len)
            };
            if rc == 0 {
                buf.truncate(len as usize);
                return Ok(buf);
            }
            if rc == crate::ffi::HOST_ERR_NO_MEMORY && len as usize > cap {
                cap = len as usize;
                continue;
            }
            return Err(Error::from_code(rc));
        }
        Err(Error::NoMemory)
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        unsafe { host_surface_destroy(self.handle) };
    }
}

#[link(wasm_import_module = "cdc")]
extern "C" {
    fn host_surface_create(w: u16, h: u16) -> c_int;
    fn host_surface_destroy(surface: u32) -> c_int;
    fn host_surface_clear(surface: u32) -> c_int;
    fn host_surface_set_font(surface: u32, font_id: u8) -> c_int;
    fn host_surface_set_text_size(surface: u32, size: u8) -> c_int;
    fn host_surface_set_text_color(surface: u32, inverted: u8) -> c_int;
    fn host_surface_set_shade(surface: u32, shade: u8) -> c_int;
    fn host_surface_draw_text(surface: u32, x: i16, y: i16, text: *const c_char) -> c_int;
    fn host_surface_draw_text_aligned(
        surface: u32,
        x: i16,
        y: i16,
        w: i16,
        text: *const c_char,
        align: u8,
    ) -> c_int;
    fn host_surface_measure_text(
        surface: u32,
        text: *const c_char,
        out_w: *mut u16,
        out_h: *mut u16,
    ) -> c_int;
    fn host_surface_draw_pixel(surface: u32, x: i16, y: i16) -> c_int;
    fn host_surface_draw_line(surface: u32, x0: i16, y0: i16, x1: i16, y1: i16) -> c_int;
    fn host_surface_draw_rect(surface: u32, x: i16, y: i16, w: i16, h: i16, filled: u8) -> c_int;
    fn host_surface_draw_circle(surface: u32, x: i16, y: i16, r: i16, filled: u8) -> c_int;
    fn host_surface_draw_triangle(
        surface: u32,
        x0: i16,
        y0: i16,
        x1: i16,
        y1: i16,
        x2: i16,
        y2: i16,
        filled: u8,
    ) -> c_int;
    fn host_surface_draw_round_rect(
        surface: u32,
        x: i16,
        y: i16,
        w: i16,
        h: i16,
        r: i16,
        filled: u8,
    ) -> c_int;
    fn host_surface_hline(surface: u32, x: i16, y: i16, w: i16) -> c_int;
    fn host_surface_vline(surface: u32, x: i16, y: i16, h: i16) -> c_int;
    fn host_surface_draw_bitmap(
        surface: u32,
        x: i16,
        y: i16,
        w: i16,
        h: i16,
        data: *const u8,
        len: usize,
    ) -> c_int;
    fn host_surface_export(
        surface: u32,
        out: *mut u8,
        out_size: usize,
        out_stride_bytes: *mut u16,
    ) -> c_int;
    fn host_surface_export_jpg(
        surface: u32,
        quality: u8,
        out: *mut u8,
        out_size: usize,
        out_len: *mut u32,
    ) -> c_int;
}
