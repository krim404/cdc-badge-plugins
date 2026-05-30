//! \file
//! \brief Addressable pixel strip (WS2811/WS2812/WS2813/SK6812 ...).
//!
//! One strip handle is shared between plugins; the
//! `(gpio_pin, num_pixels, format)` tuple given to the first successful
//! `init()` identifies the configuration. The manifest must declare
//! `capabilities.pixel_strip = true`.

use crate::{check, Result};
use core::ffi::c_int;

/// \brief Pixel layout the firmware expects.
///
/// `Grb` matches the most common WS2812/WS2813/SK6812 strips. `Grbw` /
/// `Rgbw` add a dedicated white channel (the white byte is currently
/// always written as 0 from the plugin side).
#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Format {
    Grb = 0,
    Rgb = 1,
    Grbw = 2,
    Rgbw = 3,
}

#[link(wasm_import_module = "cdc")]
extern "C" {
    fn host_pixel_strip_init(gpio_pin: u8, num_pixels: u16, format: u8) -> c_int;
    fn host_pixel_strip_deinit() -> c_int;
    fn host_pixel_strip_set(index: u16, r: u8, g: u8, b: u8) -> c_int;
    fn host_pixel_strip_fill(r: u8, g: u8, b: u8) -> c_int;
    fn host_pixel_strip_clear() -> c_int;
    fn host_pixel_strip_refresh() -> c_int;
    fn host_pixel_strip_length() -> u16;
    fn host_pixel_strip_ready() -> bool;
}

/// \brief Initialise (or re-initialise) the strip.
///
/// A subsequent call with the same parameters is a no-op; with a
/// different tuple the previous handle is replaced.
/// \param gpio_pin   GPIO driving the data line; must be allowed in
///                   `capabilities.gpio_pins`.
/// \param num_pixels Number of LEDs in the strip.
/// \param format     Pixel layout, see [`Format`].
/// \return `Ok(())` on success, `Err(Error)` on failure.
pub fn init(gpio_pin: u8, num_pixels: u16, format: Format) -> Result<()> {
    check(unsafe { host_pixel_strip_init(gpio_pin, num_pixels, format as u8) })
}

/// \brief Release the strip handle and free RMT resources.
/// \return `Ok(())` on success, `Err(Error)` if no strip was initialised.
pub fn deinit() -> Result<()> {
    check(unsafe { host_pixel_strip_deinit() })
}

/// \brief Set a single pixel in the framebuffer.
///
/// The new value is only visible on the strip after [`refresh`].
/// \param index 0-based pixel index.
/// \param r     Red channel `0..=255`.
/// \param g     Green channel `0..=255`.
/// \param b     Blue channel `0..=255`.
/// \return `Ok(())` on success, `Err(Error)` on out-of-range index.
pub fn set(index: u16, r: u8, g: u8, b: u8) -> Result<()> {
    check(unsafe { host_pixel_strip_set(index, r, g, b) })
}

/// \brief Fill the entire framebuffer with one colour.
/// \param r Red channel `0..=255`.
/// \param g Green channel `0..=255`.
/// \param b Blue channel `0..=255`.
/// \return `Ok(())` on success, `Err(Error)` on failure.
pub fn fill(r: u8, g: u8, b: u8) -> Result<()> {
    check(unsafe { host_pixel_strip_fill(r, g, b) })
}

/// \brief Clear the framebuffer (all pixels off).
/// \return `Ok(())` on success, `Err(Error)` on failure.
pub fn clear() -> Result<()> {
    check(unsafe { host_pixel_strip_clear() })
}

/// \brief Push the framebuffer to the strip.
/// \return `Ok(())` on success, `Err(Error)` on failure.
pub fn refresh() -> Result<()> {
    check(unsafe { host_pixel_strip_refresh() })
}

/// \brief Number of pixels the strip was initialised with.
/// \return Pixel count, or `0` when no strip is configured.
pub fn length() -> u16 {
    unsafe { host_pixel_strip_length() }
}

/// \brief Whether the strip is initialised and ready for writes.
/// \return `true` once [`init`] has succeeded.
pub fn is_ready() -> bool {
    unsafe { host_pixel_strip_ready() }
}
