//! \file
//! \brief WS2813 / SK6812 controller driving a strip on the Grove SIG0
//!        pin.
//!
//! Background plugin: ticks on every host frame to render rainbow / static
//! / blink / breathing effects. Settings persist via plugin NVS and the
//! lockscreen quick-action toggles the strip on / off.

#![no_std]

extern crate alloc;

use alloc::format;
use cdc_badge_plugin::{
    event, gpio, i18n, lockscreen, log, nvs, pixel_strip, plugin_main,
    ui::{self, ListBuilder, SliderBuilder},
};

plugin_main!();

const TAG: &str = "grove_led";
const DATA_PIN: u8 = gpio::pins::GROVE_0;
const MAX_LEDS: u16 = 100;
const DEFAULT_COUNT: u8 = 8;
const DEFAULT_BRIGHT: u8 = 64;
const FRAME_INTERVAL_MS: u64 = 20;

// EventType ordinals as seen by plugin_on_action when our event subscription
// fires. Match cdc::core::EventType in the firmware.
const ET_SYSTEM_SLEEP: u32 = 10;
const ET_SYSTEM_WAKE:  u32 = 11;

const NVS_ENABLED: &str = "enabled";
const NVS_COUNT:   &str = "count";
const NVS_BRIGHT:  &str = "bright";
const NVS_R:       &str = "color_r";
const NVS_G:       &str = "color_g";
const NVS_B:       &str = "color_b";
const NVS_EFFECT:  &str = "effect";
const NVS_SPEED:   &str = "speed";

const DEFAULT_SPEED: u8 = 50;

const ACT_TOP_SELECT: u32     = 1;
const ACT_COUNT_SAVE: u32     = 2;
const ACT_BRIGHT_SAVE: u32    = 3;
const ACT_COLOR_SAVE: u32     = 4;
const ACT_SPEED_SAVE: u32     = 5;
const ACT_EFFECT_SELECT: u32  = 7;
const ACT_EVENT: u32          = 8;
const ACT_LOCK_TOGGLE: u32    = 9;

const ITEM_TOGGLE: u32 = 0;
const ITEM_COUNT:  u32 = 1;
const ITEM_BRIGHT: u32 = 2;
const ITEM_COLOR:  u32 = 3;
const ITEM_EFFECT: u32 = 4;
const ITEM_SPEED:  u32 = 5;

/// \brief Selectable LED animation modes. Stored as `u8` in NVS.
#[derive(Copy, Clone, Eq, PartialEq)]
enum Effect {
    Rainbow = 0,
    Static = 1,
    Blink = 2,
    Breathing = 3,
    Random = 4,
}

impl Effect {
    fn from_u8(v: u8) -> Self {
        match v {
            1 => Effect::Static,
            2 => Effect::Blink,
            3 => Effect::Breathing,
            4 => Effect::Random,
            _ => Effect::Rainbow,
        }
    }
}

/// \brief Aggregated runtime state. A single instance lives in a
///        `static mut` because WAMR serialises every plugin call.
struct State {
    enabled: bool,
    count: u8,
    brightness: u8,
    r: u8,
    g: u8,
    b: u8,
    effect: Effect,
    speed: u8,
    rainbow_offset: u16,
    last_frame_ms: u64,
    sleeping: bool,
    dither_acc_r: u8,
    dither_acc_g: u8,
    dither_acc_b: u8,
}

impl State {
    const fn defaults() -> Self {
        Self {
            enabled: false,
            count: DEFAULT_COUNT,
            brightness: DEFAULT_BRIGHT,
            r: 255, g: 255, b: 255,
            effect: Effect::Rainbow,
            speed: DEFAULT_SPEED,
            rainbow_offset: 0,
            last_frame_ms: 0,
            sleeping: false,
            dither_acc_r: 0,
            dither_acc_g: 0,
            dither_acc_b: 0,
        }
    }
}

fn dim_dither(target_high_res: u16, acc: &mut u8) -> u8 {
    let target_lo = (target_high_res >> 8) as u8;
    let frac = (target_high_res & 0xFF) as u16;
    let (sum, overflow) = (*acc as u16).overflowing_add(frac as u16);
    if overflow || sum >= 256 {
        *acc = (sum & 0xFF) as u8;
        target_lo.saturating_add(1)
    } else {
        *acc = sum as u8;
        target_lo
    }
}

// Single-threaded WASM module: WAMR serialises all host calls and lifecycle
// hooks, so a plain `static mut` is sound here.
static mut STATE: State = State::defaults();

#[inline]
fn s() -> &'static mut State { unsafe { &mut *(&raw mut STATE) } }

// --- NVS ------------------------------------------------------------------

fn load_state() {
    if let Some(v) = nvs::get_u32(NVS_ENABLED) { s().enabled = v != 0; }
    if let Some(v) = nvs::get_u32(NVS_COUNT)   { s().count = clamp_count(v as u8); }
    if let Some(v) = nvs::get_u32(NVS_BRIGHT)  { s().brightness = v as u8; }
    if let Some(v) = nvs::get_u32(NVS_R)       { s().r = v as u8; }
    if let Some(v) = nvs::get_u32(NVS_G)       { s().g = v as u8; }
    if let Some(v) = nvs::get_u32(NVS_B)       { s().b = v as u8; }
    if let Some(v) = nvs::get_u32(NVS_EFFECT)  { s().effect = Effect::from_u8(v as u8); }
    if let Some(v) = nvs::get_u32(NVS_SPEED)   { s().speed = (v as u8).clamp(1, 100); }
}

fn phase_divisor() -> u64 {
    let speed = s().speed.max(1) as u64;
    (102 - speed).max(2)
}

/// \brief Milliseconds between re-randomisations; faster speed shortens the
///        interval.
fn random_interval() -> u64 {
    ((101 - s().speed as u64) * 6).max(1)
}

/// \brief xorshift64 step. `state` must be non-zero.
fn xorshift64(state: &mut u64) -> u64 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *state = x;
    x
}

fn clamp_count(v: u8) -> u8 {
    if v == 0 { 1 } else if v as u16 > MAX_LEDS { MAX_LEDS as u8 } else { v }
}

// --- Pixel rendering ------------------------------------------------------

fn apply_brightness(c: u8, brightness: u8) -> u8 {
    ((c as u16 * brightness as u16) / 255) as u8
}

fn hsv_to_rgb(h: u16, sat: u8, val: u8) -> (u8, u8, u8) {
    let region = (h / 60) % 6;
    let remainder = ((h % 60) as u32 * 255) / 60;
    let p = ((val as u32 * (255 - sat as u32)) / 255) as u8;
    let q = ((val as u32 * (255 - (sat as u32 * remainder) / 255)) / 255) as u8;
    let t = ((val as u32 * (255 - (sat as u32 * (255 - remainder)) / 255)) / 255) as u8;
    match region {
        0 => (val, t, p),
        1 => (q, val, p),
        2 => (p, val, t),
        3 => (p, q, val),
        4 => (t, p, val),
        _ => (val, p, q),
    }
}

/// \brief Triangle-wave "breathing" curve over a 0..=255 phase.
///
/// Pure integer maths so no libm dependency is needed.
/// \param phase Current phase position.
/// \return The amplitude at this phase.
fn breath_curve(phase: u8) -> u8 {
    if phase < 128 { phase.saturating_mul(2) } else { (255 - phase).saturating_mul(2) }
}

/// \brief Render one animation frame to the pixel strip.
/// \param uptime_ms Current uptime in milliseconds (drives time-based
///                  effects).
fn render_frame(uptime_ms: u64) {
    s().last_frame_ms = uptime_ms;
    if !s().enabled || s().count == 0 {
        let _ = pixel_strip::clear();
        let _ = pixel_strip::refresh();
        return;
    }
    let count = s().count as u16;
    let strip_len = pixel_strip::length();
    let max_idx = strip_len.min(count);
    let brightness = s().brightness;
    let (r, g, b) = (s().r, s().g, s().b);

    match s().effect {
        Effect::Rainbow => {
            let offset = s().rainbow_offset;
            for i in 0..max_idx {
                let hue = ((offset as u32 + (i as u32 * 360) / count as u32) % 360) as u16;
                let (rr, gg, bb) = hsv_to_rgb(hue, 255, brightness);
                let _ = pixel_strip::set(i, rr, gg, bb);
            }
            for i in max_idx..strip_len {
                let _ = pixel_strip::set(i, 0, 0, 0);
            }
            let rainbow_step = ((s().speed as u16 + 9) / 10).max(1);
            s().rainbow_offset = (offset + rainbow_step) % 360;
        }
        Effect::Static => {
            let rr = apply_brightness(r, brightness);
            let gg = apply_brightness(g, brightness);
            let bb = apply_brightness(b, brightness);
            for i in 0..max_idx { let _ = pixel_strip::set(i, rr, gg, bb); }
            for i in max_idx..strip_len { let _ = pixel_strip::set(i, 0, 0, 0); }
        }
        Effect::Blink => {
            let on = (uptime_ms / 500) % 2 == 0;
            let (rr, gg, bb) = if on {
                (apply_brightness(r, brightness),
                 apply_brightness(g, brightness),
                 apply_brightness(b, brightness))
            } else {
                (0, 0, 0)
            };
            for i in 0..max_idx { let _ = pixel_strip::set(i, rr, gg, bb); }
            for i in max_idx..strip_len { let _ = pixel_strip::set(i, 0, 0, 0); }
        }
        Effect::Breathing => {
            let phase = ((uptime_ms / phase_divisor()) % 256) as u8;
            let curve = breath_curve(phase);
            let rr_hi = ((r as u32 * curve as u32 * brightness as u32) / 255) as u16;
            let gg_hi = ((g as u32 * curve as u32 * brightness as u32) / 255) as u16;
            let bb_hi = ((b as u32 * curve as u32 * brightness as u32) / 255) as u16;
            let rr = dim_dither(rr_hi, &mut s().dither_acc_r);
            let gg = dim_dither(gg_hi, &mut s().dither_acc_g);
            let bb = dim_dither(bb_hi, &mut s().dither_acc_b);
            for i in 0..max_idx { let _ = pixel_strip::set(i, rr, gg, bb); }
            for i in max_idx..strip_len { let _ = pixel_strip::set(i, 0, 0, 0); }
        }
        Effect::Random => {
            let mut rng = (uptime_ms / random_interval()) ^ 0x9E3779B97F4A7C15;
            if rng == 0 { rng = 0x9E3779B97F4A7C15; }
            for i in 0..max_idx {
                let v = xorshift64(&mut rng);
                let (rr, gg, bb) = if v & 0b11 == 0 {
                    (0, 0, 0)
                } else {
                    hsv_to_rgb(((v >> 8) % 360) as u16, 255, brightness)
                };
                let _ = pixel_strip::set(i, rr, gg, bb);
            }
            for i in max_idx..strip_len { let _ = pixel_strip::set(i, 0, 0, 0); }
        }
    }
    let _ = pixel_strip::refresh();
}

// --- Menus ----------------------------------------------------------------

fn show_top_menu() {
    let toggle_label = format!(
        "{}: {}",
        i18n::tr_key("menu_enable"),
        if s().enabled { i18n::tr_key("on") } else { i18n::tr_key("off") },
    );

    ListBuilder::new(i18n::tr_meta("name"))
        .on_select(ACT_TOP_SELECT)
        .item(&toggle_label, ITEM_TOGGLE, if s().enabled { ui::UI_ICON_SUCCESS } else { ui::UI_ICON_CIRCLE })
        .item(i18n::tr_key("menu_count"),  ITEM_COUNT,  ui::UI_ICON_BAR)
        .item(i18n::tr_key("menu_bright"), ITEM_BRIGHT, ui::UI_ICON_SUN)
        .item(i18n::tr_key("menu_color"),  ITEM_COLOR,  ui::UI_ICON_DIAMOND)
        .item(i18n::tr_key("menu_effect"), ITEM_EFFECT, ui::UI_ICON_NOTES)
        .item(i18n::tr_key("menu_speed"),  ITEM_SPEED,  ui::UI_ICON_LEFTRIGHT)
        .replace();
}

fn show_speed_slider() {
    SliderBuilder::new(i18n::tr_key("menu_speed"))
        .range(1, 100).initial(s().speed as i32).step(5)
        .unit("%")
        .on_save(ACT_SPEED_SAVE).push();
}

fn show_count_slider() {
    SliderBuilder::new(i18n::tr_key("menu_count"))
        .range(1, MAX_LEDS as i32).initial(s().count as i32).step(1)
        .on_save(ACT_COUNT_SAVE).push();
}

fn show_bright_slider() {
    SliderBuilder::new(i18n::tr_key("menu_bright"))
        .range(0, 255).initial(s().brightness as i32).step(5)
        .on_save(ACT_BRIGHT_SAVE).push();
}

fn show_color_picker() {
    ui::push_color_picker(s().r, s().g, s().b, ACT_COLOR_SAVE);
}

fn show_effect_menu() {
    ListBuilder::new(i18n::tr_key("menu_effect"))
        .on_select(ACT_EFFECT_SELECT)
        .item(i18n::tr_key("effect_rainbow"),   0, ui::UI_ICON_DIAMOND)
        .item(i18n::tr_key("effect_static"),    1, ui::UI_ICON_CIRCLE)
        .item(i18n::tr_key("effect_blink"),     2, ui::UI_ICON_ALERT)
        .item(i18n::tr_key("effect_breathing"), 3, ui::UI_ICON_UPDOWN)
        .item(i18n::tr_key("effect_random"),    4, ui::UI_ICON_ALERT)
        .push();
}

fn toast_saved() {
    ui::push_toast(i18n::tr_key("saved"), ui::UI_ICON_SUCCESS, 600);
}

// --- Lifecycle ------------------------------------------------------------

/// \brief Lifecycle hook fired once when the plugin is loaded.
///
/// Restores state from NVS, initialises the pixel strip, subscribes to
/// sleep/wake events and registers the lockscreen quick action.
/// \return `0` on success, `-1` if the strip cannot be initialised.
#[no_mangle]
pub extern "C" fn plugin_init() -> i32 {
    load_state();
    if pixel_strip::init(DATA_PIN, s().count as u16, pixel_strip::Format::Grb).is_err() {
        log::error(TAG, "pixel_strip::init failed");
        return -1;
    }
    event::subscribe(event::SYSTEM_SLEEP | event::SYSTEM_WAKE, ACT_EVENT);
    lockscreen::register("menu_enable", ACT_LOCK_TOGGLE);
    log::info(TAG, "grove_led initialised");
    0
}

/// \brief Lifecycle hook fired once when the plugin is unloaded.
///
/// Clears the strip and releases the RMT handle.
/// \return `0` on success.
#[no_mangle]
pub extern "C" fn plugin_deinit() -> i32 {
    let _ = pixel_strip::clear();
    let _ = pixel_strip::refresh();
    let _ = pixel_strip::deinit();
    0
}

/// \brief Lifecycle hook fired every time the user opens the plugin.
///
/// Renders the top settings menu.
/// \return `0` on success.
#[no_mangle]
pub extern "C" fn plugin_on_enter() -> i32 {
    show_top_menu();
    0
}

/// \brief Lifecycle hook fired when the user leaves the plugin view.
/// \return `0` on success.
#[no_mangle]
pub extern "C" fn plugin_on_exit() -> i32 {
    0
}

/// \brief Background tick used to advance the animation.
///
/// Skipped while the badge is asleep and rate-limited to one frame per
/// `FRAME_INTERVAL_MS` milliseconds.
/// \param uptime_ms Current uptime in milliseconds.
/// \return `0` on success.
#[no_mangle]
pub extern "C" fn plugin_on_tick(uptime_ms: u64) -> i32 {
    if s().sleeping { return 0; }
    if uptime_ms.saturating_sub(s().last_frame_ms) < FRAME_INTERVAL_MS { return 0; }
    render_frame(uptime_ms);
    0
}

/// \brief Action dispatch for menu items, sliders, events and lockscreen.
/// \param action_id  Identifier set when pushing the originating view.
/// \param idx        Item index (for lists) or event type (for ACT_EVENT).
/// \param _user_data Unused.
/// \return `0` on success.
#[no_mangle]
pub extern "C" fn plugin_on_action(action_id: u32, idx: u32, _user_data: u32) -> i32 {
    match action_id {
        ACT_LOCK_TOGGLE => {
            s().enabled = !s().enabled;
            nvs::set_u32(NVS_ENABLED, s().enabled as u32);
            if !s().enabled {
                let _ = pixel_strip::clear();
                let _ = pixel_strip::refresh();
            }
        }
        ACT_EVENT => match idx {
            ET_SYSTEM_SLEEP => {
                s().sleeping = true;
                let _ = pixel_strip::clear();
                let _ = pixel_strip::refresh();
            }
            ET_SYSTEM_WAKE => { s().sleeping = false; }
            _ => {}
        },
        ACT_TOP_SELECT => match idx {
            ITEM_TOGGLE => {
                s().enabled = !s().enabled;
                nvs::set_u32(NVS_ENABLED, s().enabled as u32);
                if !s().enabled {
                    let _ = pixel_strip::clear();
                    let _ = pixel_strip::refresh();
                }
                show_top_menu();
            }
            ITEM_COUNT  => show_count_slider(),
            ITEM_BRIGHT => show_bright_slider(),
            ITEM_COLOR  => show_color_picker(),
            ITEM_EFFECT => show_effect_menu(),
            ITEM_SPEED  => show_speed_slider(),
            _ => {}
        },
        ACT_COUNT_SAVE => {
            if let Some(v) = ui::consume_input_int() {
                s().count = clamp_count(v.clamp(1, MAX_LEDS as i32) as u8);
                nvs::set_u32(NVS_COUNT, s().count as u32);
                let _ = pixel_strip::init(DATA_PIN, s().count as u16, pixel_strip::Format::Grb);
                toast_saved();
            }
        }
        ACT_BRIGHT_SAVE => {
            if let Some(v) = ui::consume_input_int() {
                s().brightness = v.clamp(0, 255) as u8;
                nvs::set_u32(NVS_BRIGHT, s().brightness as u32);
                toast_saved();
            }
        }
        ACT_COLOR_SAVE => {
            if let Some(packed) = ui::consume_input_int() {
                let p = packed as u32;
                s().r = ((p >> 16) & 0xFF) as u8;
                s().g = ((p >> 8) & 0xFF) as u8;
                s().b = (p & 0xFF) as u8;
                s().effect = Effect::Static;
                nvs::set_u32(NVS_R, s().r as u32);
                nvs::set_u32(NVS_G, s().g as u32);
                nvs::set_u32(NVS_B, s().b as u32);
                nvs::set_u32(NVS_EFFECT, s().effect as u32);
                toast_saved();
            }
        }
        ACT_EFFECT_SELECT => {
            s().effect = Effect::from_u8(idx as u8);
            nvs::set_u32(NVS_EFFECT, s().effect as u32);
            toast_saved();
        }
        ACT_SPEED_SAVE => {
            if let Some(v) = ui::consume_input_int() {
                s().speed = (v as u8).clamp(1, 100);
                nvs::set_u32(NVS_SPEED, s().speed as u32);
                toast_saved();
            }
        }
        _ => {}
    }
    0
}
