//! \file
//! \brief Serial SDK probe: exercises every SDK binding once and logs the
//!        result to the serial console.
//!
//! Load it over serial (`PLUGIN LOAD ...`). On entry it shows a "running"
//! toast, waits ~1s, then walks every SDK module, writing one log line per
//! function it calls, and finally shows a "done" toast. The display only
//! ever shows those two toasts; all detail goes to the serial log.
//!
//! It is non-destructive by construction: the host sandboxes plugins
//! (per-namespace NVS, name-pooled rmem, capability-gated GPIO/I2C/SAO/
//! BLE/WiFi/display, a reserved plugin ECC slot). Calls that lack a
//! declared capability return an error, which the probe logs as a passing
//! "refused" result. Operations that mutate persistent state (NVS, rmem,
//! SAO EEPROM, ECC) are written as read/restore round-trips or guarded so
//! nothing is lost.

#![no_std]

extern crate alloc;

use alloc::format;
use alloc::string::String;
use cdc_badge_plugin::{
    ble, canvas, cmd, crypto, display, event, fs, gpio, http, i18n, i2c, keypad, lockscreen, log,
    nvs, pixel_strip, plugin_main, power, random, rmem, sao, secure_element, sysinfo, time, ui, usb,
    wifi,
};

plugin_main!();

const TAG: &str = "probe";

/// ECC key name declared in meta.json `capabilities.ecc`.
const ECC_KEY: &str = "probe";

const PHASE_INIT: u8 = 0;
const PHASE_WAIT: u8 = 1;
const PHASE_DONE: u8 = 2;

static mut PHASE: u8 = PHASE_INIT;
static mut START_MS: u64 = 0;

fn line(msg: &str) {
    log::info(TAG, msg);
}

#[no_mangle]
pub extern "C" fn plugin_init() -> i32 {
    line("loaded");
    0
}

#[no_mangle]
pub extern "C" fn plugin_deinit() -> i32 {
    line("deinit");
    0
}

#[no_mangle]
pub extern "C" fn plugin_on_enter() -> i32 {
    line("enter");
    0
}

#[no_mangle]
pub extern "C" fn plugin_on_exit() -> i32 {
    0
}

/// \brief Drives the toast -> wait -> run -> toast sequence from the tick.
#[no_mangle]
pub extern "C" fn plugin_on_tick(uptime_ms: u64) -> i32 {
    let phase = unsafe { PHASE };
    if phase == PHASE_INIT {
        unsafe {
            START_MS = uptime_ms;
            PHASE = PHASE_WAIT;
        }
        ui::push_toast(i18n::tr_key("running"), ui::UI_ICON_PLAY, 1500);
        line("probe armed; starting in 1s");
    } else if phase == PHASE_WAIT && uptime_ms.saturating_sub(unsafe { START_MS }) >= 1000 {
        unsafe { PHASE = PHASE_DONE };
        run_probe();
        ui::push_toast(i18n::tr_key("done"), ui::UI_ICON_SUCCESS, 3000);
        line("probe complete");
    }
    0
}

/// \brief Walk every SDK module once, logging each call.
fn run_probe() {
    line("==== SDK probe start ====");
    probe_time();
    probe_power();
    probe_sysinfo();
    probe_random();
    probe_crypto();
    probe_nvs();
    probe_fs();
    probe_rmem();
    probe_secure_element();
    probe_keypad();
    probe_gpio();
    probe_i2c();
    probe_sao();
    probe_http();   // before WiFi: WiFi probing may drop the connection
    probe_wifi();
    probe_ble();
    probe_display();
    probe_canvas();
    probe_pixel_strip();
    probe_usb();
    probe_event();
    probe_lockscreen();
    probe_i18n();
    probe_cmd();
    probe_ui();
    line("==== SDK probe end ====");
}

fn probe_time() {
    line("-- time --");
    line(&format!("time::uptime_ms = {}", time::uptime_ms()));
    line(&format!("time::unix_time = {}", time::unix_time()));
    line(&format!("time::is_time_set = {}", time::is_time_set()));
    line(&format!("time::timezone_offset = {}", time::timezone_offset()));
    line(&format!("time::local_time = {:?}", time::local_time()));
}

fn probe_power() {
    line("-- power --");
    line(&format!("power::battery_mv = {}", power::battery_mv()));
    line(&format!("power::battery_pct = {}", power::battery_pct()));
    line(&format!("power::usb_connected = {}", power::usb_connected()));
    line(&format!("power::power_source = {:?}", power::power_source()));
    line(&format!("power::charge_status = {:?}", power::charge_status()));
    line(&format!("power::battery_low = {}", power::battery_low()));
    line(&format!("power::battery_critical = {}", power::battery_critical()));
}

fn probe_sysinfo() {
    line("-- sysinfo --");
    line(&format!("sysinfo::feature_enabled(0) = {}", sysinfo::feature_enabled(0)));
    line(&format!("sysinfo::firmware_version = {:?}", sysinfo::firmware_version()));
    line(&format!("sysinfo::build_profile = {:?}", sysinfo::build_profile()));
}

fn probe_random() {
    line("-- random --");
    let mut buf = [0u8; 16];
    line(&format!("random::fill(16) = {:?}", random::fill(&mut buf)));
    let mut buf2 = [0u8; 8];
    line(&format!("random::fill_strict(8) = {:?}", random::fill_strict(&mut buf2)));
    line(&format!("random::u32 = {:?}", random::u32()));
}

fn probe_crypto() {
    line("-- crypto (self-verifying) --");

    // SHA-256("abc") known answer.
    const SHA_ABC: [u8; 32] = [
        0xba, 0x78, 0x16, 0xbf, 0x8f, 0x01, 0xcf, 0xea, 0x41, 0x41, 0x40, 0xde, 0x5d, 0xae, 0x22,
        0x23, 0xb0, 0x03, 0x61, 0xa3, 0x96, 0x17, 0x7a, 0x9c, 0xb4, 0x10, 0xff, 0x61, 0xf2, 0x00,
        0x15, 0xad,
    ];
    match crypto::sha256(b"abc") {
        Ok(h) => line(&format!("crypto::sha256(\"abc\") {}", pass(h == SHA_ABC))),
        Err(_) => line("crypto::sha256 ERR"),
    }

    line(&format!("crypto::hmac_sha256 = {}", pass(crypto::hmac_sha256(b"key", b"msg").is_ok())));

    let pt = b"probe-plaintext-123";
    let key = [0x11u8; 32];
    let iv = [0x22u8; 12];
    match crypto::aes_gcm_encrypt(&key, &iv, b"aad", pt) {
        Ok(sealed) => match crypto::aes_gcm_decrypt(&key, &iv, b"aad", &sealed.ciphertext, &sealed.tag) {
            Ok(round) => line(&format!("crypto::aes_gcm round-trip {}", pass(round == pt))),
            Err(_) => line("crypto::aes_gcm_decrypt ERR"),
        },
        Err(_) => line("crypto::aes_gcm_encrypt ERR"),
    }

    let data: &[u8] = &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 250, 251, 252, 253, 254, 255];
    roundtrip("base64", crypto::base64_encode(data).ok(), |s| crypto::base64_decode(&s).ok(), data);
    roundtrip("base32", crypto::base32_encode(data).ok(), |s| crypto::base32_decode(&s).ok(), data);
    roundtrip("hex", crypto::hex_encode(data).ok(), |s| crypto::hex_decode(&s).ok(), data);
}

fn roundtrip(
    name: &str,
    encoded: Option<String>,
    decode: impl FnOnce(String) -> Option<alloc::vec::Vec<u8>>,
    original: &[u8],
) {
    let ok = match encoded {
        Some(s) => decode(s).map(|d| d == original).unwrap_or(false),
        None => false,
    };
    line(&format!("crypto::{} round-trip {}", name, pass(ok)));
}

fn pass(ok: bool) -> &'static str {
    if ok {
        "PASS"
    } else {
        "FAIL"
    }
}

fn probe_nvs() {
    line("-- nvs (own namespace round-trip) --");
    let set = nvs::set_u32("__probe_u32", 0xCAFE_F00D);
    let got = nvs::get_u32("__probe_u32");
    line(&format!("nvs::set_u32/get_u32 {}", pass(set.is_ok() && got == Some(0xCAFE_F00D))));

    let blob_set = nvs::set_blob("__probe_blob", &[1, 2, 3, 4]);
    let blob_get = nvs::get_blob("__probe_blob", 32);
    line(&format!(
        "nvs::set_blob/get_blob {}",
        pass(blob_set.is_ok() && blob_get.as_deref() == Some(&[1, 2, 3, 4][..]))
    ));

    let str_set = nvs::set_str("__probe_str", "hello");
    let str_get = nvs::get_str("__probe_str", 32);
    line(&format!(
        "nvs::set_str/get_str {}",
        pass(str_set.is_ok() && str_get.as_deref() == Some("hello"))
    ));

    line(&format!("nvs::list_keys = {:?}", nvs::list_keys(256)));
    line(&format!("nvs::erase(__probe_u32) = {:?}", nvs::erase("__probe_u32")));
    line(&format!("nvs::erase(__probe_blob) = {:?}", nvs::erase("__probe_blob")));
    line(&format!("nvs::erase(__probe_str) = {:?}", nvs::erase("__probe_str")));
    // Namespace-scoped: only this plugin's own keys.
    line(&format!("nvs::erase_all = {:?}", nvs::erase_all()));
}

fn probe_fs() {
    line("-- fs (own vFAT folder round-trip) --");
    let write = fs::write_str("__probe.txt", "hello\nworld");
    let read = fs::read_str("__probe.txt", 64);
    line(&format!(
        "fs::write_str/read_str {}",
        pass(write.is_ok() && read.as_deref() == Some("hello\nworld"))
    ));
    line(&format!("fs::size = {:?}", fs::size("__probe.txt")));
    line(&format!("fs::list = {:?}", fs::list(256)));
    line(&format!("fs::remove = {:?}", fs::remove("__probe.txt")));
}

fn probe_rmem() {
    line("-- rmem (own pool slot 'probe') --");
    line(&format!("rmem::slot_size = {}", rmem::slot_size()));
    match rmem::write("probe", &[0xAA, 0xBB, 0xCC]) {
        Ok(()) => {
            let rd = rmem::read("probe", 8);
            line(&format!(
                "rmem::write/read {}",
                pass(rd.map(|v| v == [0xAA, 0xBB, 0xCC]).unwrap_or(false))
            ));
            line(&format!("rmem::is_used = {}", rmem::is_used("probe")));
            line(&format!("rmem::erase = {:?}", rmem::erase("probe")));
        }
        Err(_) => line("rmem::write -> Err (SE absent or no capability) [ok]"),
    }
}

fn probe_secure_element() {
    line("-- secure_element (reserved plugin slot) --");
    line(&format!("secure_element::chip_id = {:?}", secure_element::chip_id(32)));
    line(&format!("secure_element::fw_version = {:?}", secure_element::fw_version()));
    let used = secure_element::exists(ECC_KEY);
    line(&format!("secure_element::exists({}) = {}", ECC_KEY, used));
    if used {
        line("secure_element: key present, skipping generate/sign/delete");
        line(&format!("secure_element::pubkey = {:?}", secure_element::pubkey(ECC_KEY, secure_element::Curve::P256).map(|k| k.len())));
        return;
    }
    match secure_element::generate(ECC_KEY, secure_element::Curve::P256) {
        Ok(()) => {
            // Prove exists() flips to true for a present key (was false above
            // because the probe deletes the key at the end of every run).
            line(&format!("secure_element::exists after generate = {} (expect true)",
                          secure_element::exists(ECC_KEY)));
            line(&format!("secure_element::pubkey len = {:?}", secure_element::pubkey(ECC_KEY, secure_element::Curve::P256).map(|k| k.len())));
            line(&format!("secure_element::ecdsa_sign = {:?}", secure_element::ecdsa_sign(ECC_KEY, b"probe").map(|_| ())));
            line(&format!("secure_element::delete = {:?}", secure_element::delete(ECC_KEY)));
            line(&format!("secure_element::exists after delete = {} (expect false)",
                          secure_element::exists(ECC_KEY)));
        }
        Err(_) => line("secure_element::generate -> Err (SE absent or no capability) [ok]"),
    }
}

fn probe_keypad() {
    line("-- keypad --");
    line(&format!("keypad::is_pressed(KEY_0) = {}", keypad::is_pressed(keypad::KEY_0)));
    line(&format!("keypad::consume_next = {:?}", keypad::consume_next()));
}

fn probe_gpio() {
    line("-- gpio / pwm / adc (SAO GPIO 15) --");
    let pin = gpio::pins::SAO_GPIO1;
    line(&format!("gpio::set_direction = {:?}", gpio::set_direction(pin, gpio::Direction::Output)));
    line(&format!("gpio::write(high) = {:?}", gpio::write(pin, true)));
    line(&format!("gpio::write(low) = {:?}", gpio::write(pin, false)));
    line(&format!("gpio::set_pull = {:?}", gpio::set_pull(pin, gpio::Pull::Up)));
    line(&format!("gpio::read = {:?}", gpio::read(pin)));
    line(&format!("gpio::pwm_start = {:?}", gpio::pwm_start(pin, 1000, 500)));
    line(&format!("gpio::pwm_set_duty = {:?}", gpio::pwm_set_duty(pin, 250)));
    line(&format!("gpio::pwm_stop = {:?}", gpio::pwm_stop(pin)));
    line(&format!("gpio::adc_read(4) = {:?}", gpio::adc_read(4)));
    gpio::release(pin);
    line("gpio::release done");
}

fn probe_i2c() {
    line("-- i2c (expansion bus 1) --");
    line(&format!("i2c::scan = {:?}", i2c::scan(1)));
    line(&format!("i2c::write = {:?}", i2c::write(1, 0x7F, &[0])));
    line(&format!("i2c::read = {:?}", i2c::read(1, 0x7F, 1)));
    line(&format!("i2c::write_read = {:?}", i2c::write_read(1, 0x7F, &[0], 1)));
}

fn probe_sao() {
    line("-- sao eeprom (read/modify/restore) --");
    match sao::eeprom_read(0, 4) {
        Ok(orig) => {
            line(&format!("sao::eeprom_read = {:?}", orig));
            let w = sao::eeprom_write(0, &[0xDE, 0xAD, 0xBE, 0xEF]);
            line(&format!("sao::eeprom_write = {:?}", w));
            if w.is_ok() {
                let _ = sao::eeprom_write(0, &orig);
                line("sao: restored original bytes");
            }
        }
        Err(_) => line("sao::eeprom_read -> Err (no SAO or stub) [ok]"),
    }
}

fn probe_wifi() {
    line("-- wifi (read-only) --");
    line(&format!("wifi::is_connected = {}", wifi::is_connected()));
    line(&format!("wifi::ssid = {:?}", wifi::ssid()));
    line(&format!("wifi::ip = {:?}", wifi::ip()));
    line(&format!("wifi::rssi = {}", wifi::rssi()));
    line(&format!("wifi::mac = {:?}", wifi::mac()));
    line(&format!("wifi::start_scan = {:?}", wifi::start_scan()));
    line(&format!("wifi::scan_done = {}", wifi::scan_done()));
    line(&format!("wifi::scan_results = {:?}", wifi::scan_results(4).map(|v| v.len())));
}

fn probe_ble() {
    line("-- ble (read-only + register round-trip) --");
    line(&format!("ble::is_enabled = {}", ble::is_enabled()));
    line(&format!("ble::mac = {:?}", ble::mac()));
    line(&format!("ble::device_name = {:?}", ble::device_name()));
    line(&format!("ble::rssi = {}", ble::rssi()));
    line(&format!("ble::scan_start = {:?}", ble::scan_start(2000)));
    line(&format!("ble::scan_done = {}", ble::scan_done()));
    line(&format!("ble::scan_results = {:?}", ble::scan_results(4).map(|v| v.len())));
    line(&format!("ble::conn_handle = {}", ble::conn_handle()));

    // Peripheral round-trip: register a throwaway service on the reserved slot
    // (random 128-bit UUID, not a reserved one), then unregister it.
    let svc_uuid = [0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88,
                    0x99, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xF0, 0x0F];
    let char_uuid = [0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88,
                     0x99, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xF1, 0x0F];
    let mut chars = [ble::CharDef::new(
        char_uuid,
        ble::PROP_READ | ble::PROP_NOTIFY | ble::PROP_WRITE,
        4250,
    )];
    match ble::register_service(svc_uuid, &mut chars) {
        Ok(h) => {
            line(&format!("ble::register_service = svc {}, char {}", h, chars[0].char_handle));
            line(&format!("ble::notify (no peer) = {:?}", ble::notify(chars[0].char_handle, b"hi")));
            let mut wbuf = [0u8; 16];
            line(&format!("ble::consume_write (empty) = {:?}", ble::consume_write(chars[0].char_handle, &mut wbuf)));
            line(&format!("ble::unregister_service = {:?}", ble::unregister_service(h)));
        }
        Err(e) => line(&format!("ble::register_service -> {:?} [ok if BLE off]", e)),
    }
    let mut nbuf = [0u8; 32];
    line(&format!("ble::consume_notification (empty) = {:?}", ble::consume_notification(&mut nbuf)));
}

fn probe_display() {
    line("-- display (lowlevel; screen effects expected) --");
    let (w, h) = (display::width(), display::height());
    line(&format!("display::width/height = {}x{}", w, h));
    line(&format!("display::is_busy = {}", display::is_busy()));
    line(&format!("display::clear = {:?}", display::clear()));
    line(&format!("display::draw_pixel = {:?}", display::draw_pixel(1, 1, 1)));
    line(&format!("display::draw_line = {:?}", display::draw_line(0, 0, 10, 10, 1)));
    line(&format!("display::draw_rect = {:?}", display::draw_rect(2, 2, 8, 8, 1)));
    line(&format!("display::fill_rect = {:?}", display::fill_rect(4, 4, 4, 4, 1)));
    line(&format!("display::draw_text = {:?}", display::draw_text(0, 20, "probe", 1, 1)));
    line(&format!("display::flush = {:?}", display::flush(0)));
}

fn probe_pixel_strip() {
    line("-- pixel_strip (Grove SIG0, 1 LED round-trip) --");
    line(&format!("pixel_strip::is_ready (pre) = {}", pixel_strip::is_ready()));
    match pixel_strip::init(gpio::pins::GROVE_0, 1, pixel_strip::Format::Grb) {
        Ok(()) => {
            line(&format!("pixel_strip::length = {}", pixel_strip::length()));
            line(&format!("pixel_strip::fill = {:?}", pixel_strip::fill(0, 0, 0)));
            line(&format!("pixel_strip::set = {:?}", pixel_strip::set(0, 0, 0, 0)));
            line(&format!("pixel_strip::refresh = {:?}", pixel_strip::refresh()));
            line(&format!("pixel_strip::clear = {:?}", pixel_strip::clear()));
            let _ = pixel_strip::refresh();
            line(&format!("pixel_strip::deinit = {:?}", pixel_strip::deinit()));
        }
        Err(_) => line("pixel_strip::init -> Err (no capability / pin busy) [ok]"),
    }
}

fn probe_usb() {
    line("-- usb cdc --");
    line(&format!("usb::cdc_write = {:?}", usb::cdc_write(b"[probe] usb_cdc_write\n")));
}

fn probe_event() {
    line("-- event bus (subscribe/unsubscribe) --");
    match event::subscribe(event::KEY_PRESSED, 4242) {
        Ok(id) => {
            line(&format!("event::subscribe = {}", id));
            event::unsubscribe(id);
            line("event::unsubscribe done");
        }
        Err(e) => line(&format!("event::subscribe -> {:?}", e)),
    }
    event::publish_module_event(1, 0);
    line("event::publish_module_event done");
}

fn probe_lockscreen() {
    line("-- lockscreen quick action --");
    line(&format!("lockscreen::register = {:?}", lockscreen::register("running", 4243)));
    lockscreen::unregister();
    line("lockscreen::unregister done");
}

fn probe_i18n() {
    line("-- i18n --");
    line(&format!("i18n::current_language = {:?}", i18n::current_language()));
    line(&format!("i18n::tr_key(running) = {:?}", i18n::tr_key("running")));
    line(&format!("i18n::tr_meta(name) = {:?}", i18n::tr_meta("name")));
    line(&format!("i18n::tr_core(core.ok) = {:?}", i18n::tr_core("core.ok")));
}

fn probe_cmd() {
    line("-- cmd (plugin command channel) --");
    // No host-pushed command pending in this context -> None.
    line(&format!("cmd::consume = {:?}", cmd::consume(64)));
}

fn probe_http() {
    line("-- http (live request, only when WiFi is connected) --");
    if !wifi::is_connected() {
        line("http: WiFi not connected, skipping live request");
        return;
    }
    match http::Request::open(http::GET, "http://example.com/", 8000) {
        Ok(req) => {
            let _ = req.header("Accept", "text/plain");
            match req.perform() {
                Ok(status) => {
                    line(&format!("http::perform status = {}", status));
                    line(&format!("http::content_length = {}", req.content_length()));
                    match req.read_to_string() {
                        Ok(body) => line(&format!("http::read_to_string = {} bytes", body.len())),
                        Err(e) => line(&format!("http::read_to_string -> {:?}", e)),
                    }
                }
                Err(e) => line(&format!("http::perform -> {:?}", e)),
            }
        }
        Err(e) => line(&format!("http::open -> {:?}", e)),
    }
}

fn probe_canvas() {
    line("-- canvas (push, draw everything, widgets, then pop) --");
    canvas::push("Probe canvas", 4400, 4401);
    let (w, h) = canvas::body_size();
    line(&format!("canvas::body_size = {}x{}", w, h));
    canvas::set_footer("canvas footer");
    canvas::clear();
    canvas::set_text_size(2);
    line(&format!("canvas::set_font = {:?}", canvas::set_font(canvas::FONT_BOLD_9PT)));
    line(&format!(
        "canvas::pick_font_that_fits = {:?}",
        canvas::pick_font_that_fits(
            "Probe",
            120,
            &[canvas::FONT_BOLD_12PT, canvas::FONT_BOLD_9PT, canvas::FONT_BUILTIN]
        )
    ));
    canvas::set_text_inverted(false);
    canvas::draw_text(0, 10, "probe");
    canvas::draw_text_aligned(0, 24, w as i16, "centered", canvas::ALIGN_CENTER);
    canvas::draw_rect(2, 40, 20, 12, false);
    canvas::draw_rect(26, 40, 20, 12, true);
    canvas::invert_rect(2, 40, 8, 6);
    canvas::hline(0, 60, 80);
    canvas::vline(0, 0, 60);
    line("canvas: draw primitives done");
    canvas::add_slider(1, 0, 100, 50, 5);
    canvas::add_text(2, 16, Some("hi"));
    canvas::add_button(3);
    canvas::set_value(1, 42);
    line(&format!("canvas::get_value(1) = {:?}", canvas::get_value(1)));
    canvas::set_text(2, "edited");
    line(&format!("canvas::get_text(2) = {:?}", canvas::get_text(2, 16)));
    canvas::set_focus(1);
    line(&format!("canvas::get_focus = {}", canvas::get_focus()));
    canvas::set_key_repeat(400, 120);
    canvas::remove_widget(3);
    canvas::commit(false);
    line("canvas: widgets + commit done");
    ui::pop();
    line("canvas::pop done");
}

fn probe_ui() {
    line("-- ui (push every view type, then pop) --");

    // String conversion helpers.
    line(&format!("ui::to_display = {} bytes", ui::to_display("<b>H&auml;</b>").len()));
    line(&format!(
        "ui::to_display_with(LATIN1) = {} bytes",
        ui::to_display_with("Test", ui::HOST_STR_TARGET_LATIN1).len()
    ));

    // Transient / modal pushes.
    ui::push_toast("probe toast", ui::UI_ICON_INFO, 300);
    line("ui::push_toast done");
    ui::push_message("probe message", ui::UI_ICON_INFO, 300);
    line("ui::push_message done");
    ui::push_info("Probe", "info body");
    ui::pop();
    line("ui::push_info + pop done");
    ui::push_confirm("confirm?", ui::UI_ICON_INFO, 4300);
    ui::pop();
    line("ui::push_confirm + pop done");

    // List view + list mutators.
    ui::ListBuilder::new("Probe list")
        .on_select(4301)
        .on_menu(4302)
        .item("Item A", 1, ui::UI_ICON_BULLET)
        .item("Item B", 2, ui::UI_ICON_BULLET)
        .push();
    ui::set_footer("footer hint");
    ui::set_list_empty("empty text");
    ui::update_list_item(0, "Item A*", 1, ui::UI_ICON_CIRCLE);
    ui::insert_list_item(1, "Item Ins", 3, ui::UI_ICON_BULLET);
    ui::remove_list_item(2);
    line("ui::ListBuilder + set_footer/set_list_empty/update/insert/remove_list_item done");
    ui::ListBuilder::new("Probe list 2")
        .on_select(4301)
        .item("X", 9, ui::UI_ICON_BULLET)
        .replace();
    line("ui::ListBuilder::replace done");
    ui::pop();
    line("ui::pop (list) done");

    // Context menu.
    ui::ContextMenuBuilder::new("Ctx")
        .on_select(4303)
        .item("C1", 1, ui::UI_ICON_BULLET)
        .push();
    ui::pop();
    line("ui::ContextMenuBuilder + pop done");

    // Slider.
    ui::SliderBuilder::new("Slider")
        .range(0, 100)
        .initial(50)
        .step(5)
        .unit("%")
        .on_save(4304)
        .push();
    ui::pop();
    line("ui::SliderBuilder + pop done");

    // Color picker.
    ui::push_color_picker(10, 20, 30, 4305);
    ui::pop();
    line("ui::push_color_picker + pop done");

    // Text inputs.
    ui::push_t9_input("T9", Some("hi"), 16, 4306);
    ui::pop();
    line("ui::push_t9_input + pop done");
    ui::push_password("PW", None, 16, 4307);
    ui::pop();
    line("ui::push_password + pop done");

    // Date / time / PIN entry.
    ui::push_date("Date", 30, 5, 2026, 4308);
    ui::pop();
    line("ui::push_date + pop done");
    ui::push_time("Time", 12, 34, 4309);
    ui::pop();
    line("ui::push_time + pop done");
    ui::push_pin_entry("PIN", 6, 3, 4310);
    ui::pop();
    line("ui::push_pin_entry + pop done");

    // Input consumers (no pending input -> None).
    line(&format!("ui::consume_input_int = {:?}", ui::consume_input_int()));
    line(&format!("ui::consume_input_text = {:?}", ui::consume_input_text(32)));

    // Exclusive lock / inactivity / wink / repaint.
    line(&format!("ui::acquire_exclusive = {:?}", ui::acquire_exclusive()));
    line(&format!("ui::release_exclusive = {:?}", ui::release_exclusive()));
    ui::set_inactivity(600_000, 4311);
    line("ui::set_inactivity done");
    ui::wink(1, 60);
    line("ui::wink done");
    ui::repaint();
    line("ui::repaint done");

    // Return to the plugin's root view.
    ui::pop_to_plugin();
    line("ui::pop_to_plugin done");
}
