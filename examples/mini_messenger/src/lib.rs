//! \file
//! \brief Mini Messenger: the simplest possible exercise of the badge-to-badge
//!        message API. Sends a short text to a nearby badge and shows received
//!        texts. The plugin need not run in the background: because its manifest
//!        declares `message_types: ["text/plain"]`, the firmware auto-starts it
//!        when the user accepts an incoming text/plain transfer, then delivers
//!        the payload to the `ACT_RECEIVED` handler below.

#![no_std]
extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use cdc_badge_plugin::{i18n, log, msg, plugin_main, ui};

plugin_main!();

const TAG: &str = "msgr";

// Action ids routed back through plugin_on_action.
const ACT_MENU: u32 = 1; // main menu select
const ACT_COMPOSE: u32 = 2; // T9 compose confirmed/cancelled
const ACT_RECEIVED: u32 = 3; // inbound text/plain delivered

// Main-menu item ids (echoed back as user_data).
const ITEM_SEND: u32 = 0;
const ITEM_INBOX: u32 = 1;

const MAX_TEXT: u16 = 200;

// Foreground-only plugin running single-threaded in the WASM sandbox.
static mut INBOX: Vec<String> = Vec::new();

fn show_menu() {
    ui::ListBuilder::new(i18n::tr_meta("name"))
        .on_select(ACT_MENU)
        .item(i18n::tr_key("menu_send"), ITEM_SEND, ui::UI_ICON_PLAY)
        .item(i18n::tr_key("menu_inbox"), ITEM_INBOX, ui::UI_ICON_NOTES)
        .push();
}

fn show_inbox() {
    let messages: &Vec<String> = unsafe { &*core::ptr::addr_of!(INBOX) };
    if messages.is_empty() {
        ui::push_info(i18n::tr_key("menu_inbox"), i18n::tr_key("empty_inbox"));
        return;
    }
    let mut b = ui::ListBuilder::new(i18n::tr_key("menu_inbox"));
    for (i, m) in messages.iter().enumerate() {
        b = b.item(m.as_str(), i as u32, ui::UI_ICON_NOTES);
    }
    b.push();
}

#[no_mangle]
pub extern "C" fn plugin_init() -> i32 {
    // Register so incoming text/plain offers are not auto-declined.
    let _ = msg::register_handler("text/plain", ACT_RECEIVED);
    log::info(TAG, "init");
    0
}

#[no_mangle]
pub extern "C" fn plugin_deinit() -> i32 {
    let _ = msg::unregister_handler("text/plain");
    0
}

#[no_mangle]
pub extern "C" fn plugin_on_enter() -> i32 {
    show_menu();
    0
}

#[no_mangle]
pub extern "C" fn plugin_on_exit() -> i32 {
    0
}

#[no_mangle]
pub extern "C" fn plugin_on_action(action_id: u32, _idx: u32, user_data: u32) -> i32 {
    match action_id {
        ACT_MENU => {
            if user_data == ITEM_SEND {
                ui::push_t9_input(i18n::tr_key("compose"), None, MAX_TEXT, ACT_COMPOSE);
            } else {
                show_inbox();
            }
        }
        ACT_COMPOSE => {
            // user_data == 1 on confirm, 0 on cancel (the view popped itself).
            if user_data == 1 {
                if let Some(text) = ui::consume_input_text(MAX_TEXT as usize) {
                    // Remember the pairing for this session so a back-and-forth
                    // chat with the same badge only confirms the code once.
                    if msg::send_text_interactive_with(&text, msg::FLAG_PERSIST).is_err() {
                        ui::push_toast(i18n::tr_key("menu_send"), ui::UI_ICON_ERROR, 1500);
                    }
                }
            }
        }
        ACT_RECEIVED => {
            if let Some((_mime, text)) = msg::consume_text(msg::PAYLOAD_MAX) {
                unsafe {
                    (*core::ptr::addr_of_mut!(INBOX)).push(text.clone());
                }
                ui::push_info(i18n::tr_key("received"), &text);
            }
        }
        _ => {}
    }
    0
}
