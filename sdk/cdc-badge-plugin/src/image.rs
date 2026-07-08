//! \file
//! \brief Image decoding: PNG/JPEG bytes into a dithered 1-bpp raster
//!        (no capability required).
//!
//! Uses the firmware image-viewer engine (max ~1 megapixel input). Output
//! layout matches the `qr` and `surface` modules: packed rows, MSB-first,
//! a set bit is black.

use crate::{check, Error, Result};
use alloc::vec::Vec;
use core::ffi::c_int;

/// \brief A decoded, dithered 1-bpp raster.
pub struct MonoImage {
    pub data: Vec<u8>,
    pub stride_bytes: u16,
    pub width_px: u16,
    pub height_px: u16,
}

/// \brief Native dimensions of an encoded PNG/JPEG, or `None` on a decode error.
pub fn info(data: &[u8]) -> Option<(u16, u16)> {
    let mut w: u16 = 0;
    let mut h: u16 = 0;
    let rc = unsafe { host_image_info(crate::slice_ptr(data), data.len(), &mut w, &mut h) };
    if rc == 0 {
        Some((w, h))
    } else {
        None
    }
}

/// \brief Decode, scale to `target_w` (aspect preserved) and dither to 1-bpp.
/// \param target_w Output width in pixels, 1..1024 (e.g. 384 for thermal print).
pub fn render(data: &[u8], target_w: u16) -> Result<MonoImage> {
    let (w, h) = info(data).ok_or(Error::Generic)?;
    if w == 0 || h == 0 || target_w == 0 {
        return Err(Error::InvalidArg);
    }
    // Round-to-nearest height estimate plus one spare row: if the host ceils
    // instead, the buffer would otherwise be one row short (spurious NO_MEMORY).
    let target_h = ((h as u32 * target_w as u32 + w as u32 / 2) / w as u32).max(1) as usize + 1;
    let stride = (target_w as usize).div_ceil(8);
    let mut buf = Vec::<u8>::with_capacity(stride * target_h);

    let mut stride_out: u16 = 0;
    let mut height_out: u16 = 0;
    let rc = unsafe {
        buf.set_len(stride * target_h);
        host_image_render(
            crate::slice_ptr(data),
            data.len(),
            target_w,
            buf.as_mut_ptr(),
            stride * target_h,
            &mut stride_out,
            &mut height_out,
        )
    };
    check(rc)?;
    buf.truncate(stride_out as usize * height_out as usize);
    Ok(MonoImage {
        data: buf,
        stride_bytes: stride_out,
        width_px: target_w,
        height_px: height_out,
    })
}

#[link(wasm_import_module = "cdc")]
extern "C" {
    fn host_image_info(data: *const u8, len: usize, out_w: *mut u16, out_h: *mut u16) -> c_int;
    fn host_image_render(
        data: *const u8,
        len: usize,
        target_w: u16,
        out: *mut u8,
        out_size: usize,
        out_stride_bytes: *mut u16,
        out_height_px: *mut u16,
    ) -> c_int;
}
