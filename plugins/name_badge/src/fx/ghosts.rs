//! \file
//! \brief FX5 "Ghost Parade": two masked ghost sprites cross the panel on
//!        endless lanes, one behind and one in front of the name (z-order);
//!        completion actions respawn them on fresh PRNG lanes.

use crate::cell::{PluginCell, PluginRef};
use crate::engine::fx_action;
use crate::gfx;
use crate::namefit::{self, FxContext};
use crate::rng::XorShift32;
use cdc_badge_plugin::{anim, canvas, random, sprite};

const ELEM_GHOST_BACK: u32 = 10;
const ELEM_GHOST_FRONT: u32 = 11;

const ACT_BACK_DONE: u32 = fx_action(5, 0);
const ACT_FRONT_DONE: u32 = fx_action(5, 1);

const CROSS_BACK_MS: u16 = 9000;
const CROSS_FRONT_MS: u16 = 7000;

static RNG: PluginRef<XorShift32> = PluginRef::new(XorShift32::new(1));
static BACK_SPRITE: PluginCell<u32> = PluginCell::new(0);
static FRONT_SPRITE: PluginCell<u32> = PluginCell::new(0);
static BACK_RIGHTWARD: PluginCell<bool> = PluginCell::new(true);
static FRONT_RIGHTWARD: PluginCell<bool> = PluginCell::new(false);

pub fn enter(ctx: &FxContext, now_ms: u64) {
    *RNG.borrow_mut() = XorShift32::new(random::u32().unwrap_or(now_ms as u32));
    BACK_RIGHTWARD.set(true);
    FRONT_RIGHTWARD.set(false);

    namefit::record_name_elem(ctx, ctx.name_y());
    canvas::elem_set_z(namefit::ELEM_NAME, 3).ok();
    anim::Anim::element(namefit::ELEM_NAME)
        .to(0, 2)
        .duration_ms(1800)
        .ease(anim::Ease::QuadInOut)
        .yoyo()
        .repeat(anim::REPEAT_FOREVER)
        .start()
        .ok();

    let sheet = gfx::ghost_sheet();
    let mask = gfx::ghost_mask();
    for &(elem, z, cell) in &[
        (ELEM_GHOST_BACK, 1i8, &BACK_SPRITE),
        (ELEM_GHOST_FRONT, 5, &FRONT_SPRITE),
    ] {
        let Ok(s) =
            sprite::Sprite::from_sheet(gfx::GHOST_W, gfx::GHOST_H, gfx::GHOST_FRAMES, &sheet)
        else {
            continue;
        };
        s.set_mask(&mask).ok();
        s.play(sprite::Mode::Loop, 300, sprite::REPEAT_FOREVER, 0)
            .ok();
        cell.set(s.handle());
        // Recorded just off the left edge; tweens carry it across.
        canvas::elem_begin(elem).ok();
        canvas::draw_sprite(-(gfx::GHOST_W as i16), 0, s.handle()).ok();
        canvas::elem_end();
        canvas::elem_set_z(elem, z).ok();
    }

    launch(
        ctx,
        ELEM_GHOST_BACK,
        &BACK_SPRITE,
        true,
        CROSS_BACK_MS,
        ACT_BACK_DONE,
        0,
    );
    launch(
        ctx,
        ELEM_GHOST_FRONT,
        &FRONT_SPRITE,
        false,
        CROSS_FRONT_MS,
        ACT_FRONT_DONE,
        2000,
    );
}

/// Send a ghost across the panel on a fresh vertical lane.
fn launch(
    ctx: &FxContext,
    elem: u32,
    sprite_cell: &PluginCell<u32>,
    rightward: bool,
    cross_ms: u16,
    done: u32,
    delay: u16,
) {
    let lane = RNG
        .borrow_mut()
        .range_i16(2, ctx.h - gfx::GHOST_H as i16 - 2);
    let span = ctx.w + 2 * gfx::GHOST_W as i16;
    let (from_x, to_x) = if rightward { (0, span) } else { (span, 0) };

    let handle = sprite_cell.get();
    if handle != 0 {
        let flags = if rightward { 0 } else { sprite::FLAG_FLIP_H };
        sprite::Sprite::from_handle(handle).set_flags(flags).ok();
    }
    anim::Anim::element(elem)
        .from(from_x, lane)
        .to(to_x, lane)
        .duration_ms(cross_ms)
        .delay_ms(delay)
        .ease(anim::Ease::Linear)
        .on_done(done)
        .start()
        .ok();
}

pub fn tick(_ctx: &FxContext, _now_ms: u64) {}

pub fn on_action(ctx: &FxContext, action_id: u32) {
    match action_id {
        ACT_BACK_DONE => {
            let dir = !BACK_RIGHTWARD.get();
            BACK_RIGHTWARD.set(dir);
            launch(
                ctx,
                ELEM_GHOST_BACK,
                &BACK_SPRITE,
                dir,
                CROSS_BACK_MS,
                ACT_BACK_DONE,
                400,
            );
        }
        ACT_FRONT_DONE => {
            let dir = !FRONT_RIGHTWARD.get();
            FRONT_RIGHTWARD.set(dir);
            launch(
                ctx,
                ELEM_GHOST_FRONT,
                &FRONT_SPRITE,
                dir,
                CROSS_FRONT_MS,
                ACT_FRONT_DONE,
                800,
            );
        }
        _ => {}
    }
}
