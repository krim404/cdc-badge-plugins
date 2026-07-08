//! \file
//! \brief Plugin lifecycle: canvas setup, key routing, name editing and the
//!        context menu. Everything animated lives in `engine`/`fx`.

use crate::cell::PluginCell;
use crate::engine;
use crate::namefit;
use alloc::string::String;
use cdc_badge_plugin::{canvas, display, i18n, nvs, plugin_main, ui};

plugin_main!();

/// Canvas key events (user_data = ASCII key code).
const ACT_KEY: u32 = 1;
/// Context-menu selection (user_data = item id).
const ACT_MENU: u32 = 2;
/// T9 name editor result (user_data 1 = confirm, 0 = cancel).
const ACT_NAME_INPUT: u32 = 3;

const ITEM_SET_NAME: u32 = 1;
const ITEM_TOGGLE_AUTO: u32 = 2;

const NVS_NAME: &str = "name";
const NVS_FX: &str = "fx";
const NVS_AUTO: &str = "auto";
pub const NAME_MAX_CHARS: u16 = 32;

/// Backlight range and per-keypress step (transient, never persisted to NVS).
const BACKLIGHT_MAX: u16 = 1023;
const BACKLIGHT_STEP: u16 = 128;

static UPTIME_MS: PluginCell<u64> = PluginCell::new(0);
/// Live backlight level, seeded from the host on enter; keys 4/6 step it.
static BACKLIGHT: PluginCell<u16> = PluginCell::new(0);

/// The stored name, or empty when never set.
fn load_name() -> String {
    nvs::get_str(NVS_NAME, NAME_MAX_CHARS as usize * 4)
        .unwrap_or_default()
        .trim()
        .into()
}

/// Apply a (possibly empty) name: empty falls back to the i18n placeholder
/// so the badge is never blank.
fn apply_name(name: String) {
    let shown = if name.is_empty() {
        String::from(i18n::tr_key("placeholder"))
    } else {
        name
    };
    namefit::CTX.set(namefit::build_context(&shown));
    *namefit::NAME.borrow_mut() = shown;
}

#[no_mangle]
pub extern "C" fn plugin_init() -> i32 {
    0
}

#[no_mangle]
pub extern "C" fn plugin_deinit() -> i32 {
    0
}

#[no_mangle]
pub extern "C" fn plugin_on_enter() -> i32 {
    canvas::push("", ACT_KEY, 0);
    canvas::set_anim_policy(canvas::ANIM_REFRESH_AUTO, 4).ok();
    // Seed the backlight step baseline from the current live level so the
    // first 4/6 press adjusts from where the badge already is.
    BACKLIGHT.set(display::backlight());

    let name = load_name();
    let first_run = name.is_empty();
    apply_name(name);
    engine::AUTO_ROTATE.set(nvs::get_u32(NVS_AUTO).unwrap_or(1) != 0);
    engine::switch_effect(nvs::get_u32(NVS_FX).unwrap_or(0), UPTIME_MS.get());

    if first_run {
        ui::push_t9_input(
            i18n::tr_key("name_prompt"),
            None,
            NAME_MAX_CHARS,
            ACT_NAME_INPUT,
        );
    }
    0
}

#[no_mangle]
pub extern "C" fn plugin_on_exit() -> i32 {
    0
}

#[no_mangle]
pub extern "C" fn plugin_on_tick(uptime_ms: u64) -> i32 {
    UPTIME_MS.set(uptime_ms);
    engine::tick(uptime_ms);
    0
}

fn open_menu() {
    let auto_label = if engine::AUTO_ROTATE.get() {
        i18n::tr_key("menu_auto_on")
    } else {
        i18n::tr_key("menu_auto_off")
    };
    ui::ContextMenuBuilder::new(i18n::tr_key("menu_title"))
        .on_select(ACT_MENU)
        .item(i18n::tr_key("menu_set_name"), ITEM_SET_NAME, 0)
        .item(auto_label, ITEM_TOGGLE_AUTO, 0)
        .push();
}

/// Step the (transient) backlight one notch and apply it live.
fn step_backlight(brighter: bool) {
    let next = if brighter {
        BACKLIGHT.get().saturating_add(BACKLIGHT_STEP).min(BACKLIGHT_MAX)
    } else {
        BACKLIGHT.get().saturating_sub(BACKLIGHT_STEP)
    };
    BACKLIGHT.set(next);
    display::set_backlight(next).ok();
}

fn on_key(key: u8) {
    let now = UPTIME_MS.get();
    match key {
        // 2/8 (and Y) step through the effects.
        b'8' | b'Y' => {
            engine::next_effect(now);
            nvs::set_u32(NVS_FX, engine::CURRENT_FX.get()).ok();
        }
        b'2' => {
            engine::prev_effect(now);
            nvs::set_u32(NVS_FX, engine::CURRENT_FX.get()).ok();
        }
        // 4/6 dim/brighten the backlight (live, not persisted).
        b'6' => step_backlight(true),
        b'4' => step_backlight(false),
        b'3' => open_menu(),
        b'N' => {
            ui::pop(); // pops the canvas; menu/T9 pop themselves
        }
        _ => {}
    }
}

fn on_menu_select(item: u32) {
    match item {
        ITEM_SET_NAME => {
            let current = namefit::NAME.borrow().clone();
            ui::push_t9_input(
                i18n::tr_key("name_prompt"),
                Some(&current),
                NAME_MAX_CHARS,
                ACT_NAME_INPUT,
            );
        }
        ITEM_TOGGLE_AUTO => {
            let auto = !engine::AUTO_ROTATE.get();
            engine::AUTO_ROTATE.set(auto);
            nvs::set_u32(NVS_AUTO, auto as u32).ok();
            let msg = i18n::tr_key(if auto { "auto_on" } else { "auto_off" });
            ui::push_toast(msg, ui::UI_ICON_SUCCESS, 1500);
        }
        _ => {}
    }
}

fn on_name_input(confirmed: bool) {
    if confirmed {
        if let Some(text) = ui::consume_input_text(NAME_MAX_CHARS as usize) {
            let trimmed: String = text.trim().into();
            nvs::set_str(NVS_NAME, &trimmed).ok();
            apply_name(trimmed);
        }
    }
    // Restart the running effect so the new (or unchanged) name lays out
    // freshly, with the full refresh the switch always does.
    engine::switch_effect(engine::CURRENT_FX.get(), UPTIME_MS.get());
}

#[no_mangle]
pub extern "C" fn plugin_on_action(action_id: u32, _idx: u32, user_data: u32) -> i32 {
    match action_id {
        ACT_KEY => on_key(user_data as u8),
        ACT_MENU => on_menu_select(user_data),
        ACT_NAME_INPUT => on_name_input(user_data == 1),
        _ => engine::on_action(action_id),
    }
    0
}
