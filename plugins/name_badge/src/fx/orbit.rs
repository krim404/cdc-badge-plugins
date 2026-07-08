//! \file
//! \brief FX7 "Orbit": a breathing dithered frame around the name (shade
//!        table, re-recorded per step) while two satellites circle it on the
//!        fixed-point orbit table; the name drifts on a slow yoyo. The white
//!        ink punches the frame ring open so it stays an outline.

use crate::cell::PluginCell;
use crate::engine::fx_action;
use crate::gfx;
use crate::namefit::{self, FxContext};
use cdc_badge_plugin::{anim, canvas};

const ELEM_FRAME: u32 = 10;
const ELEM_SAT_A: u32 = 11;
const ELEM_SAT_B: u32 = 12;
// Documented base for future completion actions of this effect.
const _ACT_UNUSED: u32 = fx_action(7, 0);

const PULSE_STEP_MS: u64 = 500;
/// Breathing intensity of the frame ring per step.
const SHADES: [u8; 6] = [64, 128, 192, 255, 192, 128];
const RING_THICKNESS: i16 = 4;
const FRAME_PAD: i16 = 10;

static LAST_MS: PluginCell<u64> = PluginCell::new(0);
static STEP: PluginCell<u32> = PluginCell::new(0);

pub fn enter(ctx: &FxContext, now_ms: u64) {
    LAST_MS.set(now_ms);
    STEP.set(0);

    namefit::record_name_elem(ctx, ctx.name_y());
    canvas::elem_set_z(namefit::ELEM_NAME, 3).ok();
    anim::Anim::element(namefit::ELEM_NAME)
        .to(3, 0)
        .duration_ms(2200)
        .ease(anim::Ease::QuadInOut)
        .yoyo()
        .repeat(anim::REPEAT_FOREVER)
        .start()
        .ok();

    record_frame(ctx, SHADES[0]);

    // Satellites: a diamond and a twinkle star on opposite orbit slots.
    canvas::elem_begin(ELEM_SAT_A).ok();
    canvas::draw_bitmap(0, 0, 8, 8, &gfx::DIAMOND8);
    canvas::elem_end();
    let star = gfx::star_sheet();
    canvas::elem_begin(ELEM_SAT_B).ok();
    canvas::draw_bitmap(0, 0, 8, 8, &star[16..24]); // burst frame
    canvas::elem_end();
    place_satellites(ctx, 0);
}

/// Frame ring: a dither-shaded filled rounded rect with its middle punched
/// back to white, leaving a breathing outline band.
fn record_frame(ctx: &FxContext, shade: u8) {
    let x = FRAME_PAD;
    let y = ctx.name_y() - FRAME_PAD;
    let w = ctx.w - 2 * FRAME_PAD;
    let h = ctx.name_h as i16 + 2 * FRAME_PAD;
    canvas::elem_clear(ELEM_FRAME).ok();
    canvas::elem_begin(ELEM_FRAME).ok();
    canvas::set_shade(shade);
    canvas::draw_round_rect(x, y, w, h, 8, true);
    canvas::set_shade(canvas::SHADE_SOLID);
    canvas::set_ink_white(true).ok();
    canvas::draw_round_rect(
        x + RING_THICKNESS,
        y + RING_THICKNESS,
        w - 2 * RING_THICKNESS,
        h - 2 * RING_THICKNESS,
        6,
        true,
    );
    canvas::set_ink_white(false).ok();
    canvas::elem_end();
    canvas::elem_set_z(ELEM_FRAME, -1).ok();
}

/// Both satellites ride the fixed-point circle, half a revolution apart.
fn place_satellites(ctx: &FxContext, step: u32) {
    let cx = ctx.w / 2 - 4;
    let cy = ctx.name_y() + ctx.name_h as i16 / 2 - 4;
    let rx = ctx.w / 2 - 12;
    let ry = (ctx.name_h as i16 + 2 * FRAME_PAD) / 2 + 8;
    let slot = step as usize;
    let (ax, ay) = gfx::orbit_offset(slot, rx, ry);
    let (bx, by) = gfx::orbit_offset(slot + gfx::ORBIT_12.len() / 2, rx, ry);
    canvas::elem_set_offset(ELEM_SAT_A, cx + ax, cy + ay).ok();
    canvas::elem_set_offset(ELEM_SAT_B, cx + bx, cy + by).ok();
}

pub fn tick(ctx: &FxContext, now_ms: u64) {
    if now_ms.saturating_sub(LAST_MS.get()) < PULSE_STEP_MS {
        return;
    }
    LAST_MS.set(now_ms);
    let step = STEP.get() + 1;
    STEP.set(step);
    record_frame(ctx, SHADES[step as usize % SHADES.len()]);
    place_satellites(ctx, step);
    canvas::commit(false);
}

pub fn on_action(_ctx: &FxContext, _action_id: u32) {}
