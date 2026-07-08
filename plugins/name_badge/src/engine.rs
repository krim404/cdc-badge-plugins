//! \file
//! \brief Effect engine: id map, auto-rotation, switch procedure and the
//!        routing of tween/sprite completion actions to the active effect.

use crate::cell::PluginCell;
use crate::fx;
use crate::namefit::{self, FxContext};
use cdc_badge_plugin::{anim, canvas};

pub const FX_COUNT: u32 = 8;

/// Auto-rotation dwell per effect, ms (indexed like [`EffectId`]).
const FX_DURATION_MS: [u64; FX_COUNT as usize] = [
    18_000, // Entrance
    20_000, // Typewriter
    18_000, // Bounce
    25_000, // Sparkle
    20_000, // Wipe
    25_000, // Ghosts
    18_000, // Glitch
    25_000, // Orbit
];

/// Effect-local completion actions live at BASE + fx * STRIDE + n; the range
/// check in [`on_action`] drops stale events from a previous effect (sprite
/// done actions are not covered by `anim::cancel`).
pub const FX_ACTION_BASE: u32 = 100;
pub const FX_ACTION_STRIDE: u32 = 16;

/// Completion-action id `n` of effect `fx`.
pub const fn fx_action(fx: u32, n: u32) -> u32 {
    FX_ACTION_BASE + fx * FX_ACTION_STRIDE + n
}

pub static CURRENT_FX: PluginCell<u32> = PluginCell::new(0);
pub static AUTO_ROTATE: PluginCell<bool> = PluginCell::new(true);
static FX_STARTED_MS: PluginCell<u64> = PluginCell::new(0);

/// Tear down whatever ran before and start effect `fx` with a full refresh
/// (the ghosting reset between effects).
pub fn switch_effect(fx: u32, now_ms: u64) {
    let fx = fx % FX_COUNT;
    // Cancel all tweens first: the SDK guarantees no completion actions fire,
    // so nothing stale arrives while the new effect is being recorded.
    anim::cancel(0).ok();
    // Drops the display list, all elements and all sprites.
    canvas::clear();
    // Plugin-global resources not covered by canvas::clear.
    fx::typewriter::release_measure();
    CURRENT_FX.set(fx);
    FX_STARTED_MS.set(now_ms);
    let ctx = namefit::CTX.get();
    dispatch_enter(fx, &ctx, now_ms);
    canvas::commit(true);
}

pub fn next_effect(now_ms: u64) {
    switch_effect(CURRENT_FX.get() + 1, now_ms);
}

pub fn prev_effect(now_ms: u64) {
    switch_effect(CURRENT_FX.get() + FX_COUNT - 1, now_ms);
}

/// Called from `plugin_on_tick`: rotates when the dwell elapsed, otherwise
/// forwards the tick (each effect throttles itself).
pub fn tick(now_ms: u64) {
    let fx = CURRENT_FX.get();
    let dwell = FX_DURATION_MS[fx as usize];
    if AUTO_ROTATE.get() && now_ms.saturating_sub(FX_STARTED_MS.get()) >= dwell {
        switch_effect(fx + 1, now_ms);
        return;
    }
    let ctx = namefit::CTX.get();
    match fx {
        0 => fx::entrance::tick(&ctx, now_ms),
        1 => fx::typewriter::tick(&ctx, now_ms),
        2 => fx::bounce::tick(&ctx, now_ms),
        3 => fx::sparkle::tick(&ctx, now_ms),
        4 => fx::wipe::tick(&ctx, now_ms),
        5 => fx::ghosts::tick(&ctx, now_ms),
        6 => fx::glitch::tick(&ctx, now_ms),
        _ => fx::orbit::tick(&ctx, now_ms),
    }
}

/// Route an effect-range action to the CURRENT effect only.
pub fn on_action(action_id: u32) {
    if action_id < FX_ACTION_BASE {
        return;
    }
    let fx = (action_id - FX_ACTION_BASE) / FX_ACTION_STRIDE;
    if fx != CURRENT_FX.get() {
        return; // stale completion from a previous effect
    }
    let ctx = namefit::CTX.get();
    match fx {
        0 => fx::entrance::on_action(&ctx, action_id),
        1 => fx::typewriter::on_action(&ctx, action_id),
        2 => fx::bounce::on_action(&ctx, action_id),
        3 => fx::sparkle::on_action(&ctx, action_id),
        4 => fx::wipe::on_action(&ctx, action_id),
        5 => fx::ghosts::on_action(&ctx, action_id),
        6 => fx::glitch::on_action(&ctx, action_id),
        _ => fx::orbit::on_action(&ctx, action_id),
    }
}

fn dispatch_enter(fx: u32, ctx: &FxContext, now_ms: u64) {
    match fx {
        0 => fx::entrance::enter(ctx, now_ms),
        1 => fx::typewriter::enter(ctx, now_ms),
        2 => fx::bounce::enter(ctx, now_ms),
        3 => fx::sparkle::enter(ctx, now_ms),
        4 => fx::wipe::enter(ctx, now_ms),
        5 => fx::ghosts::enter(ctx, now_ms),
        6 => fx::glitch::enter(ctx, now_ms),
        _ => fx::orbit::enter(ctx, now_ms),
    }
}
