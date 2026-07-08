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

/// Mirrors `host_anim_t` in host_api.h.
#[repr(C)]
pub struct HostAnim {
    pub elem_id: u32,
    pub from_x: i16,
    pub from_y: i16,
    pub to_x: i16,
    pub to_y: i16,
    pub duration_ms: u16,
    pub delay_ms: u16,
    pub repeat: u16,
    pub easing: u8,
    pub flags: u8,
    pub done_action_id: u32,
    pub start_after: u32,
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
    pub fn host_log_hex(tag: *const c_char, label: *const c_char, data: *const u8, len: usize);

    // Time
    pub fn host_uptime_ms() -> u64;
    pub fn host_unix_time() -> i64;
    pub fn host_local_time(out: *mut HostTm) -> c_int;
    pub fn host_timezone_offset() -> i32;
    pub fn host_is_time_set() -> bool;

    // Power
    pub fn host_battery_mv() -> u16;
    pub fn host_battery_pct() -> u8;
    pub fn host_is_usb_connected() -> bool;
    pub fn host_power_source() -> u8;
    pub fn host_charge_status() -> u8;
    pub fn host_is_battery_low() -> bool;
    pub fn host_is_battery_critical() -> bool;
    pub fn host_set_sleep_inhibit(on: u32);

    // NVS
    pub fn host_nvs_get_blob(key: *const c_char, buf: *mut u8, buf_size: usize) -> c_int;
    pub fn host_nvs_set_blob(key: *const c_char, buf: *const u8, len: usize) -> c_int;
    pub fn host_nvs_get_str(key: *const c_char, buf: *mut c_char, buf_size: usize) -> c_int;
    pub fn host_nvs_set_str(key: *const c_char, value: *const c_char) -> c_int;
    pub fn host_nvs_get_u32(key: *const c_char, out: *mut u32) -> c_int;
    pub fn host_nvs_set_u32(key: *const c_char, value: u32) -> c_int;
    pub fn host_nvs_erase(key: *const c_char) -> c_int;
    pub fn host_nvs_erase_all() -> c_int;
    pub fn host_nvs_list_keys(out: *mut c_char, out_len: *mut usize) -> c_int;

    // vFAT (sandboxed plugin file storage)
    pub fn host_fs_write(name: *const c_char, data: *const u8, len: usize) -> c_int;
    pub fn host_fs_read(name: *const c_char, buf: *mut u8, buf_size: usize) -> c_int;
    pub fn host_fs_remove(name: *const c_char) -> c_int;
    pub fn host_fs_size(name: *const c_char) -> c_int;
    pub fn host_fs_list(out: *mut c_char, out_size: usize) -> c_int;
    pub fn host_fs_view(name: *const c_char) -> c_int;

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
    pub fn host_ui_set_view_lifecycle(hide_action_id: u32, show_action_id: u32) -> c_int;
    pub fn host_ui_update_list_item(index: u16, item: *const UiItem) -> c_int;
    pub fn host_ui_insert_list_item(index: u16, item: *const UiItem) -> c_int;
    pub fn host_ui_remove_list_item(index: u16) -> c_int;
    pub fn host_ui_push_context_menu(
        title: *const c_char,
        items: *const UiItem,
        count: u16,
        select_action_id: u32,
    ) -> c_int;
    pub fn host_ui_push_t9_input(
        title: *const c_char,
        initial: *const c_char,
        max_len: u16,
        action_id: u32,
    ) -> c_int;
    pub fn host_ui_push_password(
        title: *const c_char,
        initial: *const c_char,
        max_len: u16,
        action_id: u32,
    ) -> c_int;
    pub fn host_ui_consume_input_int(out: *mut i32) -> c_int;
    pub fn host_ui_consume_input_text(out: *mut c_char, out_size: usize) -> c_int;
    pub fn host_ui_push_date(title: *const c_char, d: u8, m: u8, y: u16, action_id: u32) -> c_int;
    pub fn host_ui_push_time(title: *const c_char, h: u8, m: u8, action_id: u32) -> c_int;
    pub fn host_ui_push_pin_entry(
        title: *const c_char,
        max_len: u8,
        max_attempts: u8,
        action_id: u32,
    ) -> c_int;
    pub fn host_ui_acquire_exclusive() -> c_int;
    pub fn host_ui_release_exclusive() -> c_int;
    pub fn host_ui_set_inactivity(timeout_ms: u32, action_id: u32) -> c_int;
    pub fn host_ui_wink(count: u8, period_ms: u16) -> c_int;

    // Plugin command channel
    pub fn host_cmd_consume(out: *mut c_char, out_size: usize) -> c_int;

    // Canvas view
    pub fn host_view_canvas_push(
        title: *const c_char,
        key_action_id: u32,
        widget_action_id: u32,
    ) -> c_int;
    pub fn host_view_canvas_get_body_size(w: *mut u16, h: *mut u16) -> c_int;
    pub fn host_view_canvas_set_footer(hint: *const c_char) -> c_int;
    pub fn host_view_canvas_clear() -> c_int;
    pub fn host_view_canvas_clear_ex(flags: u32) -> c_int;
    pub fn host_view_canvas_set_text_size(size: u8) -> c_int;
    pub fn host_view_canvas_set_font(font_id: u8) -> c_int;
    pub fn host_text_pick_font_that_fits(
        text: *const c_char,
        max_width_px: i16,
        candidates: *const u8,
        count: u32,
        out_font_id: *mut u8,
    ) -> c_int;
    pub fn host_view_canvas_set_text_color(inverted: bool) -> c_int;
    pub fn host_view_canvas_set_shade(shade: u8) -> c_int;
    pub fn host_view_canvas_draw_text(x: i16, y: i16, text: *const c_char) -> c_int;
    pub fn host_view_canvas_draw_text_aligned(
        x: i16,
        y: i16,
        w: i16,
        text: *const c_char,
        align: u8,
    ) -> c_int;
    pub fn host_view_canvas_draw_rect(x: i16, y: i16, w: i16, h: i16, filled: bool) -> c_int;
    pub fn host_view_canvas_draw_pixel(x: i16, y: i16) -> c_int;
    pub fn host_view_canvas_draw_line(x0: i16, y0: i16, x1: i16, y1: i16) -> c_int;
    pub fn host_view_canvas_draw_circle(x: i16, y: i16, r: i16, filled: bool) -> c_int;
    pub fn host_view_canvas_draw_triangle(
        x0: i16,
        y0: i16,
        x1: i16,
        y1: i16,
        x2: i16,
        y2: i16,
        filled: bool,
    ) -> c_int;
    pub fn host_view_canvas_draw_round_rect(
        x: i16,
        y: i16,
        w: i16,
        h: i16,
        r: i16,
        filled: bool,
    ) -> c_int;
    pub fn host_view_canvas_draw_bitmap(
        x: i16,
        y: i16,
        w: i16,
        h: i16,
        data: *const u8,
        len: usize,
    ) -> c_int;
    pub fn host_view_canvas_hline(x: i16, y: i16, w: i16) -> c_int;
    pub fn host_view_canvas_vline(x: i16, y: i16, h: i16) -> c_int;
    pub fn host_view_canvas_commit(full_refresh: bool) -> c_int;
    pub fn host_view_canvas_elem_begin(elem_id: u32) -> c_int;
    pub fn host_view_canvas_elem_end() -> c_int;
    pub fn host_view_canvas_elem_set_offset(elem_id: u32, ox: i16, oy: i16) -> c_int;
    pub fn host_view_canvas_elem_move(elem_id: u32, dx: i16, dy: i16) -> c_int;
    pub fn host_view_canvas_elem_show(elem_id: u32, visible: bool) -> c_int;
    pub fn host_view_canvas_elem_remove(elem_id: u32) -> c_int;
    pub fn host_view_canvas_elem_clear(elem_id: u32) -> c_int;
    pub fn host_view_canvas_elem_set_z(elem_id: u32, z: i8) -> c_int;
    pub fn host_view_canvas_elem_get_offset(elem_id: u32, ox: *mut i16, oy: *mut i16) -> c_int;
    pub fn host_view_canvas_elem_get_bounds(
        elem_id: u32,
        x: *mut i16,
        y: *mut i16,
        w: *mut u16,
        h: *mut u16,
    ) -> c_int;
    pub fn host_view_canvas_set_anim_policy(refresh_policy: u8, max_fps: u8) -> c_int;
    pub fn host_view_canvas_draw_sprite(x: i16, y: i16, sprite: u32) -> c_int;
    pub fn host_view_canvas_set_ink(white: bool) -> c_int;
    pub fn host_view_canvas_marquee(
        x: i16,
        y: i16,
        window_w: i16,
        text: *const c_char,
        step_px: u16,
        frame_ms: u16,
    ) -> c_int;

    // Sprites
    pub fn host_sprite_create(
        frame_w: u16,
        frame_h: u16,
        frame_count: u16,
        frames: *const u8,
        len: usize,
    ) -> c_int;
    pub fn host_sprite_create_from_surface(
        surface: u32,
        frame_w: u16,
        frame_h: u16,
        frame_count: u16,
    ) -> c_int;
    pub fn host_sprite_create_from_image(
        data: *const u8,
        len: usize,
        target_w: u16,
        frame_h: u16,
    ) -> c_int;
    pub fn host_sprite_set_mask(sprite: u32, mask: *const u8, len: usize) -> c_int;
    pub fn host_sprite_set_flags(sprite: u32, flags: u8) -> c_int;
    pub fn host_sprite_set_scale(sprite: u32, scale: u8) -> c_int;
    pub fn host_sprite_set_frame(sprite: u32, frame: u16) -> c_int;
    pub fn host_sprite_get_frame(sprite: u32, out: *mut u16) -> c_int;
    pub fn host_sprite_set_frame_durations(sprite: u32, ms: *const u16, count: u16) -> c_int;
    pub fn host_sprite_play(
        sprite: u32,
        mode: u8,
        frame_ms: u16,
        repeat: u16,
        done_action_id: u32,
    ) -> c_int;
    pub fn host_sprite_stop(sprite: u32) -> c_int;
    pub fn host_sprite_destroy(sprite: u32) -> c_int;
    pub fn host_surface_draw_sprite(surface: u32, x: i16, y: i16, sprite: u32) -> c_int;

    // Canvas tweens
    pub fn host_anim_start(cfg: *const HostAnim) -> c_int;
    pub fn host_anim_cancel(handle: u32) -> c_int;
    pub fn host_anim_pause(handle: u32, paused: bool) -> c_int;
    pub fn host_anim_state(handle: u32) -> c_int;
    pub fn host_anim_active_count() -> c_int;
    pub fn host_anim_blink(
        elem_id: u32,
        period_ms: u16,
        count: u16,
        done_action_id: u32,
    ) -> c_int;
    pub fn host_view_canvas_add_slider(
        widget_id: u32,
        min: i32,
        max: i32,
        initial: i32,
        step: i32,
    ) -> c_int;
    pub fn host_view_canvas_add_text(widget_id: u32, max_len: u16, initial: *const c_char)
        -> c_int;
    pub fn host_view_canvas_add_button(widget_id: u32) -> c_int;
    pub fn host_view_canvas_remove_widget(widget_id: u32) -> c_int;
    pub fn host_view_canvas_set_value(widget_id: u32, value: i32) -> c_int;
    pub fn host_view_canvas_get_value(widget_id: u32, out: *mut i32) -> c_int;
    pub fn host_view_canvas_set_text(widget_id: u32, text: *const c_char) -> c_int;
    pub fn host_view_canvas_get_text(widget_id: u32, out: *mut c_char, cap: usize) -> c_int;
    pub fn host_view_canvas_set_focus(widget_id: u32) -> c_int;
    pub fn host_view_canvas_get_focus(out: *mut u32) -> c_int;
    pub fn host_view_canvas_set_key_repeat(initial_ms: u16, repeat_ms: u16) -> c_int;
    pub fn host_view_canvas_set_long_press_action(action_id: u32) -> c_int;

    // I18n
    pub fn host_i18n_tr_key(key: *const c_char, out: *mut c_char, out_cap: u32) -> c_int;
    pub fn host_i18n_tr_meta(field: *const c_char, out: *mut c_char, out_cap: u32) -> c_int;
    pub fn host_i18n_tr_core(key: *const c_char, out: *mut c_char, out_cap: u32) -> c_int;
    pub fn host_i18n_current_language() -> u8;

    // EventBus
    pub fn host_event_subscribe(event_mask: u32, action_id: u32) -> c_int;
    pub fn host_event_unsubscribe(subscription_id: u32) -> c_int;
    pub fn host_event_publish(module_event_subtype: u32, value: u32) -> c_int;

    // Crypto / RNG
    pub fn host_random(buf: *mut u8, len: usize) -> c_int;
    pub fn host_random_strict(buf: *mut u8, len: usize) -> c_int;

    // Strings
    pub fn host_str_to_display(
        input: *const c_char,
        out: *mut c_char,
        out_size: usize,
        target: u32,
    ) -> c_int;
    pub fn host_str_to_utf8(input: *const c_char, out: *mut c_char, out_size: usize) -> c_int;
}

/// Target codepage for host_str_to_display().
pub const HOST_STR_TARGET_CP437: u32 = 0;
pub const HOST_STR_TARGET_LATIN1: u32 = 1;
