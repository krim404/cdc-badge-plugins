//! \file
//! \brief QR encoding into a packed 1-bpp raster (no capability required).
//!
//! Output layout matches the `image` and `surface` modules: packed rows,
//! MSB-first, a set bit is black. Blit the result onto a surface with
//! [`crate::surface::Surface::draw_bitmap`] or send it to a printer.

use crate::{check, Error, Result};
use alloc::ffi::CString;
use alloc::vec::Vec;
use core::ffi::{c_char, c_int};

/// \brief ECC level for QR encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ecc {
    Low = 0,
    Medium = 1,
    Quartile = 2,
    High = 3,
}

/// \brief A rendered QR raster: packed 1-bpp rows, MSB-first, set bit = black.
pub struct QrBitmap {
    pub data: Vec<u8>,
    pub stride_bytes: u16,
    /// Side length in pixels (the raster is square).
    pub side_px: u16,
}

/// \brief Measure the module count for `data` without rendering.
/// \param max_version Maximum QR version 1..20 (0 = 20).
pub fn measure(data: &str, max_version: u8, ecc: Ecc) -> Option<u16> {
    let c = CString::new(data).ok()?;
    let mut modules: u16 = 0;
    let rc = unsafe { host_qr_measure(c.as_ptr(), max_version, ecc as u8, &mut modules) };
    if rc == 0 {
        Some(modules)
    } else {
        None
    }
}

/// \brief Encode `data` into a packed 1-bpp QR raster.
///
/// The rendered side is `(modules + 2 * quiet_modules) * scale` pixels,
/// capped at 1024; the quiet zone is white. The buffer is sized via
/// [`measure`] automatically.
pub fn render_bitmap(
    data: &str,
    max_version: u8,
    ecc: Ecc,
    scale: u8,
    quiet_modules: u8,
) -> Result<QrBitmap> {
    let modules = measure(data, max_version, ecc).ok_or(Error::Generic)?;
    let scale = if scale == 0 { 1 } else { scale };
    let side = (modules as usize + 2 * quiet_modules as usize) * scale as usize;
    let stride = side.div_ceil(8);
    let mut buf = Vec::<u8>::with_capacity(stride * side);

    let c = CString::new(data).map_err(|_| Error::InvalidArg)?;
    let mut stride_out: u16 = 0;
    let mut height_out: u16 = 0;
    let rc = unsafe {
        buf.set_len(stride * side);
        host_qr_render_bitmap(
            c.as_ptr(),
            max_version,
            ecc as u8,
            scale,
            quiet_modules,
            buf.as_mut_ptr(),
            stride * side,
            &mut stride_out,
            &mut height_out,
        )
    };
    check(rc)?;
    buf.truncate(stride_out as usize * height_out as usize);
    Ok(QrBitmap {
        data: buf,
        stride_bytes: stride_out,
        side_px: height_out,
    })
}

#[link(wasm_import_module = "cdc")]
extern "C" {
    fn host_qr_measure(
        data: *const c_char,
        max_version: u8,
        ecc: u8,
        out_modules: *mut u16,
    ) -> c_int;
    fn host_qr_render_bitmap(
        data: *const c_char,
        max_version: u8,
        ecc: u8,
        scale: u8,
        quiet_modules: u8,
        out: *mut u8,
        out_size: usize,
        out_stride_bytes: *mut u16,
        out_height_px: *mut u16,
    ) -> c_int;
}
