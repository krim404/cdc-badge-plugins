//! \file
//! \brief FX0 "Grand Entrance": pure framework showpiece. The name slides in,
//!        ornament rules follow from both sides, four corner brackets pop in
//!        as a chained sequence, then everything settles into a slow drift.

use crate::engine::fx_action;
use crate::namefit::{self, FxContext};
use cdc_badge_plugin::{anim, canvas};

const ELEM_RULE_TOP: u32 = 10;
const ELEM_RULE_BOT: u32 = 11;
const ELEM_CORNER: [u32; 4] = [12, 13, 14, 15];

const ACT_INTRO_DONE: u32 = fx_action(0, 0);

const RULE_GAP: i16 = 10;
const CORNER_LEN: i16 = 12;
const CORNER_MARGIN: i16 = 3;

pub fn enter(ctx: &FxContext, _now_ms: u64) {
    namefit::record_name_elem(ctx, ctx.name_y());
    anim::Anim::element(namefit::ELEM_NAME)
        .from(-ctx.w, 0)
        .to(0, 0)
        .duration_ms(900)
        .ease(anim::Ease::CubicOut)
        .start()
        .ok();

    // Ornament rules above and below the name, sliding in from either side.
    let rule_w = (ctx.shown_w() + 2 * RULE_GAP).min(ctx.w - 8);
    let rule_x = (ctx.w - rule_w) / 2;
    let top_y = ctx.name_y() - RULE_GAP;
    let bot_y = ctx.name_y() + ctx.name_h as i16 + RULE_GAP;

    canvas::elem_begin(ELEM_RULE_TOP).ok();
    canvas::hline(rule_x, top_y, rule_w);
    canvas::elem_end();
    anim::Anim::element(ELEM_RULE_TOP)
        .from(ctx.w, 0)
        .to(0, 0)
        .duration_ms(700)
        .delay_ms(250)
        .ease(anim::Ease::QuadOut)
        .start()
        .ok();

    canvas::elem_begin(ELEM_RULE_BOT).ok();
    canvas::hline(rule_x, bot_y, rule_w);
    canvas::elem_end();
    anim::Anim::element(ELEM_RULE_BOT)
        .from(-ctx.w, 0)
        .to(0, 0)
        .duration_ms(700)
        .delay_ms(400)
        .ease(anim::Ease::QuadOut)
        .start()
        .ok();

    // Corner brackets pop in one after another (chained, overshooting).
    let coords = [
        (CORNER_MARGIN, CORNER_MARGIN, 1i16, 1i16),
        (ctx.w - CORNER_MARGIN, CORNER_MARGIN, -1, 1),
        (ctx.w - CORNER_MARGIN, ctx.h - CORNER_MARGIN, -1, -1),
        (CORNER_MARGIN, ctx.h - CORNER_MARGIN, 1, -1),
    ];
    let mut prev = 0u32;
    for (i, &(cx, cy, dx, dy)) in coords.iter().enumerate() {
        canvas::elem_begin(ELEM_CORNER[i]).ok();
        canvas::hline(cx.min(cx + dx * CORNER_LEN), cy, CORNER_LEN);
        canvas::vline(cx, cy.min(cy + dy * CORNER_LEN), CORNER_LEN);
        canvas::elem_end();
        let mut a = anim::Anim::element(ELEM_CORNER[i])
            .from(dx * -18, dy * -18)
            .to(0, 0)
            .duration_ms(350)
            .ease(anim::Ease::Overshoot)
            .show_on_start();
        canvas::elem_show(ELEM_CORNER[i], false).ok();
        if prev != 0 {
            a = a.after(prev);
        }
        if i == coords.len() - 1 {
            a = a.on_done(ACT_INTRO_DONE);
        }
        prev = a.start().unwrap_or(0);
    }
}

pub fn tick(_ctx: &FxContext, _now_ms: u64) {}

pub fn on_action(_ctx: &FxContext, action_id: u32) {
    if action_id == ACT_INTRO_DONE {
        // Intro settled: keep the hero alive with an endless gentle drift.
        anim::Anim::element(namefit::ELEM_NAME)
            .to(0, 2)
            .duration_ms(1600)
            .ease(anim::Ease::QuadInOut)
            .yoyo()
            .repeat(anim::REPEAT_FOREVER)
            .start()
            .ok();
    }
}
