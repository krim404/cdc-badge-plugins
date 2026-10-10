//! \file
//! \brief Print Counter - a demo tying together NVS persistence, a T9-edited
//!        setting, an HTTP GET and the `thermo_print` external feature.
//!
//! The single view offers "Print #<n>". Selecting it brings WiFi up if needed,
//! fetches the configured URL with `;N` replaced by the current number, and
//! hands the response text to the printer plugin. The counter advances once
//! that plugin reports the print as done.

#![no_std]

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use cdc_badge_plugin::{feature, http, i18n, lifecycle, log, nvs, plugin_main, ui, wifi, Error};

plugin_main!();

const TAG: &str = "print_counter";

const NVS_URL: &str = "url";
const NVS_NUM: &str = "num";

/// Replaced by the current counter value in the configured URL.
const PLACEHOLDER: &str = ";N";
const URL_MAX: u16 = 256;
const WIFI_TIMEOUT_MS: u32 = 15_000;
const HTTP_TIMEOUT_MS: u32 = 10_000;

const ACT_SELECT: u32 = 1;
const ACT_URL_DONE: u32 = 2;
const ACT_STATUS: u32 = 3;

const ITEM_PRINT: u32 = 0;
const ITEM_URL: u32 = 1;

struct State {
    num: u32,
    url: String,
    /// A job was handed to the printer and its status is still pending.
    job_pending: bool,
}

static mut STATE: State = State {
    num: 0,
    url: String::new(),
    job_pending: false,
};

/// WAMR serialises every call into the plugin, so a single mutable state is safe.
#[inline]
fn s() -> &'static mut State {
    unsafe { &mut *core::ptr::addr_of_mut!(STATE) }
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
    s().num = nvs::get_u32(NVS_NUM).unwrap_or(0);
    s().url = nvs::get_str(NVS_URL, URL_MAX as usize).unwrap_or_default();
    render();
    0
}

#[no_mangle]
pub extern "C" fn plugin_on_exit() -> i32 {
    0
}

#[no_mangle]
pub extern "C" fn plugin_on_action(action_id: u32, _idx: u32, user_data: u32) -> i32 {
    match action_id {
        ACT_SELECT => match user_data {
            ITEM_PRINT => do_print(),
            ITEM_URL => edit_url(),
            _ => {}
        },
        ACT_URL_DONE => {
            // The T9 view pops itself; user_data 1 = confirmed (cancel fires
            // no action).
            if user_data == 1 {
                if let Some(text) = ui::consume_input_text(URL_MAX as usize) {
                    s().url = text.trim().into();
                    let _ = nvs::set_str(NVS_URL, &s().url);
                }
            }
            render();
        }
        ACT_STATUS => {
            // Fired while the printer plugin is in the foreground, so this only
            // persists; plugin_on_enter redraws the list on the way back.
            if !s().job_pending {
                return 0;
            }
            job_finished();
            if user_data == feature::STATUS_DONE as u32 {
                s().num = s().num.wrapping_add(1);
                let _ = nvs::set_u32(NVS_NUM, s().num);
            } else {
                log::warn(TAG, &format!("print job failed with status {}", user_data));
            }
        }
        _ => {}
    }
    0
}

fn render() {
    ui::ListBuilder::new(i18n::tr_meta("name"))
        .on_select(ACT_SELECT)
        .item(
            format!("{}{}", i18n::tr_key("print_label"), s().num),
            ITEM_PRINT,
            ui::UI_ICON_PLAY,
        )
        .item(i18n::tr_key("menu_url"), ITEM_URL, ui::UI_ICON_PARAGRAPH)
        .replace();
}

fn edit_url() {
    let initial = if s().url.is_empty() {
        None
    } else {
        Some(s().url.as_str())
    };
    ui::push_t9_input(i18n::tr_key("url_title"), initial, URL_MAX, ACT_URL_DONE);
}

/// Release the WiFi reservation and the residency held for a print job.
fn job_finished() {
    s().job_pending = false;
    wifi::release();
    let _ = lifecycle::set_resident(false);
}

fn do_print() {
    if s().job_pending {
        return;
    }
    if s().url.is_empty() {
        ui::push_message(i18n::tr_key("need_url"), ui::UI_ICON_ALERT, 2000);
        edit_url();
        return;
    }
    if wifi::request(WIFI_TIMEOUT_MS).is_err() {
        wifi::release();
        ui::push_message(i18n::tr_key("no_wifi"), ui::UI_ICON_ERROR, 2500);
        return;
    }
    let target = s().url.replace(PLACEHOLDER, &format!("{}", s().num));
    let Some(body) = fetch(&target) else {
        wifi::release();
        ui::push_message(i18n::tr_key("http_failed"), ui::UI_ICON_ERROR, 2500);
        return;
    };
    // The `background` capability only permits residency; claiming it keeps
    // this plugin loaded while the printer plugin takes the foreground, so the
    // status action still arrives.
    let _ = lifecycle::set_resident(true);
    s().job_pending = true;
    if let Err(e) = feature::use_ext_feature("thermo_print", &text_job(body.trim()), ACT_STATUS) {
        log::warn(TAG, &format!("thermo_print rejected: {:?}", e));
        job_finished();
        // NotFound already raised the host's "no plugin provides this feature" modal.
        if e != Error::NotFound {
            ui::push_message(i18n::tr_key("print_failed"), ui::UI_ICON_ERROR, 2500);
        }
    }
}

/// GET `url` and return at most [`feature::PAYLOAD_MAX`] bytes of the body as
/// text (invalid UTF-8 is replaced, not rejected).
fn fetch(url: &str) -> Option<String> {
    log::info(TAG, &format!("GET {}", url));
    let req = http::Request::open(http::GET, url, HTTP_TIMEOUT_MS).ok()?;
    let _ = req.header("Accept", "text/plain");
    let status = req.perform().ok()?;
    if status / 100 != 2 {
        log::warn(TAG, &format!("HTTP {}", status));
        return None;
    }
    let body = req.read_to_vec_max(feature::PAYLOAD_MAX).ok()?;
    Some(String::from_utf8_lossy(&body).into_owned())
}

/// Wrap `text` in a `thermo_print` text job (see
/// `plugins/thermo_printer/README.md`): magic "TX", version 1, reserved 0,
/// then UTF-8. The printer plugin does the word wrapping and pagination.
fn text_job(text: &str) -> Vec<u8> {
    const HEADER: [u8; 4] = [0x54, 0x58, 1, 0];
    let mut end = text.len().min(feature::PAYLOAD_MAX - HEADER.len());
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    let mut out = Vec::with_capacity(HEADER.len() + end);
    out.extend_from_slice(&HEADER);
    out.extend_from_slice(&text.as_bytes()[..end]);
    out
}
