//! \file
//! \brief FX6 "Glitch": hand-rolled state machine. Calm phases hold the name
//!        steady; PRNG-scheduled bursts jitter it and slap procedural noise
//!        bands over the panel; every few bursts a one-step invert flash hits
//!        (black panel, name in white).

use crate::cell::{PluginCell, PluginRef};
use crate::dither;
use crate::namefit::{self, FxContext};
use crate::rng::XorShift32;
use cdc_badge_plugin::{canvas, random};

const ELEM_BAND_A: u32 = 10;
const ELEM_BAND_B: u32 = 11;
const ELEM_FLASH: u32 = 12;

const GLITCH_STEP_MS: u64 = 300;
const BAND_ROWS: u16 = 6;
const JITTER_PX: i16 = 3;
/// Roughly every n-th burst escalates into the invert flash.
const FLASH_EVERY_N_BURSTS: u32 = 4;

#[derive(Clone, Copy, PartialEq)]
enum Phase {
    Calm(u8),
    Burst(u8),
    Flash,
}

static LAST_MS: PluginCell<u64> = PluginCell::new(0);
static PHASE: PluginCell<Phase> = PluginCell::new(Phase::Calm(4));
static BURSTS: PluginCell<u32> = PluginCell::new(0);
static RNG: PluginRef<XorShift32> = PluginRef::new(XorShift32::new(1));

pub fn enter(ctx: &FxContext, now_ms: u64) {
    LAST_MS.set(now_ms);
    PHASE.set(Phase::Calm(4));
    BURSTS.set(0);
    *RNG.borrow_mut() = XorShift32::new(random::u32().unwrap_or(now_ms as u32));

    namefit::record_name_elem(ctx, ctx.name_y());

    // Empty noise-band elements; bursts re-record them.
    for &elem in &[ELEM_BAND_A, ELEM_BAND_B] {
        canvas::elem_begin(elem).ok();
        canvas::elem_end();
        canvas::elem_show(elem, false).ok();
    }

    // The flash overlay: full black panel with the name in white, topmost,
    // hidden until a burst escalates.
    let name = namefit::NAME.borrow();
    canvas::elem_begin(ELEM_FLASH).ok();
    canvas::set_shade(canvas::SHADE_SOLID);
    canvas::draw_rect(0, 0, ctx.w, ctx.h, true);
    canvas::set_font(ctx.font).ok();
    canvas::set_text_size(1);
    canvas::set_text_inverted(true);
    canvas::draw_text_aligned(0, ctx.baseline_y(), ctx.w, &name, canvas::ALIGN_CENTER);
    canvas::set_text_inverted(false);
    canvas::elem_end();
    canvas::elem_set_z(ELEM_FLASH, 6).ok();
    canvas::elem_show(ELEM_FLASH, false).ok();
}

pub fn tick(ctx: &FxContext, now_ms: u64) {
    if now_ms.saturating_sub(LAST_MS.get()) < GLITCH_STEP_MS {
        return;
    }
    LAST_MS.set(now_ms);
    let mut rng = RNG.borrow_mut();

    match PHASE.get() {
        Phase::Calm(steps_left) => {
            if steps_left > 0 {
                PHASE.set(Phase::Calm(steps_left - 1));
                return; // hold still, no commit needed
            }
            PHASE.set(Phase::Burst(2 + (rng.below(2) as u8)));
        }
        Phase::Burst(steps_left) => {
            if steps_left == 0 {
                let bursts = BURSTS.get() + 1;
                BURSTS.set(bursts);
                if bursts.is_multiple_of(FLASH_EVERY_N_BURSTS) {
                    PHASE.set(Phase::Flash);
                    canvas::elem_show(ELEM_FLASH, true).ok();
                    canvas::commit(false);
                    return;
                }
                settle(ctx);
                PHASE.set(Phase::Calm(3 + (rng.below(5) as u8)));
                return;
            }
            PHASE.set(Phase::Burst(steps_left - 1));
            // Jitter the name and re-roll both noise bands.
            canvas::elem_set_offset(namefit::ELEM_NAME, rng.range_i16(-JITTER_PX, JITTER_PX), 0)
                .ok();
            for &elem in &[ELEM_BAND_A, ELEM_BAND_B] {
                let band = dither::noise_band(&mut rng, ctx.w as u16, BAND_ROWS);
                let y = rng.range_i16(0, ctx.h - BAND_ROWS as i16);
                canvas::elem_clear(elem).ok();
                canvas::elem_begin(elem).ok();
                canvas::draw_bitmap(0, y, ctx.w, BAND_ROWS as i16, &band);
                canvas::elem_end();
                canvas::elem_show(elem, true).ok();
            }
            canvas::commit(false);
        }
        Phase::Flash => {
            // The flash lasts exactly one step, then everything calms down.
            canvas::elem_show(ELEM_FLASH, false).ok();
            settle(ctx);
            PHASE.set(Phase::Calm(4 + (rng.below(4) as u8)));
        }
    }
}

/// Back to the clean reading state after a burst or flash.
fn settle(_ctx: &FxContext) {
    canvas::elem_set_offset(namefit::ELEM_NAME, 0, 0).ok();
    canvas::elem_show(ELEM_BAND_A, false).ok();
    canvas::elem_show(ELEM_BAND_B, false).ok();
    canvas::commit(false);
}

pub fn on_action(_ctx: &FxContext, _action_id: u32) {}
