//! \file
//! \brief Multi-frame 1-bpp sprites with host-driven frame playback.
//!
//! A sprite is a frame sheet: `frame_count` frames of `w x h` pixels stacked
//! vertically, packed rows MSB-first, set bit = black (the surface/QR/image
//! layout). Draw it into the canvas by reference with
//! [`crate::canvas::draw_sprite`] - inside an element for movement, z-order
//! and tweens - and let [`Sprite::play`] advance frames on the host clock.
//!
//! Sprites live in the canvas: they are freed when the canvas view is popped
//! or cleared, and creating them requires an open canvas.
//!
//! ```ignore
//! let walker = sprite::Sprite::from_sheet(16, 16, 4, &sheet)?;
//! canvas::elem_begin(ELEM_WALKER)?;
//! canvas::draw_sprite(0, 40, walker.handle())?;
//! canvas::elem_end();
//! walker.play(sprite::Mode::Loop, 200, sprite::REPEAT_FOREVER, 0)?;
//! ```

use crate::{check, ffi, Error, Result};

/// Playback modes for [`Sprite::play`].
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Play to the last frame and hold it.
    Once,
    /// Wrap around; finite runs hold the last frame.
    Loop,
    /// Forward then backward per cycle.
    PingPong,
}

impl Mode {
    fn raw(self) -> u8 {
        match self {
            Mode::Once => ffi::HOST_SPRITE_ONCE as u8,
            Mode::Loop => ffi::HOST_SPRITE_LOOP as u8,
            Mode::PingPong => ffi::HOST_SPRITE_PING_PONG as u8,
        }
    }
}

/// `repeat` value meaning "repeat forever".
pub const REPEAT_FOREVER: u16 = ffi::HOST_ANIM_REPEAT_FOREVER as u16;

/// Unset data bits paint white instead of being transparent.
pub const FLAG_OPAQUE: u8 = ffi::HOST_SPRITE_FLAG_OPAQUE as u8;
/// Mirror horizontally when drawn.
pub const FLAG_FLIP_H: u8 = ffi::HOST_SPRITE_FLAG_FLIP_H as u8;
/// Mirror vertically when drawn.
pub const FLAG_FLIP_V: u8 = ffi::HOST_SPRITE_FLAG_FLIP_V as u8;
/// Rotate 90 deg clockwise before the flips (combine for 180/270).
pub const FLAG_ROT_90: u8 = ffi::HOST_SPRITE_FLAG_ROT_90 as u8;

/// A sprite resource held by the current canvas.
///
/// Not `Drop`-managed: the host frees all sprites with the canvas, so leaking
/// a handle is harmless. Call [`Sprite::destroy`] to reclaim arena space
/// early.
#[derive(Clone, Copy)]
pub struct Sprite {
    handle: u32,
}

impl Sprite {
    /// \brief Create a sprite from a packed 1-bpp frame sheet.
    ///
    /// `sheet` holds `frame_count` frames of `w x h` stacked vertically,
    /// rows byte-padded, MSB first.
    pub fn from_sheet(w: u16, h: u16, frame_count: u16, sheet: &[u8]) -> Result<Sprite> {
        let rc = unsafe { ffi::host_sprite_create(w, h, frame_count, sheet.as_ptr(), sheet.len()) };
        if rc <= 0 {
            return Err(Error::from_code(rc));
        }
        Ok(Sprite { handle: rc as u32 })
    }

    /// \brief Create a sprite by slicing a surface into a row-major grid of
    ///        `w x h` cells (compose frames with surface drawing first).
    pub fn from_surface(surface: &crate::surface::Surface, w: u16, h: u16,
                        frame_count: u16) -> Result<Sprite> {
        let rc = unsafe {
            ffi::host_sprite_create_from_surface(surface.handle(), w, h, frame_count)
        };
        if rc <= 0 {
            return Err(Error::from_code(rc));
        }
        Ok(Sprite { handle: rc as u32 })
    }

    /// \brief Raw handle for [`crate::canvas::draw_sprite`].
    pub fn handle(&self) -> u32 {
        self.handle
    }

    /// \brief Rewrap a stored raw handle (e.g. from a `Cell<u32>` kept across
    ///        callbacks). No validation happens until the next host call.
    pub fn from_handle(handle: u32) -> Sprite {
        Sprite { handle }
    }

    /// \brief Create a sprite straight from an encoded PNG/JPEG filmstrip:
    ///        decoded, scaled to `target_w`, dithered and sliced vertically
    ///        into `frame_h`-tall frames.
    pub fn from_image(data: &[u8], target_w: u16, frame_h: u16) -> Result<Sprite> {
        let rc = unsafe {
            ffi::host_sprite_create_from_image(data.as_ptr(), data.len(), target_w, frame_h)
        };
        if rc <= 0 {
            return Err(Error::from_code(rc));
        }
        Ok(Sprite { handle: rc as u32 })
    }

    /// \brief Integer upscale 1..4 applied whenever the sprite is drawn
    ///        (lossless on 1-bpp - pixels just get fatter).
    pub fn set_scale(&self, scale: u8) -> Result<()> {
        check(unsafe { ffi::host_sprite_set_scale(self.handle, scale) })
    }

    /// \brief Attach a transparency mask (same sheet layout as the frames).
    ///
    /// Mask bit set = pixel painted (black or white per the data bit), unset
    /// = transparent; overrides [`FLAG_OPAQUE`].
    pub fn set_mask(&self, mask: &[u8]) -> Result<()> {
        check(unsafe { ffi::host_sprite_set_mask(self.handle, mask.as_ptr(), mask.len()) })
    }

    /// \brief Replace the flag set (`FLAG_OPAQUE | FLAG_FLIP_H | FLAG_FLIP_V`).
    pub fn set_flags(&self, flags: u8) -> Result<()> {
        check(unsafe { ffi::host_sprite_set_flags(self.handle, flags) })
    }

    /// \brief Jump to a frame; a running playback continues from it.
    pub fn set_frame(&self, frame: u16) -> Result<()> {
        check(unsafe { ffi::host_sprite_set_frame(self.handle, frame) })
    }

    /// \brief Currently displayed frame index.
    pub fn frame(&self) -> Result<u16> {
        let mut out: u16 = 0;
        check(unsafe { ffi::host_sprite_get_frame(self.handle, &mut out) })?;
        Ok(out)
    }

    /// \brief Per-frame durations in ms (`ms.len()` must equal the frame
    ///        count); an empty slice reverts to the global playback duration.
    pub fn set_frame_durations(&self, ms: &[u16]) -> Result<()> {
        check(unsafe {
            ffi::host_sprite_set_frame_durations(self.handle, ms.as_ptr(), ms.len() as u16)
        })
    }

    /// \brief Start host-driven playback from frame 0.
    ///
    /// `repeat` counts extra cycles ([`REPEAT_FOREVER`] = endless); a
    /// finished finite playback fires
    /// `plugin_on_action(done_action_id, handle, final_frame)`.
    pub fn play(&self, mode: Mode, frame_ms: u16, repeat: u16,
                done_action_id: u32) -> Result<()> {
        check(unsafe {
            ffi::host_sprite_play(self.handle, mode.raw(), frame_ms, repeat, done_action_id)
        })
    }

    /// \brief Stop playback, keeping the current frame on screen.
    pub fn stop(&self) -> Result<()> {
        check(unsafe { ffi::host_sprite_stop(self.handle) })
    }

    /// \brief Destroy the sprite; recorded draw commands referencing it are
    ///        skipped from then on.
    pub fn destroy(self) -> Result<()> {
        check(unsafe { ffi::host_sprite_destroy(self.handle) })
    }

    /// \brief Stamp the current frame onto a surface (immediate pixel copy,
    ///        honoring mask and flip flags).
    pub fn stamp_onto(&self, surface: &crate::surface::Surface, x: i16, y: i16) -> Result<()> {
        check(unsafe { ffi::host_surface_draw_sprite(surface.handle(), x, y, self.handle) })
    }
}

/// 8-frame 16x16 spinner (a dot orbiting a ring), ready to `play(Loop, ..)`.
/// Draw it via [`crate::canvas::draw_sprite`]; scale it up with
/// [`Sprite::set_scale`] for larger indicators.
pub fn spinner() -> Result<Sprite> {
    // One orbit position per frame, 45 deg steps on a ring of radius 6
    // around (8, 8), dot radius 2, plus a faint ring of 8 anchor dots.
    const POS: [(i16, i16); 8] = [
        (8, 2), (12, 4), (14, 8), (12, 12), (8, 14), (4, 12), (2, 8), (4, 4),
    ];
    let mut sheet = [0u8; 2 * 16 * 8];
    for (f, &(cx, cy)) in POS.iter().enumerate() {
        let frame = &mut sheet[f * 32..(f + 1) * 32];
        // Anchor dots: single pixels at every orbit position.
        for &(ax, ay) in POS.iter() {
            frame[(ay as usize) * 2 + (ax as usize) / 8] |= 0x80 >> (ax % 8);
        }
        // The moving dot: a filled 3x3 block (clipped to the frame).
        for dy in -1i16..=1 {
            for dx in -1i16..=1 {
                let (px, py) = (cx + dx, cy + dy);
                if (0..16).contains(&px) && (0..16).contains(&py) {
                    frame[(py as usize) * 2 + (px as usize) / 8] |= 0x80 >> (px % 8);
                }
            }
        }
    }
    Sprite::from_sheet(16, 16, 8, &sheet)
}
