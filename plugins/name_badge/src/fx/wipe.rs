//! \file
//! \brief FX4 "Dither Wipe": pure hand-rolled effect, no framework at all.
//!        The name is rendered once into a surface, exported as a raster,
//!        then revealed and hidden through Bayer dissolve levels, breathing
//!        endlessly between positive and inverted (white-on-black) phases.

use crate::cell::{PluginCell, PluginRef};
use crate::dither;
use crate::namefit::{self, FxContext};
use alloc::vec::Vec;
use cdc_badge_plugin::{canvas, surface::Surface};

const ELEM_REVEAL: u32 = 10;
const WIPE_STEP_MS: u64 = 400;
const BAND_PAD: i16 = 6;

static LAST_MS: PluginCell<u64> = PluginCell::new(0);
static LEVEL: PluginCell<i8> = PluginCell::new(0);
/// +1 revealing, -1 dissolving.
static DIR: PluginCell<i8> = PluginCell::new(1);
/// false = black-on-white raster, true = inverted band.
static NEGATIVE: PluginCell<bool> = PluginCell::new(false);

struct Band {
    raster: Vec<u8>,
    stride: usize,
    w: u16,
    h: u16,
}
static BAND: PluginRef<Option<Band>> = PluginRef::new(None);

pub fn enter(ctx: &FxContext, now_ms: u64) {
    LAST_MS.set(now_ms);
    LEVEL.set(0);
    DIR.set(1);
    NEGATIVE.set(false);

    // Render the name once into a transient surface and keep the raster.
    let band_h = (ctx.name_h as i16 + 2 * BAND_PAD).min(ctx.h) as u16;
    *BAND.borrow_mut() = render_band(ctx, band_h);

    canvas::elem_begin(ELEM_REVEAL).ok();
    canvas::elem_end();
    // First tick paints level 0 (all white); nothing else exists on screen.
}

fn render_band(ctx: &FxContext, band_h: u16) -> Option<Band> {
    let name = namefit::NAME.borrow();
    let s = Surface::create(ctx.w as u16, band_h).ok()?;
    s.set_font(ctx.font).ok()?;
    // Marquee-wide names show their leftmost window in this effect. The
    // surface draw_text is baseline-anchored like the canvas one.
    let x = if ctx.marquee { 0 } else { ctx.name_x() };
    s.draw_text(x, BAND_PAD + ctx.ascent as i16, &name).ok()?;
    let raster = s.export().ok()?;
    Some(Band {
        raster: raster.data,
        stride: raster.stride_bytes as usize,
        w: raster.width_px,
        h: raster.height_px,
    })
}

pub fn tick(ctx: &FxContext, now_ms: u64) {
    if now_ms.saturating_sub(LAST_MS.get()) < WIPE_STEP_MS {
        return;
    }
    LAST_MS.set(now_ms);

    let band = BAND.borrow();
    let Some(band) = band.as_ref() else { return };

    // Advance the dissolve; at the empty end swap polarity, at the full end
    // linger by reversing so the name stays readable most of the time.
    let mut level = LEVEL.get() + DIR.get();
    if level > dither::LEVEL_MAX as i8 {
        level = dither::LEVEL_MAX as i8;
        DIR.set(-1);
    } else if level < 0 {
        level = 0;
        DIR.set(1);
        NEGATIVE.set(!NEGATIVE.get());
    }
    LEVEL.set(level);

    let source = if NEGATIVE.get() {
        dither::invert(&band.raster)
    } else {
        band.raster.clone()
    };
    let masked = dither::apply_bayer(&source, band.stride, level as u8);

    let band_y = ctx.name_y() - BAND_PAD;
    canvas::elem_clear(ELEM_REVEAL).ok();
    canvas::elem_begin(ELEM_REVEAL).ok();
    canvas::draw_bitmap(0, band_y, band.w as i16, band.h as i16, &masked);
    canvas::elem_end();
    canvas::commit(false);
}

pub fn on_action(_ctx: &FxContext, _action_id: u32) {}
