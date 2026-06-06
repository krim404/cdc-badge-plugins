//! \file
//! \brief View rendering for the room list, context menu, system config and chat.

use crate::actions::*;
use crate::model::{Config, Room};
use cdc_badge_plugin::{canvas, i18n, ui};
use alloc::format;
use alloc::vec::Vec;

/// Builtin 6x8 font cell width (px) used to wrap chat lines to the body width.
const CHAR_W: usize = 6;
/// Per-line vertical advance (px) for the chat body.
const LINE_H: i16 = 10;

/// Render (or replace) the plugin-root room list.
pub fn render_room_list(rooms: &[Room], configured: bool) {
    let mut b = ui::ListBuilder::new(i18n::tr_meta("name"))
        .on_select(ACT_ROOM_SELECT)
        .on_menu(ACT_ROOM_MENU);
    if rooms.is_empty() {
        let key = if configured { "no_rooms" } else { "not_configured" };
        b = b.item(i18n::tr_key(key), 0, ui::UI_ICON_INFO);
    } else {
        // Display newest-first, but key each item by its storage index in `rooms`
        // so a tap maps back to the same room regardless of the sorted order.
        let mut order: Vec<usize> = (0..rooms.len()).collect();
        // Pending invitations float to the top (they need action), then newest.
        order.sort_by(|&a, &b| {
            rooms[b]
                .invited
                .cmp(&rooms[a].invited)
                .then(rooms[b].last_ts.cmp(&rooms[a].last_ts))
        });
        for &i in &order {
            let r = &rooms[i];
            // Pending invitations stand out first; then unread; then mark direct
            // chats with a person (smiley) so they read as a user, not a room;
            // else mark encrypted.
            let icon = if r.invited {
                ui::UI_ICON_ALERT
            } else if r.unread {
                ui::UI_ICON_INVERSE_BULLET
            } else if r.is_dm {
                ui::UI_ICON_SUCCESS
            } else if r.encrypted {
                ui::UI_ICON_CIRCLE
            } else {
                ui::UI_ICON_NONE
            };
            b = b.item(&r.name, i as u32, icon);
        }
    }
    b.replace();
    ui::set_footer(i18n::tr_key("hint_rooms"));
}

/// Open the `[3]` context menu. For a pending invitation it offers accept /
/// reject; otherwise join / new room / delete. System is always present.
pub fn open_room_menu(invited: bool) {
    let mut b = ui::ContextMenuBuilder::new(i18n::tr_meta("name")).on_select(ACT_ROOM_CTX_PICK);
    if invited {
        b = b
            .item(i18n::tr_key("menu_accept"), CTX_ACCEPT, ui::UI_ICON_SUCCESS)
            .item(i18n::tr_key("menu_reject"), CTX_REJECT, ui::UI_ICON_REMOVE);
    } else {
        b = b
            .item(i18n::tr_key("menu_join"), CTX_JOIN, ui::UI_ICON_ARROW_RIGHT)
            .item(i18n::tr_key("menu_create"), CTX_CREATE, ui::UI_ICON_PLAY)
            .item(i18n::tr_key("menu_delete"), CTX_DELETE, ui::UI_ICON_REMOVE);
    }
    b.item(i18n::tr_key("menu_system"), CTX_SYSTEM, ui::UI_ICON_TASK)
        .push();
}

/// Context menu over the open chat (key 3): invite a user to this room.
pub fn open_chat_menu() {
    ui::ContextMenuBuilder::new(i18n::tr_meta("name"))
        .on_select(ACT_ROOM_CTX_PICK)
        .item(i18n::tr_key("menu_invite"), CTX_INVITE, ui::UI_ICON_ARROW_RIGHT)
        .push();
}

/// Render the System config list. `replace` swaps the current system list in
/// place (after editing a field); `false` pushes it as a new screen.
pub fn render_system(cfg: &Config, replace: bool) {
    let user = if cfg.user.is_empty() { "-" } else { cfg.user.as_str() };
    let server = if cfg.server.is_empty() { "-" } else { cfg.server.as_str() };
    let b = ui::ListBuilder::new(i18n::tr_key("sys_title"))
        .on_select(ACT_SYS_SELECT)
        .item(format!("{}: {}", i18n::tr_key("sys_user"), user), SYS_USER, ui::UI_ICON_MALE)
        .item(format!("{}: ****", i18n::tr_key("sys_pass")), SYS_PASS, ui::UI_ICON_PARAGRAPH)
        .item(format!("{}: {}", i18n::tr_key("sys_server"), server), SYS_SRV, ui::UI_ICON_ANGLE)
        .item(format!("{}: ****", i18n::tr_key("sys_key")), SYS_KEY, ui::UI_ICON_DIAMOND)
        .item(i18n::tr_key("sys_login"), SYS_LOGIN, ui::UI_ICON_PLAY)
        .item(i18n::tr_key("sys_reset"), SYS_RESET, ui::UI_ICON_ALERT);
    if replace {
        b.replace();
    } else {
        b.push();
    }
    ui::set_footer(i18n::tr_key("hint_system"));
}

/// Open the editor for a System field. Returns `false` for non-field rows
/// (login / reset) which the caller handles directly.
pub fn open_field_editor(field: u32, cfg: &Config) -> bool {
    match field {
        SYS_USER => {
            ui::push_t9_input(i18n::tr_key("sys_user"), Some(&cfg.user), 128, ACT_SYS_USER_DONE);
            true
        }
        SYS_PASS => {
            let pass = crate::store::load_password().unwrap_or_default();
            ui::push_password(i18n::tr_key("sys_pass"), Some(&pass), 128, ACT_SYS_PASS_DONE);
            true
        }
        SYS_SRV => {
            ui::push_t9_input(i18n::tr_key("sys_server"), Some(&cfg.server), 200, ACT_SYS_SRV_DONE);
            true
        }
        SYS_KEY => {
            let key = crate::store::load_recovery_key().unwrap_or_default();
            ui::push_password(i18n::tr_key("sys_key"), Some(&key), 120, ACT_SYS_KEY_DONE);
            true
        }
        _ => false,
    }
}

/// Push a fresh canvas for the room's chat and draw its messages. The canvas
/// keeps the chat visible across key presses (unlike the info view, which pops
/// on any key), so the plugin owns Y (reply) / N (back) via the canvas key
/// callback [`ACT_CHAT_KEY`].
pub fn push_chat(room: &Room) {
    canvas::push(&room.name, ACT_CHAT_KEY, 0);
    canvas::set_footer(i18n::tr_key("hint_chat"));
    draw_chat(room, 0);
}

/// Redraw the chat body bottom-anchored (newest at the bottom), scrolled
/// `scroll` lines up from the newest. Returns `scroll` clamped to the available
/// history so the caller can store the effective value.
pub fn draw_chat(room: &Room, scroll: usize) -> usize {
    let (w, h) = canvas::body_size();
    let cols = ((w as usize) / CHAR_W).max(1);
    let rows = ((h as usize) / (LINE_H as usize)).max(1);

    // Wrap each message into display lines, oldest -> newest.
    let mut lines: Vec<alloc::string::String> = Vec::new();
    for m in &room.messages {
        let line = if m.encrypted {
            format!("{}: {}", m.sender, i18n::tr_key("decrypt_failed"))
        } else {
            format!("{}: {}", m.sender, m.body)
        };
        let chars: Vec<char> = line.chars().collect();
        if chars.is_empty() {
            lines.push(alloc::string::String::new());
        } else {
            for chunk in chars.chunks(cols) {
                lines.push(chunk.iter().collect());
            }
        }
    }

    canvas::clear();
    if lines.is_empty() {
        canvas::draw_text(0, 2, i18n::tr_key("no_messages"));
        canvas::commit(false);
        return 0;
    }

    let total = lines.len();
    let scroll = scroll.min(total.saturating_sub(rows));
    let end = total - scroll; // exclusive index of the newest visible line
    let start = end.saturating_sub(rows);
    let visible = &lines[start..end];

    let base = h as i16;
    for (i, line) in visible.iter().enumerate() {
        let y = base - ((visible.len() - i) as i16) * LINE_H;
        canvas::draw_text(0, y, line);
    }
    canvas::commit(false);
    scroll
}
