//! \file
//! \brief Raw FFI bindings: host functions imported from the WASM `cdc`
//!        module.
//!
//! Use the higher-level wrappers in the sibling modules instead of calling
//! these directly. Mirrors the C declarations in `sdk/host_api.h`.

use core::ffi::{c_char, c_int};

include!(concat!(env!("OUT_DIR"), "/host_api_consts.rs"));

#[repr(C)]
pub struct UiItem {
    pub label: *const c_char,
    pub icon: u8,
    pub icon_disabled: bool,
    pub item_id: u32,
}

#[repr(C)]
pub struct HostTm {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
    pub weekday: u8,
}

#[link(wasm_import_module = "cdc")]
extern "C" {
    // Logging
    pub fn host_log(level: u8, tag: *const c_char, msg: *const c_char);

    // Time
    pub fn host_uptime_ms() -> u64;
    pub fn host_unix_time() -> i64;
    pub fn host_local_time(out: *mut HostTm) -> c_int;
    pub fn host_is_time_set() -> bool;

    // Power
    pub fn host_battery_mv() -> u16;
    pub fn host_battery_pct() -> u8;
    pub fn host_is_usb_connected() -> bool;
    pub fn host_charge_status() -> u8;
    pub fn host_is_battery_low() -> bool;
    pub fn host_is_battery_critical() -> bool;

    // NVS
    pub fn host_nvs_get_blob(key: *const c_char, buf: *mut u8, len: *mut usize) -> c_int;
    pub fn host_nvs_set_blob(key: *const c_char, buf: *const u8, len: usize) -> c_int;
    pub fn host_nvs_get_str(key: *const c_char, buf: *mut c_char, buf_size: usize) -> c_int;
    pub fn host_nvs_set_str(key: *const c_char, value: *const c_char) -> c_int;
    pub fn host_nvs_get_u32(key: *const c_char, out: *mut u32) -> c_int;
    pub fn host_nvs_set_u32(key: *const c_char, value: u32) -> c_int;
    pub fn host_nvs_erase(key: *const c_char) -> c_int;

    // UI
    pub fn host_ui_push_toast(text: *const c_char, icon: u8, duration_ms: u16) -> c_int;
    pub fn host_ui_push_message(text: *const c_char, icon: u8, duration_ms: u32) -> c_int;
    pub fn host_ui_push_confirm(text: *const c_char, icon: u8, action_id: u32) -> c_int;
    pub fn host_ui_push_info(title: *const c_char, body: *const c_char) -> c_int;
    pub fn host_ui_push_list(
        title: *const c_char,
        items: *const UiItem,
        count: u16,
        select_action_id: u32,
        menu_action_id: u32,
    ) -> c_int;
    pub fn host_ui_pop() -> c_int;
    pub fn host_ui_pop_to_plugin() -> c_int;
    pub fn host_ui_repaint() -> c_int;
    pub fn host_ui_push_slider(
        title: *const c_char,
        min: i32,
        max: i32,
        init: i32,
        step: i32,
        unit: *const c_char,
        action_id: u32,
    ) -> c_int;
    pub fn host_ui_push_color_picker(r: u8, g: u8, b: u8, action_id: u32) -> c_int;
    pub fn host_ui_replace_list(
        title: *const c_char,
        items: *const UiItem,
        count: u16,
        select_action_id: u32,
        menu_action_id: u32,
    ) -> c_int;
    pub fn host_ui_set_view_footer(hint: *const c_char) -> c_int;
    pub fn host_ui_set_view_empty(text: *const c_char) -> c_int;
    pub fn host_ui_push_context_menu(
        title: *const c_char,
        items: *const UiItem,
        count: u16,
        select_action_id: u32,
    ) -> c_int;
    pub fn host_ui_push_t9_input(title: *const c_char, initial: *const c_char,
                                  max_len: u16, action_id: u32) -> c_int;
    pub fn host_ui_push_password(title: *const c_char, initial: *const c_char,
                                  max_len: u16, action_id: u32) -> c_int;
    pub fn host_ui_consume_input_int(out: *mut i32) -> c_int;
    pub fn host_ui_consume_input_text(out: *mut c_char, out_size: usize) -> c_int;

    // Plugin command channel
    pub fn host_cmd_consume(out: *mut c_char, out_size: usize) -> c_int;

    // Canvas view
    pub fn host_view_canvas_push(title: *const c_char,
                                  key_action_id: u32,
                                  widget_action_id: u32) -> c_int;
    pub fn host_view_canvas_get_body_size(w: *mut u16, h: *mut u16) -> c_int;
    pub fn host_view_canvas_set_footer(hint: *const c_char) -> c_int;
    pub fn host_view_canvas_clear() -> c_int;
    pub fn host_view_canvas_set_text_size(size: u8) -> c_int;
    pub fn host_view_canvas_set_text_color(inverted: bool) -> c_int;
    pub fn host_view_canvas_draw_text(x: i16, y: i16, text: *const c_char) -> c_int;
    pub fn host_view_canvas_draw_text_aligned(x: i16, y: i16, w: i16,
                                               text: *const c_char, align: u8) -> c_int;
    pub fn host_view_canvas_draw_rect(x: i16, y: i16, w: i16, h: i16, filled: bool) -> c_int;
    pub fn host_view_canvas_invert_rect(x: i16, y: i16, w: i16, h: i16) -> c_int;
    pub fn host_view_canvas_hline(x: i16, y: i16, w: i16) -> c_int;
    pub fn host_view_canvas_vline(x: i16, y: i16, h: i16) -> c_int;
    pub fn host_view_canvas_commit(full_refresh: bool) -> c_int;
    pub fn host_view_canvas_add_slider(widget_id: u32, min: i32, max: i32,
                                        initial: i32, step: i32) -> c_int;
    pub fn host_view_canvas_add_text(widget_id: u32, max_len: u16,
                                      initial: *const c_char) -> c_int;
    pub fn host_view_canvas_add_button(widget_id: u32) -> c_int;
    pub fn host_view_canvas_remove_widget(widget_id: u32) -> c_int;
    pub fn host_view_canvas_set_value(widget_id: u32, value: i32) -> c_int;
    pub fn host_view_canvas_get_value(widget_id: u32, out: *mut i32) -> c_int;
    pub fn host_view_canvas_set_text(widget_id: u32, text: *const c_char) -> c_int;
    pub fn host_view_canvas_get_text(widget_id: u32, out: *mut c_char, cap: usize) -> c_int;
    pub fn host_view_canvas_set_focus(widget_id: u32) -> c_int;
    pub fn host_view_canvas_get_focus(out: *mut u32) -> c_int;
    pub fn host_view_canvas_set_key_repeat(initial_ms: u16, repeat_ms: u16) -> c_int;

    // I18n
    pub fn host_i18n_tr_key(key: *const c_char, out: *mut c_char, out_cap: u32) -> c_int;
    pub fn host_i18n_tr_meta(field: *const c_char, out: *mut c_char, out_cap: u32) -> c_int;
    pub fn host_i18n_tr_core(key: *const c_char, out: *mut c_char, out_cap: u32) -> c_int;
    pub fn host_i18n_current_language() -> u8;

    // EventBus
    pub fn host_event_subscribe(event_mask: u32, action_id: u32) -> c_int;
    pub fn host_event_unsubscribe(subscription_id: u32) -> c_int;
    pub fn host_event_publish(module_event_subtype: u32, value: u32) -> c_int;

    // Strings
    pub fn host_str_to_display(
        input: *const c_char,
        out: *mut c_char,
        out_size: usize,
        target: u32,
    ) -> c_int;
}

/// Target codepage for host_str_to_display().
pub const HOST_STR_TARGET_CP437: u32 = 0;
pub const HOST_STR_TARGET_LATIN1: u32 = 1;
