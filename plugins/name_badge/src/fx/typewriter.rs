//! \file
//! \brief FX1 "Typewriter": hand-rolled character stepping (UTF-8 safe) with
//!        a framework-blinked cursor block. Types the name, rests, restarts.

use crate::cell::{PluginCell, PluginRef};
use crate::engine::fx_action;
use crate::namefit::{self, FxContext};
use crate::textmath;
use cdc_badge_plugin::{anim, canvas, surface::Surface};

const ELEM_CURSOR: u32 = 10;
// Reserved for future use; keeps the action range documented.
const _ACT_UNUSED: u32 = fx_action(1, 0);

const TYPE_STEP_MS: u64 = 350;
const REST_MS: u64 = 2000;
const CURSOR_W: i16 = 3;

static CHARS: PluginCell<usize> = PluginCell::new(0);
static RESTING_SINCE: PluginCell<u64> = PluginCell::new(0);
static LAST_MS: PluginCell<u64> = PluginCell::new(0);
/// Measurement surface kept alive for the effect (one of the two allowed).
static MEASURE: PluginRef<Option<Surface>> = PluginRef::new(None);

pub fn enter(ctx: &FxContext, now_ms: u64) {
    CHARS.set(0);
    RESTING_SINCE.set(0);
    LAST_MS.set(now_ms);
    if let Ok(s) = Surface::create(8, 8) {
        let _ = s.set_font(ctx.font);
        *MEASURE.borrow_mut() = Some(s);
    }

    // Empty name element; tick re-records the growing prefix into it.
    canvas::elem_begin(namefit::ELEM_NAME).ok();
    canvas::elem_end();

    canvas::elem_begin(ELEM_CURSOR).ok();
    canvas::draw_rect(left_x(ctx), ctx.name_y(), CURSOR_W, ctx.name_h as i16, true);
    canvas::elem_end();
    anim::blink(ELEM_CURSOR, 500, anim::REPEAT_FOREVER, 0).ok();
}

/// Typing starts at the name's final left edge so the finished line sits
/// exactly where the other effects place it.
fn left_x(ctx: &FxContext) -> i16 {
    ctx.name_x()
}

pub fn tick(ctx: &FxContext, now_ms: u64) {
    if now_ms.saturating_sub(LAST_MS.get()) < TYPE_STEP_MS {
        return;
    }
    LAST_MS.set(now_ms);

    let name = namefit::NAME.borrow();
    let total = textmath::char_count(&name);

    if RESTING_SINCE.get() != 0 {
        if now_ms.saturating_sub(RESTING_SINCE.get()) >= REST_MS {
            RESTING_SINCE.set(0);
            CHARS.set(0); // wipe and retype
        } else {
            return;
        }
    } else if CHARS.get() >= total {
        RESTING_SINCE.set(now_ms);
        return;
    } else {
        CHARS.set(CHARS.get() + 1);
    }

    let prefix = &name[..textmath::prefix_len_bytes(&name, CHARS.get())];
    canvas::elem_clear(namefit::ELEM_NAME).ok();
    canvas::elem_begin(namefit::ELEM_NAME).ok();
    canvas::set_font(ctx.font).ok();
    canvas::set_text_size(1);
    canvas::set_text_inverted(false);
    canvas::draw_text(left_x(ctx), ctx.baseline_y(), prefix);
    canvas::elem_end();

    // Cursor trails the typed prefix; clamp inside the panel for marquee-wide
    // names (they type clipped, which reads as intentional overflow).
    let prefix_w = MEASURE
        .borrow()
        .as_ref()
        .and_then(|s| s.measure_text(prefix).ok())
        .map(|(w, _)| w as i16)
        .unwrap_or(0);
    let cursor_x = (prefix_w + 2).min(ctx.w - namefit::NAME_MARGIN - CURSOR_W - left_x(ctx));
    canvas::elem_set_offset(ELEM_CURSOR, cursor_x, 0).ok();
    canvas::commit(false);
}

pub fn on_action(_ctx: &FxContext, _action_id: u32) {}

/// Drop the measurement surface when another effect takes over (called from
/// its own enter via the fresh state reset; surfaces are plugin-global).
pub fn release_measure() {
    *MEASURE.borrow_mut() = None;
}
