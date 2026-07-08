//! \file
//! \brief Name layout: font auto-fit, measurement and the shared hero
//!        element every effect starts from.

use crate::cell::{PluginCell, PluginRef};
use crate::textmath;
use alloc::string::String;
use cdc_badge_plugin::{canvas, surface::Surface};

/// Horizontal breathing room the name keeps from the panel edges.
pub const NAME_MARGIN: i16 = 6;
/// Hero element id, shared by all effects (namefit records it).
pub const ELEM_NAME: u32 = 1;
/// Optional backdrop element behind the name (effects record it themselves).
pub const ELEM_BANNER: u32 = 2;

/// Everything an effect needs to lay out around the name. Copy so it lives
/// in a `PluginCell`; the name string itself is in [`NAME`].
#[derive(Clone, Copy)]
pub struct FxContext {
    pub w: i16,
    pub h: i16,
    pub font: u8,
    pub name_w: u16,
    pub name_h: u16,
    /// Cap-height of the picked font: distance from the glyph top to the
    /// baseline. The GFX display fonts draw from the BASELINE, so every
    /// `draw_text*` call needs `top + ascent`, not the top edge itself
    /// (the marquee sprite, bitmaps and rects stay top-anchored).
    pub ascent: u16,
    pub marquee: bool,
}

impl FxContext {
    pub const fn empty() -> Self {
        Self {
            w: 0,
            h: 0,
            font: 0,
            name_w: 0,
            name_h: 0,
            ascent: 0,
            marquee: false,
        }
    }

    /// Body-local y where the name band starts when vertically centered.
    pub fn name_y(&self) -> i16 {
        (self.h - self.name_h as i16) / 2
    }

    /// Baseline y for `draw_text*` so the glyphs sit inside the name band.
    pub fn baseline_y(&self) -> i16 {
        self.name_y() + self.ascent as i16
    }

    /// Body-local x of the name's left edge when horizontally centered.
    pub fn name_x(&self) -> i16 {
        if self.marquee {
            NAME_MARGIN
        } else {
            (self.w - self.name_w as i16) / 2
        }
    }

    /// Width the name actually occupies on screen.
    pub fn shown_w(&self) -> i16 {
        if self.marquee {
            self.w - 2 * NAME_MARGIN
        } else {
            self.name_w as i16
        }
    }
}

pub static NAME: PluginRef<String> = PluginRef::new(String::new());
pub static CTX: PluginCell<FxContext> = PluginCell::new(FxContext::empty());

/// The 18/24 pt faces are ASCII-only; umlauts need the Latin-1 9/12 pt set.
fn font_candidates(name: &str) -> &'static [u8] {
    if name.is_ascii() {
        &[
            canvas::FONT_BOLD_24PT,
            canvas::FONT_BOLD_18PT,
            canvas::FONT_BOLD_12PT,
            canvas::FONT_BOLD_9PT,
        ]
    } else {
        &[canvas::FONT_BOLD_12PT, canvas::FONT_BOLD_9PT]
    }
}

/// Measure text at `font` via a throwaway surface (measure_text needs no
/// room, only a drawing context).
fn measure(text: &str, font: u8) -> (u16, u16) {
    let Ok(s) = Surface::create(8, 8) else {
        return (0, 12);
    };
    let _ = s.set_font(font);
    s.measure_text(text).unwrap_or((0, 12))
}

/// Rebuild the layout context after the name (or the view) changed.
pub fn build_context(name: &str) -> FxContext {
    let (w, h) = canvas::body_size();
    let (w, h) = (w as i16, h as i16);
    let max_w = w - 2 * NAME_MARGIN;
    let font = canvas::pick_font_that_fits(name, max_w, font_candidates(name))
        .unwrap_or(canvas::FONT_BOLD_9PT);
    let (name_w, name_h) = measure(name, font);
    // Cap height doubles as the baseline offset; "A" has no descender, so
    // its bbox height is exactly the ascent of these all-baseline GFX fonts.
    let (_, ascent) = measure("A", font);
    FxContext {
        w,
        h,
        font,
        name_w,
        name_h,
        ascent,
        marquee: name_w as i16 > max_w,
    }
}

/// Record the hero name element with its band top at `top_y` (its left/center
/// placement follows the context). Effects call this inside their `enter`.
/// Overlong names become a host-driven marquee instead of static text.
pub fn record_name_elem(ctx: &FxContext, top_y: i16) {
    record_name_elem_styled(ctx, top_y, false);
}

/// Like [`record_name_elem`], with white (inverted) text for dark backdrops.
pub fn record_name_elem_styled(ctx: &FxContext, top_y: i16, inverted: bool) {
    let name = NAME.borrow();
    canvas::elem_begin(ELEM_NAME).ok();
    canvas::set_font(ctx.font).ok();
    canvas::set_text_size(1);
    canvas::set_text_inverted(inverted);
    if ctx.marquee {
        // The marquee sprite is TOP-anchored (unlike draw_text's baseline).
        canvas::marquee(
            NAME_MARGIN,
            top_y,
            ctx.shown_w(),
            &name,
            textmath::MARQUEE_STEP_PX,
            textmath::MARQUEE_FRAME_MS,
        )
        .ok();
    } else {
        canvas::draw_text_aligned(
            0,
            top_y + ctx.ascent as i16,
            ctx.w,
            &name,
            canvas::ALIGN_CENTER,
        );
    }
    canvas::set_text_inverted(false);
    canvas::elem_end();
}
