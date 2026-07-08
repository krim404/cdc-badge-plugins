//! \file
//! \brief FX3 "Sparkle Night": inverted banner with the name in white,
//!        surrounded by twinkling star sprites that teleport to fresh
//!        PRNG positions, never covering the name.

use crate::cell::{PluginCell, PluginRef};
use crate::gfx;
use crate::namefit::{self, FxContext};
use crate::rng::XorShift32;
use cdc_badge_plugin::{canvas, random, sprite};

const SPARK_ELEMS: [u32; 5] = [10, 11, 12, 13, 14];
const SPARK_STEP_MS: u64 = 600;
const BANNER_PAD: i16 = 8;

static LAST_MS: PluginCell<u64> = PluginCell::new(0);
static RNG: PluginRef<XorShift32> = PluginRef::new(XorShift32::new(1));

pub fn enter(ctx: &FxContext, now_ms: u64) {
    LAST_MS.set(now_ms);
    *RNG.borrow_mut() = XorShift32::new(random::u32().unwrap_or(now_ms as u32));

    // Solid banner band behind the name, name in white on top of it.
    let band_y = ctx.name_y() - BANNER_PAD;
    let band_h = ctx.name_h as i16 + 2 * BANNER_PAD;
    canvas::elem_begin(namefit::ELEM_BANNER).ok();
    canvas::set_shade(canvas::SHADE_SOLID);
    canvas::draw_round_rect(2, band_y, ctx.w - 4, band_h, 6, true);
    canvas::elem_end();
    canvas::elem_set_z(namefit::ELEM_BANNER, -2).ok();

    // White name over the black banner.
    namefit::record_name_elem_styled(ctx, ctx.name_y(), true);

    // Two twinkle sprites at staggered phases; five spark elements share them.
    let sheet = gfx::star_sheet();
    let mut handles = [0u32; 2];
    for (i, h) in handles.iter_mut().enumerate() {
        if let Ok(s) =
            sprite::Sprite::from_sheet(gfx::STAR_W, gfx::STAR_H, gfx::STAR_FRAMES, &sheet)
        {
            s.set_frame((i as u16) * 2).ok();
            s.play(sprite::Mode::PingPong, 400, sprite::REPEAT_FOREVER, 0)
                .ok();
            *h = s.handle();
        }
    }

    let mut rng = RNG.borrow_mut();
    for (i, &elem) in SPARK_ELEMS.iter().enumerate() {
        canvas::elem_begin(elem).ok();
        canvas::draw_sprite(0, 0, handles[i % handles.len()]).ok();
        canvas::elem_end();
        let (x, y) = spark_position(ctx, &mut rng, band_y, band_h);
        canvas::elem_set_offset(elem, x, y).ok();
    }
}

/// Random spark position outside the banner band (sparks orbit the name,
/// they never sit on it).
fn spark_position(ctx: &FxContext, rng: &mut XorShift32, band_y: i16, band_h: i16) -> (i16, i16) {
    let x = rng.range_i16(2, ctx.w - gfx::STAR_W as i16 - 2);
    let above = rng.below(2) == 0;
    let y = if above && band_y > gfx::STAR_H as i16 + 2 {
        rng.range_i16(2, band_y - gfx::STAR_H as i16 - 1)
    } else {
        let lo = band_y + band_h + 1;
        rng.range_i16(lo.min(ctx.h - 10), ctx.h - gfx::STAR_H as i16 - 1)
    };
    (x, y)
}

pub fn tick(ctx: &FxContext, now_ms: u64) {
    if now_ms.saturating_sub(LAST_MS.get()) < SPARK_STEP_MS {
        return;
    }
    LAST_MS.set(now_ms);
    let band_y = ctx.name_y() - BANNER_PAD;
    let band_h = ctx.name_h as i16 + 2 * BANNER_PAD;
    let mut rng = RNG.borrow_mut();
    let elem = SPARK_ELEMS[rng.below(SPARK_ELEMS.len() as u32) as usize];
    let (x, y) = spark_position(ctx, &mut rng, band_y, band_h);
    canvas::elem_set_offset(elem, x, y).ok();
    canvas::commit(false);
}

pub fn on_action(_ctx: &FxContext, _action_id: u32) {}
