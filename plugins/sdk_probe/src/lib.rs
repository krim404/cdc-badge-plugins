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
//!
//! ## How to read this file (for newcomers)
//!
//! This plugin is meant to be read, not just run. It calls almost every SDK
//! function once, and the code below is commented like a mini-tutorial so you
//! can copy the lines you need into your own plugin. A few ideas apply
//! everywhere:
//!
//! - Lifecycle: the badge calls the `plugin_*` functions for you (load, enter,
//!   tick, exit, unload). You never call them yourself; you just fill them in.
//! - `Result` / `Option`: most SDK calls return `Ok`/`Err` or `Some`/`None`.
//!   Always handle the failure case (`match`, `if let`, or `.is_ok()`).
//! - Capabilities: a plugin may only touch hardware it declared in its
//!   `meta.json` (WiFi, GPIO pins, BLE, ...). Asking for something undeclared
//!   just returns an error, which is why this probe treats many errors as
//!   "fine, not allowed here".
//! - Output: everything is printed with `line(...)` (a wrapper around
//!   `log::info`), so you read the results over the USB serial console, not on
//!   the badge screen.

#![no_std]

extern crate alloc;

// `format!` builds a String from a template, like printf. `String` is a
// growable, heap-allocated text buffer. `alloc` provides both because a
// `#![no_std]` plugin has no standard library, only the allocator.
use alloc::format;
use alloc::string::String;

// Pull in every SDK module this probe touches. Each name here is one area of
// the host API (`time`, `power`, `nvs`, ...). `plugin_main!` is a macro that
// wires up the boilerplate the firmware expects.
use cdc_badge_plugin::{
    ble, canvas, cmd, crypto, display, event, fs, gpio, http, i18n, i2c, keypad, lockscreen, log,
    nvs, pixel_strip, plugin_main, power, random, rmem, sao, secure_element, socket, sysinfo, time,
    ui, usb, wifi,
};

plugin_main!();

/// Tag prefixed to every serial log line, so probe output is easy to filter.
const TAG: &str = "probe";

/// ECC key name declared in meta.json `capabilities.ecc`.
const ECC_KEY: &str = "probe";

/// Action id echoed back when the user answers the lockscreen alert.
const ALERT_ACTION_ID: u32 = 4244;

// A tiny state machine for the startup sequence. `plugin_on_tick` runs many
// times per second; these three values track where we are in the
// "show toast -> wait 1s -> run once" flow.
const PHASE_INIT: u8 = 0;
const PHASE_WAIT: u8 = 1;
const PHASE_DONE: u8 = 2;
const PHASE_ALERT: u8 = 3;

// State that must survive between tick calls lives in `static mut`. Reading or
// writing it needs an `unsafe` block because the compiler cannot prove only
// one place touches it at a time; on this single-threaded runtime that is fine.
static mut PHASE: u8 = PHASE_INIT;
static mut START_MS: u64 = 0;

/// Print one line to the serial log. Every probe uses this so the tag is not
/// repeated everywhere. In your own plugin you can just call
/// `log::info("mytag", "message")` directly.
fn line(msg: &str) {
    log::info(TAG, msg);
}

// --- Plugin lifecycle hooks ------------------------------------------------
// The badge calls these automatically. Return 0 for "OK" (any other value
// signals an error to the firmware). A real plugin sets up its state in
// `plugin_init` and draws its first screen in `plugin_on_enter`; this probe
// only logs that each hook ran.

/// Runs once when the plugin is loaded into memory.
#[no_mangle]
pub extern "C" fn plugin_init() -> i32 {
    line("loaded");
    0
}

/// Runs once when the plugin is unloaded.
#[no_mangle]
pub extern "C" fn plugin_deinit() -> i32 {
    line("deinit");
    0
}

/// Runs when the plugin comes to the foreground (the user opened it).
#[no_mangle]
pub extern "C" fn plugin_on_enter() -> i32 {
    line("enter");
    0
}

/// Runs when the plugin leaves the foreground.
#[no_mangle]
pub extern "C" fn plugin_on_exit() -> i32 {
    0
}

/// Receives the answer to the deferred lockscreen alert: `user_data` is 1 for Y
/// (confirm) or 0 for N (cancel).
#[no_mangle]
pub extern "C" fn plugin_on_action(action_id: u32, _idx: u32, user_data: u32) -> i32 {
    if action_id == ALERT_ACTION_ID {
        line(if user_data == 1 {
            "lockscreen::alert answered YES"
        } else {
            "lockscreen::alert answered NO"
        });
    }
    0
}

/// Runs once per frame while the plugin is open; `uptime_ms` is the current
/// time in milliseconds. This uses the PHASE_* state machine to show a
/// "running" toast on the first tick, then ~1 second later run the probe once
/// and show a "done" toast. Deferring the heavy work keeps the first frame
/// responsive, a good habit for any tick handler.
#[no_mangle]
pub extern "C" fn plugin_on_tick(uptime_ms: u64) -> i32 {
    let phase = unsafe { PHASE };
    if phase == PHASE_INIT {
        // First tick: remember when we started and move to the waiting phase.
        unsafe {
            START_MS = uptime_ms;
            PHASE = PHASE_WAIT;
        }
        // Pop up a short on-screen message. `tr_key` resolves the translated
        // text for the key "running" (see the plugin's i18n strings).
        ui::push_toast(i18n::tr_key("running"), ui::UI_ICON_PLAY, 1500);
        line("probe armed; starting in 1s");
    } else if phase == PHASE_WAIT && uptime_ms.saturating_sub(unsafe { START_MS }) >= 1000 {
        // 1000 ms have passed. `saturating_sub` avoids underflow if the clock
        // ever goes backwards. Run everything once, then defer the alert demo.
        unsafe {
            PHASE = PHASE_DONE;
            START_MS = uptime_ms;
        }
        run_probe();
        ui::push_toast(i18n::tr_key("done"), ui::UI_ICON_SUCCESS, 3000);
        line("probe complete");
    } else if phase == PHASE_DONE && uptime_ms.saturating_sub(unsafe { START_MS }) >= 3500 {
        // Once the "done" toast has cleared, raise the persistent Y/N alert a
        // background plugin would use to reach the user on the lock screen. The
        // umlauts double as a UTF-8 round-trip check; the answer arrives in
        // plugin_on_action.
        unsafe { PHASE = PHASE_ALERT };
        let rc = lockscreen::alert("SDK probe alert: OK? äöü", ui::UI_ICON_ALERT, ALERT_ACTION_ID);
        line(&format!("lockscreen::alert raised = {:?}", rc));
    }
    0
}

/// Calls every `probe_*` function in turn. Each one demonstrates a different
/// SDK module. The "==== ... ====" markers make the run easy to spot in the
/// serial log.
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
    probe_socket();
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

// time: read the clock and date. These need no capability and no setup, so
// they are the simplest possible SDK calls: just call and use the value.
// `{}` in format! prints a plain value; `{:?}` prints the debug form, used
// here for the `Option`/struct returns. Example:
//   if time::is_time_set() { let now = time::local_time(); }
fn probe_time() {
    line("-- time --");
    line(&format!("time::uptime_ms = {}", time::uptime_ms()));
    line(&format!("time::unix_time = {}", time::unix_time()));
    line(&format!("time::is_time_set = {}", time::is_time_set()));
    line(&format!("time::timezone_offset = {}", time::timezone_offset()));
    line(&format!("time::local_time = {:?}", time::local_time()));
}

// power: read battery and charger state. All read-only, all instant. Handy for
// a battery widget or for pausing heavy work when `battery_low()` is true.
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

// sysinfo: ask which firmware this is. Useful to enable features only when the
// firmware supports them, or to show a version string.
fn probe_sysinfo() {
    line("-- sysinfo --");
    line(&format!("sysinfo::feature_enabled(0) = {}", sysinfo::feature_enabled(0)));
    line(&format!("sysinfo::firmware_version = {:?}", sysinfo::firmware_version()));
    line(&format!("sysinfo::build_profile = {:?}", sysinfo::build_profile()));
}

// random: get random bytes. You create a buffer and pass it by reference
// (`&mut`); the call fills it in place. Example:
//   let mut key = [0u8; 16];
//   random::fill(&mut key)?;        // now `key` holds 16 random bytes
fn probe_random() {
    line("-- random --");
    let mut buf = [0u8; 16];
    line(&format!("random::fill(16) = {:?}", random::fill(&mut buf)));
    let mut buf2 = [0u8; 8];
    line(&format!("random::fill_strict(8) = {:?}", random::fill_strict(&mut buf2)));
    line(&format!("random::u32 = {:?}", random::u32()));
}

// crypto: hashing, encryption, and text encodings. This probe is
// "self-verifying": instead of just calling each function it checks the result
// against a known answer or does a round-trip (encode then decode and confirm
// you got the original back), printing PASS or FAIL.
fn probe_crypto() {
    line("-- crypto (self-verifying) --");

    // A hash is a fixed-size fingerprint of data. The SHA-256 of the text
    // "abc" is publicly known, so we compare against it to prove the call works.
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

    // HMAC is a hash that also depends on a secret key, used to prove a message
    // was not tampered with. Here we only check that the call succeeds.
    line(&format!("crypto::hmac_sha256 = {}", pass(crypto::hmac_sha256(b"key", b"msg").is_ok())));

    // AES-256-GCM locks data with a 32-byte key and a 12-byte nonce (iv), and
    // returns ciphertext plus a tag that detects tampering. We encrypt then
    // decrypt and confirm the text survived the trip.
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

    // base64 / base32 / hex turn raw bytes into printable text (and back), e.g.
    // to embed binary data in a string or URL. Each is checked with a round-trip.
    let data: &[u8] = &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 250, 251, 252, 253, 254, 255];
    roundtrip("base64", crypto::base64_encode(data).ok(), |s| crypto::base64_decode(&s).ok(), data);
    roundtrip("base32", crypto::base32_encode(data).ok(), |s| crypto::base32_decode(&s).ok(), data);
    roundtrip("hex", crypto::hex_encode(data).ok(), |s| crypto::hex_decode(&s).ok(), data);
}

/// Test helper (not an SDK call): encode some data, decode it back with the
/// given closure, and log PASS if the result matches the original.
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

/// Test helper: turn a true/false check into the text "PASS" or "FAIL".
fn pass(ok: bool) -> &'static str {
    if ok {
        "PASS"
    } else {
        "FAIL"
    }
}

// nvs: a small key/value store that survives a reboot, perfect for settings.
// Pattern: `set_*(key, value)` to save, `get_*(key)` to load. Each plugin gets
// its own private namespace, so your keys never clash with anyone else's.
// This probe writes, reads back, then erases, so it leaves nothing behind.
// Example: nvs::set_u32("brightness", 80)?; let b = nvs::get_u32("brightness");
fn probe_nvs() {
    line("-- nvs (own namespace round-trip) --");
    // Save a number, read it back, and confirm we got the same value.
    let set = nvs::set_u32("__probe_u32", 0xCAFE_F00D);
    let got = nvs::get_u32("__probe_u32");
    line(&format!("nvs::set_u32/get_u32 {}", pass(set.is_ok() && got == Some(0xCAFE_F00D))));

    // The same round-trip for a binary blob...
    let blob_set = nvs::set_blob("__probe_blob", &[1, 2, 3, 4]);
    let blob_get = nvs::get_blob("__probe_blob", 32);
    line(&format!(
        "nvs::set_blob/get_blob {}",
        pass(blob_set.is_ok() && blob_get.as_deref() == Some(&[1, 2, 3, 4][..]))
    ));

    // ...and for a text string.
    let str_set = nvs::set_str("__probe_str", "hello");
    let str_get = nvs::get_str("__probe_str", 32);
    line(&format!(
        "nvs::set_str/get_str {}",
        pass(str_set.is_ok() && str_get.as_deref() == Some("hello"))
    ));

    // List our keys, then clean everything up so the probe is non-destructive.
    line(&format!("nvs::list_keys = {:?}", nvs::list_keys(256)));
    line(&format!("nvs::erase(__probe_u32) = {:?}", nvs::erase("__probe_u32")));
    line(&format!("nvs::erase(__probe_blob) = {:?}", nvs::erase("__probe_blob")));
    line(&format!("nvs::erase(__probe_str) = {:?}", nvs::erase("__probe_str")));
    // Namespace-scoped: only this plugin's own keys.
    line(&format!("nvs::erase_all = {:?}", nvs::erase_all()));
}

// fs: store whole files in the plugin's private folder (also kept across
// reboots). Use it for bigger data than nvs, like logs or downloaded content.
// Needs the `vfat` capability. Example:
//   fs::write_str("notes.txt", "hello")?;
//   let s = fs::read_str("notes.txt", 256);
fn probe_fs() {
    line("-- fs (own vFAT folder round-trip) --");
    // Write a file, read it back, and confirm the contents match.
    let write = fs::write_str("__probe.txt", "hello\nworld");
    let read = fs::read_str("__probe.txt", 64);
    line(&format!(
        "fs::write_str/read_str {}",
        pass(write.is_ok() && read.as_deref() == Some("hello\nworld"))
    ));
    line(&format!("fs::size = {:?}", fs::size("__probe.txt")));
    line(&format!("fs::list = {:?}", fs::list(256)));
    line(&format!("fs::remove = {:?}", fs::remove("__probe.txt")));   // tidy up
}

// rmem: a few bytes of named storage inside the TROPIC01 security chip. The
// slot name must be declared in `capabilities.rmem`. If the chip is missing or
// the capability is not granted, `write` returns Err, which is fine here.
fn probe_rmem() {
    line("-- rmem (own pool slot 'probe') --");
    line(&format!("rmem::slot_size = {}", rmem::slot_size()));
    // Only continue the round-trip if the first write actually worked.
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

// secure_element: signing keys that live inside the security chip. The private
// key can never be read out; you can only ask the chip to sign with it. Typical
// flow: generate a key, export its public key, sign messages, verify elsewhere.
// This probe generates a throwaway key and deletes it again at the end.
fn probe_secure_element() {
    line("-- secure_element (reserved plugin slot) --");
    line(&format!("secure_element::chip_id = {:?}", secure_element::chip_id(32)));
    line(&format!("secure_element::fw_version = {:?}", secure_element::fw_version()));
    // Is a key with our name already stored?
    let used = secure_element::exists(ECC_KEY);
    line(&format!("secure_element::exists({}) = {}", ECC_KEY, used));
    if used {
        // A key is already present (e.g. left by another plugin): do not touch
        // it, just read the public part and stop.
        line("secure_element: key present, skipping generate/sign/delete");
        line(&format!("secure_element::pubkey = {:?}", secure_element::pubkey(ECC_KEY, secure_element::Curve::P256).map(|k| k.len())));
        return;
    }
    // Create a fresh P-256 key, use it, then clean it up.
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

// keypad: read the buttons directly. `is_pressed` tells you if a key is held
// right now; `consume_next` pops the next press from a queue (None if empty).
// Most plugins instead react to key events via the view callbacks; polling like
// this is for games or custom screens.
fn probe_keypad() {
    line("-- keypad --");
    line(&format!("keypad::is_pressed(KEY_0) = {}", keypad::is_pressed(keypad::KEY_0)));
    line(&format!("keypad::consume_next = {:?}", keypad::consume_next()));
}

// gpio: the expansion-port pins, plus PWM (square-wave output, e.g. to dim an
// LED) and ADC (read an analog voltage). Each pin must be declared in the
// manifest. Typical flow: set_direction -> write/read -> release when done.
// Example (blink): set_direction(pin, Output)?; write(pin, true)?; ... write(pin, false)?;
fn probe_gpio() {
    line("-- gpio / pwm / adc (SAO GPIO 15) --");
    let pin = gpio::pins::SAO_GPIO1;   // friendly name instead of a raw number
    line(&format!("gpio::set_direction = {:?}", gpio::set_direction(pin, gpio::Direction::Output)));
    line(&format!("gpio::write(high) = {:?}", gpio::write(pin, true)));
    line(&format!("gpio::write(low) = {:?}", gpio::write(pin, false)));
    line(&format!("gpio::set_pull = {:?}", gpio::set_pull(pin, gpio::Pull::Up)));
    line(&format!("gpio::read = {:?}", gpio::read(pin)));
    // PWM duty is in per-mille (tenths of a percent): 500 = 50%, 250 = 25%.
    line(&format!("gpio::pwm_start = {:?}", gpio::pwm_start(pin, 1000, 500)));
    line(&format!("gpio::pwm_set_duty = {:?}", gpio::pwm_set_duty(pin, 250)));
    line(&format!("gpio::pwm_stop = {:?}", gpio::pwm_stop(pin)));
    line(&format!("gpio::adc_read(4) = {:?}", gpio::adc_read(4)));
    gpio::release(pin);   // hand the pin back so other features can use it
    line("gpio::release done");

    // Negative test: try to grab a hardware-reserved pin. GPIO 33 is an octal
    // PSRAM data line (SPIIO4); driving it corrupts the PSRAM bus and crashes
    // the device. The host must reject every operation on a blocked pin even
    // when a plugin asks for it directly. An Ok(()) here would be a security
    // bug; the manifest also cannot declare a blocked pin (load is rejected).
    const BLOCKED_PIN: u8 = 33;
    line(&format!(
        "gpio::set_direction(blocked {}) = {:?} (expect Err)",
        BLOCKED_PIN,
        gpio::set_direction(BLOCKED_PIN, gpio::Direction::Input)
    ));
    line(&format!(
        "gpio::read(blocked {}) = {:?} (expect Err)",
        BLOCKED_PIN,
        gpio::read(BLOCKED_PIN)
    ));
}

// i2c: talk to add-on chips over the two-wire bus. `scan` finds which 7-bit
// addresses answer; then you read/write to a specific address. Bus 1 is the
// expansion bus; bus 0 is internal and off-limits to plugins.
fn probe_i2c() {
    line("-- i2c (expansion bus 1) --");
    line(&format!("i2c::scan = {:?}", i2c::scan(1)));
    line(&format!("i2c::write = {:?}", i2c::write(1, 0x7F, &[0])));
    line(&format!("i2c::read = {:?}", i2c::read(1, 0x7F, 1)));
    // write_read does a write then an immediate read in one transaction, the
    // usual way to read a register: send the register number, read its value.
    line(&format!("i2c::write_read = {:?}", i2c::write_read(1, 0x7F, &[0], 1)));
}

// sao: the small EEPROM memory on an attached "Shitty Add-On" board. To avoid
// corrupting someone's add-on, this probe reads the first bytes, writes test
// bytes, then writes the originals back (a read-modify-restore round-trip).
fn probe_sao() {
    line("-- sao eeprom (read/modify/restore) --");
    match sao::eeprom_read(0, 4) {
        Ok(orig) => {
            line(&format!("sao::eeprom_read = {:?}", orig));
            let w = sao::eeprom_write(0, &[0xDE, 0xAD, 0xBE, 0xEF]);
            line(&format!("sao::eeprom_write = {:?}", w));
            if w.is_ok() {
                let _ = sao::eeprom_write(0, &orig);   // put the originals back
                line("sao: restored original bytes");
            }
        }
        Err(_) => line("sao::eeprom_read -> Err (no SAO or stub) [ok]"),
    }
}

// wifi: read-only status here (are we online, SSID, IP, signal) plus an async
// scan for nearby networks. Most plugins do not connect by hand; they list
// `wifi_connected` in the manifest prerequisites and the host connects first.
fn probe_wifi() {
    line("-- wifi (read-only) --");
    line(&format!("wifi::is_connected = {}", wifi::is_connected()));
    line(&format!("wifi::ssid = {:?}", wifi::ssid()));
    line(&format!("wifi::ip = {:?}", wifi::ip()));
    line(&format!("wifi::rssi = {}", wifi::rssi()));
    line(&format!("wifi::mac = {:?}", wifi::mac()));
    // Scanning is asynchronous: start it, later check scan_done, then read the
    // results. The probe just fires each call once to show the shape.
    line(&format!("wifi::start_scan = {:?}", wifi::start_scan()));
    line(&format!("wifi::scan_done = {}", wifi::scan_done()));
    line(&format!("wifi::scan_results = {:?}", wifi::scan_results(4).map(|v| v.len())));
}

// ble: Bluetooth Low Energy. As a "peripheral" the badge offers a service that
// a phone connects to; as a "central" it scans for and reads other devices.
// Inbound data arrives asynchronously: you pass an `action_id`, the host later
// calls your `plugin_on_action`, and you pull the bytes with a `consume_*` call.
// This probe registers a throwaway service and immediately unregisters it.
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
    // A "characteristic" is one value the service exposes. The property flags
    // say it can be read, subscribed to (notify) and written; 4250 is the
    // action id fired when a phone writes to it.
    let mut chars = [ble::CharDef::new(
        char_uuid,
        ble::PROP_READ | ble::PROP_NOTIFY | ble::PROP_WRITE,
        4250,
    )];
    match ble::register_service(svc_uuid, &mut chars) {
        Ok(h) => {
            // On success the host fills in `chars[0].char_handle` for us.
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

// display: draw pixels straight onto the screen (advanced; needs the
// `display_lowlevel` capability). You draw into an off-screen buffer, then
// `flush` shows it. For normal UIs the `canvas` module below is much easier.
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
    line(&format!("display::flush = {:?}", display::flush(0)));   // nothing shows until this
}

// canvas: a managed drawing screen with built-in widgets (sliders, text fields,
// buttons). The usual flow is: push a canvas, draw text/shapes, add widgets,
// `commit` to show it, and `pop` to close it. The host handles key input for
// the focused widget; you handle the drawing. Widget ids (1, 2, 3 here) are
// your own labels for reading values back later.
fn probe_canvas() {
    line("-- canvas (push, draw everything, widgets, then pop) --");
    // push(title, key_action_id, widget_action_id): the two ids are fired back
    // to plugin_on_action for key presses and widget changes.
    canvas::push("Probe canvas", 4400, 4401);
    let (w, h) = canvas::body_size();   // drawable area in pixels
    line(&format!("canvas::body_size = {}x{}", w, h));
    canvas::set_footer("canvas footer");
    canvas::clear();
    canvas::set_text_size(2);
    line(&format!("canvas::set_font = {:?}", canvas::set_font(canvas::FONT_BOLD_9PT)));
    // pick_font_that_fits just measures: it returns the biggest listed font
    // whose text fits in the width, so you can avoid clipping long strings.
    line(&format!(
        "canvas::pick_font_that_fits = {:?}",
        canvas::pick_font_that_fits(
            "Probe",
            120,
            &[canvas::FONT_BOLD_12PT, canvas::FONT_BOLD_9PT, canvas::FONT_BUILTIN]
        )
    ));
    canvas::set_text_inverted(false);
    // Drawing primitives: text, aligned text, rectangles, lines, pixel, circle,
    // triangle, rounded rectangle, and a 1-bpp bitmap.
    canvas::draw_text(0, 10, "probe");
    canvas::draw_text_aligned(0, 24, w as i16, "centered", canvas::ALIGN_CENTER);
    canvas::draw_rect(2, 40, 20, 12, false);    // outline
    canvas::draw_rect(26, 40, 20, 12, true);    // filled
    canvas::hline(0, 60, 80);
    canvas::vline(0, 0, 60);
    canvas::draw_pixel(54, 42);
    canvas::draw_line(52, 54, 78, 40);          // diagonal
    canvas::draw_circle(96, 46, 8, false);      // outline
    canvas::draw_circle(120, 46, 8, true);      // filled
    canvas::draw_triangle(140, 54, 156, 38, 172, 54, false);
    canvas::draw_round_rect(180, 38, 28, 16, 4, false);
    // 8x8 checkerboard bitmap (1 bpp, MSB first, one byte per row).
    let checker: [u8; 8] = [0xAA, 0x55, 0xAA, 0x55, 0xAA, 0x55, 0xAA, 0x55];
    canvas::draw_bitmap(214, 38, 8, 8, &checker);
    line("canvas: draw primitives done");
    // Add interactive widgets, each with a unique id you choose.
    canvas::add_slider(1, 0, 100, 50, 5);   // id 1: min, max, initial, step
    canvas::add_text(2, 16, Some("hi"));     // id 2: max length, initial text
    canvas::add_button(3);                   // id 3
    canvas::set_value(1, 42);                 // change the slider from code
    line(&format!("canvas::get_value(1) = {:?}", canvas::get_value(1)));
    canvas::set_text(2, "edited");            // change the text field from code
    line(&format!("canvas::get_text(2) = {:?}", canvas::get_text(2, 16)));
    canvas::set_focus(1);                      // send key input to the slider
    line(&format!("canvas::get_focus = {}", canvas::get_focus()));
    canvas::set_key_repeat(400, 120);
    canvas::remove_widget(3);
    canvas::commit(false);    // false = fast partial refresh; show what we drew
    line("canvas: widgets + commit done");
    ui::pop();                 // close the canvas view
    line("canvas::pop done");
}

// pixel_strip: drive an addressable LED strip (WS2812 and similar). Flow:
// init the strip on a pin, set pixel colors into a buffer, then `refresh` to
// actually light them up, and `deinit` when finished. Colors only appear after
// refresh. Example: init(pin, 8, Grb)?; fill(0,0,40)?; refresh()?;
fn probe_pixel_strip() {
    line("-- pixel_strip (Grove SIG0, 1 LED round-trip) --");
    line(&format!("pixel_strip::is_ready (pre) = {}", pixel_strip::is_ready()));
    // Init may fail if the capability is missing or the pin is busy; only use
    // the strip if it succeeded.
    match pixel_strip::init(gpio::pins::GROVE_0, 1, pixel_strip::Format::Grb) {
        Ok(()) => {
            line(&format!("pixel_strip::length = {}", pixel_strip::length()));
            line(&format!("pixel_strip::fill = {:?}", pixel_strip::fill(0, 0, 0)));
            line(&format!("pixel_strip::set = {:?}", pixel_strip::set(0, 0, 0, 0)));
            line(&format!("pixel_strip::refresh = {:?}", pixel_strip::refresh()));
            line(&format!("pixel_strip::clear = {:?}", pixel_strip::clear()));
            let _ = pixel_strip::refresh();   // push the cleared buffer (LED off)
            line(&format!("pixel_strip::deinit = {:?}", pixel_strip::deinit()));
        }
        Err(_) => line("pixel_strip::init -> Err (no capability / pin busy) [ok]"),
    }
}

// usb: write raw bytes straight to the USB serial port. The `b"..."` prefix
// makes a byte string. Useful for talking to a script on the host computer.
fn probe_usb() {
    line("-- usb cdc --");
    line(&format!("usb::cdc_write = {:?}", usb::cdc_write(b"[probe] usb_cdc_write\n")));
}

// event: ask to be notified when something happens (a key press, charging
// starts, the badge unlocks, ...). You subscribe with a bitmask of event types
// and an action id; when an event fires the host calls your plugin_on_action.
// Remember to unsubscribe when you no longer need it.
fn probe_event() {
    line("-- event bus (subscribe/unsubscribe) --");
    match event::subscribe(event::KEY_PRESSED, 4242) {
        Ok(id) => {
            line(&format!("event::subscribe = {}", id));
            event::unsubscribe(id);   // `id` is the handle returned by subscribe
            line("event::unsubscribe done");
        }
        Err(e) => line(&format!("event::subscribe -> {:?}", e)),
    }
    // Plugins can also broadcast their own events to other plugins/modules.
    event::publish_module_event(1, 0);
    line("event::publish_module_event done");
}

// lockscreen: add one quick-action entry to the lock-screen menu. When the user
// picks it, the host calls your plugin_on_action with the given action id.
fn probe_lockscreen() {
    line("-- lockscreen quick action --");
    line(&format!("lockscreen::register = {:?}", lockscreen::register("running", 4243)));
    lockscreen::unregister();
    line("lockscreen::unregister done");
    // lockscreen::alert (persistent Y/N over the lock screen) is exercised
    // after the probe finishes, in plugin_on_tick -> PHASE_ALERT, so it does not
    // get clobbered by the other modal pushes below.
    line("lockscreen::alert deferred to end of probe");
}

// i18n: look up translated text. `tr_key` reads your plugin's strings,
// `tr_meta` your manifest fields (name, description), `tr_core` the OS strings.
// The badge picks the language; you always ask by key and get the right text.
fn probe_i18n() {
    line("-- i18n --");
    line(&format!("i18n::current_language = {:?}", i18n::current_language()));
    line(&format!("i18n::tr_key(running) = {:?}", i18n::tr_key("running")));
    line(&format!("i18n::tr_meta(name) = {:?}", i18n::tr_meta("name")));
    line(&format!("i18n::tr_core(core.ok) = {:?}", i18n::tr_core("core.ok")));
}

// cmd: read a command someone sent to this plugin over serial
// (`PLUGIN CMD <id> <args>`). Normally you call `consume` inside the optional
// `plugin_on_cmd` export; here there is no pending command, so it returns None.
fn probe_cmd() {
    line("-- cmd (plugin command channel) --");
    // No host-pushed command pending in this context -> None.
    line(&format!("cmd::consume = {:?}", cmd::consume(64)));
}

// http: fetch data from the internet. Needs WiFi, so the probe skips the live
// request when offline. Flow: open(method, url, timeout) -> optionally set
// headers/body -> perform() returns the status code -> read_to_string() reads
// the body. The request closes itself when `req` goes out of scope.
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
                    line(&format!("http::perform status = {}", status));   // e.g. 200
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

// socket: the low-level transport under http. Open a connected TCP stream or
// UDP socket to one remote endpoint, then write/read raw bytes; build your own
// protocol (MQTT, DNS, ...) on top. Needs WiFi and the "socket" capability.
// Both probes are real round-trips: TCP speaks a bare HTTP request, UDP sends a
// DNS query to a public resolver and checks the reply echoes the same id.
fn probe_socket() {
    line("-- socket (TCP + UDP client, only when WiFi is connected) --");
    if !wifi::is_connected() {
        line("socket: WiFi not connected, skipping");
        return;
    }

    // TCP: connect -> write a minimal HTTP/1.0 request -> read the status line.
    match socket::TcpStream::connect("example.com", 80, 5000) {
        Ok(stream) => {
            let request = b"GET / HTTP/1.0\r\nHost: example.com\r\n\r\n";
            match stream.write(request, 5000) {
                Ok(n) => line(&format!("socket::tcp write = {} bytes", n)),
                Err(e) => line(&format!("socket::tcp write -> {:?}", e)),
            }
            let mut buf = [0u8; 64];
            match stream.read(&mut buf, 5000) {
                Ok(n) => line(&format!(
                    "socket::tcp read = {} bytes, HTTP reply = {}",
                    n,
                    buf.starts_with(b"HTTP")
                )),
                Err(e) => line(&format!("socket::tcp read -> {:?}", e)),
            }
            // stream closes itself when `stream` goes out of scope.
        }
        Err(e) => line(&format!("socket::tcp connect -> {:?}", e)),
    }

    // UDP: a connected socket fixes the peer, so write/read need no address.
    // Query the A record for example.com from a public DNS resolver.
    match socket::UdpSocket::connect("8.8.8.8", 53, 5000) {
        Ok(sock) => {
            let query: [u8; 29] = [
                0x12, 0x34, // transaction id
                0x01, 0x00, // flags: standard query, recursion desired
                0x00, 0x01, // QDCOUNT = 1
                0x00, 0x00, // ANCOUNT
                0x00, 0x00, // NSCOUNT
                0x00, 0x00, // ARCOUNT
                0x07, b'e', b'x', b'a', b'm', b'p', b'l', b'e', // "example"
                0x03, b'c', b'o', b'm', // "com"
                0x00, // root label
                0x00, 0x01, // QTYPE = A
                0x00, 0x01, // QCLASS = IN
            ];
            match sock.write(&query, 5000) {
                Ok(n) => line(&format!("socket::udp write = {} bytes", n)),
                Err(e) => line(&format!("socket::udp write -> {:?}", e)),
            }
            let mut buf = [0u8; 64];
            match sock.read(&mut buf, 5000) {
                Ok(n) => line(&format!(
                    "socket::udp read = {} bytes, id echoed = {}",
                    n,
                    n >= 2 && buf[0] == 0x12 && buf[1] == 0x34
                )),
                Err(e) => line(&format!("socket::udp read -> {:?}", e)),
            }
            // sock closes itself when `sock` goes out of scope.
        }
        Err(e) => line(&format!("socket::udp connect -> {:?}", e)),
    }
}

// ui: the easy, high-level way to show screens and ask the user for input.
// `push_*` puts a view on a stack; `pop` removes the top one. Input views
// (confirm, list, slider, text, ...) fire an action id into plugin_on_action,
// and you read the entered value with `consume_input_int`/`consume_input_text`.
// This probe pushes one of each kind and immediately pops it so nothing sticks.
fn probe_ui() {
    line("-- ui (push every view type, then pop) --");

    // The host API speaks UTF-8: text (HTML entities, umlauts and all) is passed
    // straight to the UI functions, which convert it for the display.

    // Toasts and messages disappear on their own; info and confirm stay until
    // dismissed. The last argument of push_confirm is the action id fired with
    // idx=1 (yes) or idx=0 (no).
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

    // A scrollable list is built with a builder: set the title, the actions for
    // select/menu, add items (label, your id, icon), then push().
    // List view + list mutators.
    ui::ListBuilder::new("Probe list")
        .on_select(4301)
        .on_menu(4302)
        .item("Item A", 1, ui::UI_ICON_BULLET)
        .item("Item B", 2, ui::UI_ICON_BULLET)
        .push();
    // The list can then be edited in place without rebuilding the whole thing.
    ui::set_footer("footer hint");
    ui::set_list_empty("empty text");
    ui::update_list_item(0, "Item A*", 1, ui::UI_ICON_CIRCLE);
    ui::insert_list_item(1, "Item Ins", 3, ui::UI_ICON_BULLET);
    ui::remove_list_item(2);
    line("ui::ListBuilder + set_footer/set_list_empty/update/insert/remove_list_item done");
    // `replace` swaps the current list for a new one in place (the common
    // "refresh after an action" pattern) instead of stacking another view.
    ui::ListBuilder::new("Probe list 2")
        .on_select(4301)
        .item("X", 9, ui::UI_ICON_BULLET)
        .replace();
    line("ui::ListBuilder::replace done");
    ui::pop();
    line("ui::pop (list) done");

    // A context menu is a small popup, built the same way as a list.
    // Context menu.
    ui::ContextMenuBuilder::new("Ctx")
        .on_select(4303)
        .item("C1", 1, ui::UI_ICON_BULLET)
        .push();
    ui::pop();
    line("ui::ContextMenuBuilder + pop done");

    // A slider asks for a number; read it later via consume_input_int.
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

    // Text entry: T9 for normal text, password for masked input. Read the
    // result later via consume_input_text.
    // Text inputs.
    ui::push_t9_input("T9", Some("hi"), 16, 4306);
    ui::pop();
    line("ui::push_t9_input + pop done");
    ui::push_password("PW", None, 16, 4307);
    ui::pop();
    line("ui::push_password + pop done");

    // Specialised pickers for a date, a time, and a numeric PIN.
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

    // These read the value the user confirmed in the views above. Here nothing
    // is pending (we popped everything), so both return None.
    // Input consumers (no pending input -> None).
    line(&format!("ui::consume_input_int = {:?}", ui::consume_input_int()));
    line(&format!("ui::consume_input_text = {:?}", ui::consume_input_text(32)));

    // acquire_exclusive takes over the whole screen; set_inactivity fires an
    // action after the user is idle; wink blinks the backlight; repaint forces
    // a redraw.
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
