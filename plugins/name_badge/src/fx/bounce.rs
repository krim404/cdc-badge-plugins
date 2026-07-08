//! \file
//! \brief FX2 "Bounce Drop": the name drops with a bounce, kicks up a dust
//!        puff sprite on landing and settles into an elastic wobble; two
//!        ornament stars bounce in after it.

use crate::cell::PluginCell;
use crate::engine::fx_action;
use crate::gfx;
use crate::namefit::{self, FxContext};
use cdc_badge_plugin::{anim, canvas, sprite};

const ELEM_STAR_L: u32 = 10;
const ELEM_STAR_R: u32 = 11;
const ELEM_PUFF: u32 = 12;

const ACT_LANDED: u32 = fx_action(2, 0);
const ACT_PUFF_DONE: u32 = fx_action(2, 1);

const STAR_GAP: i16 = 14;

/// Puff sprite handle captured at creation (0 = none; canvas-owned).
static PUFF: PluginCell<u32> = PluginCell::new(0);

pub fn enter(ctx: &FxContext, _now_ms: u64) {
    PUFF.set(0);
    namefit::record_name_elem(ctx, ctx.name_y());
    anim::Anim::element(namefit::ELEM_NAME)
        .from(0, -ctx.h)
        .to(0, 0)
        .duration_ms(1200)
        .ease(anim::Ease::Bounce)
        .on_done(ACT_LANDED)
        .start()
        .ok();

    // Dust puff below the name, hidden until landing.
    if let Ok(p) = sprite::Sprite::from_sheet(
        gfx::PUFF_W,
        gfx::PUFF_H,
        gfx::PUFF_FRAMES,
        &gfx::puff_sheet(),
    ) {
        PUFF.set(p.handle());
        canvas::elem_begin(ELEM_PUFF).ok();
        canvas::draw_sprite(
            (ctx.w - gfx::PUFF_W as i16) / 2,
            ctx.name_y() + ctx.name_h as i16,
            p.handle(),
        )
        .ok();
        canvas::elem_end();
        canvas::elem_show(ELEM_PUFF, false).ok();
    }

    // Two stars flanking the name drop in late, also bouncing.
    let star_y = ctx.name_y() + (ctx.name_h as i16 - 8) / 2;
    let positions = [
        (ELEM_STAR_L, ctx.name_x() - STAR_GAP, 500u16),
        (
            ELEM_STAR_R,
            ctx.name_x() + ctx.shown_w() + STAR_GAP - 8,
            650,
        ),
    ];
    for &(elem, x, delay) in &positions {
        canvas::elem_begin(elem).ok();
        canvas::draw_bitmap(x, star_y, 8, 8, &gfx::DIAMOND8);
        canvas::elem_end();
        canvas::elem_show(elem, false).ok();
        anim::Anim::element(elem)
            .from(0, -ctx.h)
            .to(0, 0)
            .duration_ms(900)
            .delay_ms(delay)
            .ease(anim::Ease::Bounce)
            .show_on_start()
            .start()
            .ok();
    }
}

pub fn tick(_ctx: &FxContext, _now_ms: u64) {}

pub fn on_action(_ctx: &FxContext, action_id: u32) {
    match action_id {
        ACT_LANDED => {
            // Kick up the dust, then float on a springy elastic hover loop.
            let puff = PUFF.get();
            if puff != 0 {
                canvas::elem_show(ELEM_PUFF, true).ok();
                sprite::Sprite::from_handle(puff)
                    .play(sprite::Mode::Once, 180, 1, ACT_PUFF_DONE)
                    .ok();
            }
            anim::Anim::element(namefit::ELEM_NAME)
                .to(0, -6)
                .duration_ms(900)
                .ease(anim::Ease::Elastic)
                .yoyo()
                .repeat(anim::REPEAT_FOREVER)
                .start()
                .ok();
        }
        ACT_PUFF_DONE => {
            canvas::elem_show(ELEM_PUFF, false).ok();
            canvas::commit(false);
        }
        _ => {}
    }
}
