//! \file
//! \brief GPIO-to-MQTT example plugin.
//!
//! Monitors the Grove and SAO GPIO lines as inputs and publishes a snapshot
//! plus state changes to an MQTT broker over the generic TCP socket host API.

#![no_std]

extern crate alloc;

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::cell::RefCell;

use cdc_badge_plugin::{canvas, gpio, i18n, log, nvs, plugin_main, socket, ui, wifi};

plugin_main!();

const TAG: &str = "gpio_mqtt";

const DEFAULT_TOPIC: &str = "cdc-badge/gpio";
const CLIENT_ID: &str = "cdc-badge-gpio";
const KEEPALIVE_SECS: u16 = 60;
const SOCKET_TIMEOUT_MS: u32 = 5_000;
const POLL_INTERVAL_MS: u64 = 100;

const NVS_URL: &str = "mqtt_url";
const NVS_USER: &str = "username";
const NVS_PASS: &str = "password";
const NVS_TOPIC: &str = "topic";

const ACT_CANVAS_KEY: u32 = 1;
const ACT_CTX_PICK: u32 = 2;
const ACT_URL_DONE: u32 = 3;
const ACT_USER_DONE: u32 = 4;
const ACT_PASS_DONE: u32 = 5;
const ACT_TOPIC_DONE: u32 = 6;

const CTX_URL: u32 = 1;
const CTX_USER: u32 = 2;
const CTX_PASS: u32 = 3;
const CTX_TOPIC: u32 = 4;

// Canvas key callback delivers the firmware KeyCodes char: '3' opens the
// settings menu, 'N' (back) leaves the view. A canvas view with a key
// callback consumes every key, so the plugin must pop itself on back.
const KEY_MENU: u32 = b'3' as u32;
const KEY_BACK: u32 = b'N' as u32;

const URL_MAX_LEN: u16 = 160;
const USER_MAX_LEN: u16 = 96;
const PASS_MAX_LEN: u16 = 96;
const TOPIC_MAX_LEN: u16 = 128;

// Grove (2/3) and SAO (15/16) are covered by the grove/sao manifest caps;
// the 40-pin header GPIOs are covered by the manifest gpio_pins list. All pin
// numbers here must stay within the firmware user-pin whitelist.
const PINS: &[(u8, &str)] = &[
    (gpio::pins::GROVE_0, "grove0"),
    (gpio::pins::GROVE_1, "grove1"),
    (gpio::pins::SAO_GPIO1, "sao1"),
    (gpio::pins::SAO_GPIO2, "sao2"),
    (4, "io4"),
    (5, "io5"),
    (6, "io6"),
    (7, "io7"),
    (9, "io9"),
    (14, "io14"),
    (38, "io38"),
    (40, "io40"),
    (43, "io43"),
    (44, "io44"),
];

const PIN_COUNT: usize = PINS.len();

#[derive(Clone, Copy, PartialEq, Eq)]
enum PublishState {
    Never,
    Ok,
    Failed,
}

struct PluginCell<T>(RefCell<T>);
unsafe impl<T> Sync for PluginCell<T> {}
impl<T> PluginCell<T> {
    const fn new(v: T) -> Self {
        Self(RefCell::new(v))
    }
}
impl<T> core::ops::Deref for PluginCell<T> {
    type Target = RefCell<T>;
    fn deref(&self) -> &RefCell<T> {
        &self.0
    }
}

static LAST_LEVELS: PluginCell<[bool; PIN_COUNT]> = PluginCell::new([false; PIN_COUNT]);
static HAVE_LEVELS: PluginCell<bool> = PluginCell::new(false);
static LAST_POLL_MS: PluginCell<u64> = PluginCell::new(0);
static LAST_PUBLISH: PluginCell<PublishState> = PluginCell::new(PublishState::Never);

struct MqttTarget {
    host: String,
    port: u16,
}

fn parse_mqtt_url(input: &str) -> Option<MqttTarget> {
    let trimmed = input.trim();
    let rest = trimmed.strip_prefix("mqtt://").unwrap_or(trimmed);
    let rest = rest.strip_suffix('/').unwrap_or(rest);
    if rest.is_empty() || rest.contains('/') {
        return None;
    }
    let (host, port) = match rest.rsplit_once(':') {
        Some((host, port_s)) if !host.is_empty() && !port_s.is_empty() => {
            let port = port_s.parse::<u16>().ok()?;
            (host, port)
        }
        _ => (rest, 1883),
    };
    if host.is_empty() {
        return None;
    }
    Some(MqttTarget {
        host: host.to_string(),
        port,
    })
}

fn push_utf8(s: &str, out: &mut Vec<u8>) {
    let len = s.len().min(u16::MAX as usize) as u16;
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(&s.as_bytes()[..len as usize]);
}

fn encode_remaining_len(mut len: usize, out: &mut Vec<u8>) {
    loop {
        let mut byte = (len % 128) as u8;
        len /= 128;
        if len > 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if len == 0 {
            break;
        }
    }
}

fn encode_connect(client_id: &str, username: Option<&str>, password: Option<&str>) -> Vec<u8> {
    let username = username.filter(|s| !s.is_empty());
    let password = password.filter(|s| !s.is_empty());
    let mut variable = Vec::new();
    push_utf8("MQTT", &mut variable);
    variable.push(0x04);
    let mut flags = 0x02;
    if password.is_some() {
        flags |= 0x40;
    }
    if username.is_some() {
        flags |= 0x80;
    }
    variable.push(flags);
    variable.extend_from_slice(&KEEPALIVE_SECS.to_be_bytes());
    push_utf8(client_id, &mut variable);
    if let Some(user) = username {
        push_utf8(user, &mut variable);
    }
    if let Some(pass) = password {
        push_utf8(pass, &mut variable);
    }

    let mut out = Vec::new();
    out.push(0x10);
    encode_remaining_len(variable.len(), &mut out);
    out.extend_from_slice(&variable);
    out
}

fn encode_publish(topic: &str, payload: &[u8]) -> Vec<u8> {
    let mut variable = Vec::new();
    push_utf8(topic, &mut variable);
    variable.extend_from_slice(payload);

    let mut out = Vec::new();
    out.push(0x30);
    encode_remaining_len(variable.len(), &mut out);
    out.extend_from_slice(&variable);
    out
}

fn config_url() -> String {
    nvs::get_str(NVS_URL, URL_MAX_LEN as usize)
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn config_user() -> String {
    nvs::get_str(NVS_USER, USER_MAX_LEN as usize).unwrap_or_default()
}

fn config_pass() -> String {
    nvs::get_str(NVS_PASS, PASS_MAX_LEN as usize).unwrap_or_default()
}

fn config_topic() -> String {
    let topic = nvs::get_str(NVS_TOPIC, TOPIC_MAX_LEN as usize)
        .unwrap_or_default()
        .trim()
        .to_string();
    if topic.is_empty() {
        DEFAULT_TOPIC.to_string()
    } else {
        topic
    }
}

fn read_levels() -> [bool; PIN_COUNT] {
    let mut levels = [false; PIN_COUNT];
    for (i, (pin, _name)) in PINS.iter().enumerate() {
        levels[i] = gpio::read(*pin).unwrap_or(false);
    }
    levels
}

fn level_u8(v: bool) -> u8 {
    if v { 1 } else { 0 }
}

fn snapshot_payload(levels: &[bool; PIN_COUNT]) -> String {
    let mut out = String::from("{\"type\":\"snapshot\",\"pins\":{");
    for (i, (_pin, name)) in PINS.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&format!("\"{}\":{}", name, level_u8(levels[i])));
    }
    out.push_str("}}");
    out
}

fn change_payload(pin_name: &str, level: bool) -> String {
    format!(
        "{{\"type\":\"change\",\"pin\":\"{}\",\"value\":{}}}",
        pin_name,
        level_u8(level)
    )
}

fn write_all(stream: &socket::TcpStream, mut data: &[u8]) -> Result<(), &'static str> {
    while !data.is_empty() {
        let n = stream
            .write(data, SOCKET_TIMEOUT_MS)
            .map_err(|_| "socket write failed")?;
        if n == 0 {
            return Err("socket write failed");
        }
        data = &data[n..];
    }
    Ok(())
}

fn mqtt_publish(payload: &str) -> Result<(), &'static str> {
    let url = config_url();
    let target = parse_mqtt_url(&url).ok_or("invalid url: expected host or mqtt://host[:port]")?;
    let user = config_user();
    let pass = config_pass();
    let user_opt = if user.is_empty() { None } else { Some(user.as_str()) };
    let pass_opt = if pass.is_empty() { None } else { Some(pass.as_str()) };

    let stream = socket::TcpStream::connect(&target.host, target.port, SOCKET_TIMEOUT_MS)
        .map_err(|_| "tcp connect failed")?;
    let connect = encode_connect(CLIENT_ID, user_opt, pass_opt);
    write_all(&stream, &connect)?;

    let mut connack = [0u8; 4];
    let n = stream
        .read(&mut connack, SOCKET_TIMEOUT_MS)
        .map_err(|_| "connack read failed")?;
    if n < 4 || connack[0] != 0x20 {
        return Err("no connack from broker");
    }
    // connack[3] is the MQTT CONNACK return code (0x00 = accepted).
    match connack[3] {
        0x00 => {}
        0x01 => return Err("broker: unacceptable protocol version"),
        0x02 => return Err("broker: client id rejected"),
        0x03 => return Err("broker: server unavailable"),
        0x04 => return Err("broker: bad username/password"),
        0x05 => return Err("broker: not authorized (set username/password)"),
        _ => return Err("broker rejected connect"),
    }

    let publish = encode_publish(&config_topic(), payload.as_bytes());
    write_all(&stream, &publish)?;
    Ok(())
}

fn remember_publish(result: Result<(), &'static str>) {
    let prev = *LAST_PUBLISH.borrow();
    match result {
        Ok(()) => {
            if prev != PublishState::Ok {
                log::info(TAG, "publish ok");
            }
            *LAST_PUBLISH.borrow_mut() = PublishState::Ok;
        }
        Err(reason) => {
            if prev != PublishState::Failed {
                log::error(TAG, reason);
            }
            *LAST_PUBLISH.borrow_mut() = PublishState::Failed;
        }
    }
}

fn publish_snapshot(levels: &[bool; PIN_COUNT]) {
    let payload = snapshot_payload(levels);
    remember_publish(mqtt_publish(&payload));
}

fn publish_change(pin_name: &str, level: bool) {
    let payload = change_payload(pin_name, level);
    remember_publish(mqtt_publish(&payload));
}

fn draw_canvas() {
    let levels = *LAST_LEVELS.borrow();
    let have_levels = *HAVE_LEVELS.borrow();
    let last_publish = *LAST_PUBLISH.borrow();
    let target = config_url();
    let not_configured;
    let target_label: &str = if target.is_empty() {
        not_configured = i18n::tr_key("not_configured");
        &not_configured
    } else {
        &target
    };
    let wifi_label = wifi::ip().unwrap_or_else(|| "-".to_string());
    let publish_label = match last_publish {
        PublishState::Never => i18n::tr_key("never"),
        PublishState::Ok => i18n::tr_key("ok"),
        PublishState::Failed => i18n::tr_key("failed"),
    };

    let (w, h) = canvas::body_size();
    canvas::clear();
    canvas::set_text_size(1);
    canvas::draw_text(0, 0, &format!("MQTT: {}", target_label));
    canvas::draw_text(0, 12, &format!("{}: {}", i18n::tr_key("wifi"), wifi_label));
    canvas::draw_text(0, 24, &format!("{}: {}", i18n::tr_key("last_publish"), publish_label));
    canvas::hline(0, 35, w as i16);

    // Pin grid: fill columns top-to-bottom, then wrap to the next column so all
    // pins stay visible on the wide display without scrolling.
    const PIN_TOP: i16 = 40;
    const ROW_H: i16 = 10;
    const MIN_COL_W: i16 = 54;
    let rows = (((h as i16 - PIN_TOP) / ROW_H).max(1)) as usize;
    let max_cols = ((w as i16 / MIN_COL_W).max(1)) as usize;
    let cols = PIN_COUNT.div_ceil(rows).clamp(1, max_cols);
    let col_w = w as i16 / cols as i16;
    let capacity = rows * cols;
    for (i, (_pin, name)) in PINS.iter().take(capacity).enumerate() {
        let value = if have_levels { level_u8(levels[i]) } else { 0 };
        let x = (i / rows) as i16 * col_w;
        let y = PIN_TOP + (i % rows) as i16 * ROW_H;
        canvas::draw_text(x, y, &format!("{}:{}", name, value));
    }
    canvas::commit(false);
}

fn open_settings_menu() {
    ui::ContextMenuBuilder::new(&i18n::tr_key("settings"))
        .on_select(ACT_CTX_PICK)
        .item(i18n::tr_key("mqtt_url"), CTX_URL, ui::UI_ICON_ANGLE)
        .item(i18n::tr_key("username"), CTX_USER, ui::UI_ICON_MALE)
        .item(i18n::tr_key("password"), CTX_PASS, ui::UI_ICON_PARAGRAPH)
        .item(i18n::tr_key("topic"), CTX_TOPIC, ui::UI_ICON_SECTION)
        .push();
}

fn poll_gpio(now_ms: u64) {
    if now_ms.wrapping_sub(*LAST_POLL_MS.borrow()) < POLL_INTERVAL_MS {
        return;
    }
    *LAST_POLL_MS.borrow_mut() = now_ms;

    let next = read_levels();
    if !*HAVE_LEVELS.borrow() {
        *LAST_LEVELS.borrow_mut() = next;
        *HAVE_LEVELS.borrow_mut() = true;
        draw_canvas();
        publish_snapshot(&next);
        draw_canvas();
        return;
    }

    let mut current = LAST_LEVELS.borrow_mut();
    for (i, (_pin, name)) in PINS.iter().enumerate() {
        if current[i] != next[i] {
            current[i] = next[i];
            drop(current);
            draw_canvas();
            publish_change(name, next[i]);
            draw_canvas();
            current = LAST_LEVELS.borrow_mut();
        }
    }
}

#[no_mangle]
pub extern "C" fn plugin_init() -> i32 {
    for (pin, _name) in PINS {
        if gpio::set_direction(*pin, gpio::Direction::Input).is_err() {
            log::error(TAG, "gpio direction failed");
            return -1;
        }
        let _ = gpio::set_pull(*pin, gpio::Pull::Down);
    }
    0
}

#[no_mangle]
pub extern "C" fn plugin_deinit() -> i32 {
    for (pin, _name) in PINS {
        gpio::release(*pin);
    }
    0
}

#[no_mangle]
pub extern "C" fn plugin_on_enter() -> i32 {
    *LAST_LEVELS.borrow_mut() = read_levels();
    *HAVE_LEVELS.borrow_mut() = true;
    *LAST_PUBLISH.borrow_mut() = PublishState::Never;
    log::info(TAG, &format!("enter: url='{}'", config_url()));
    canvas::push(&i18n::tr_meta("name"), ACT_CANVAS_KEY, 0);
    canvas::set_footer(&i18n::tr_key("hint_main"));
    draw_canvas();
    let levels = *LAST_LEVELS.borrow();
    publish_snapshot(&levels);
    draw_canvas();
    0
}

#[no_mangle]
pub extern "C" fn plugin_on_exit() -> i32 {
    *HAVE_LEVELS.borrow_mut() = false;
    *LAST_POLL_MS.borrow_mut() = 0;
    0
}

#[no_mangle]
pub extern "C" fn plugin_on_tick(uptime_ms: u64) -> i32 {
    poll_gpio(uptime_ms);
    0
}

#[no_mangle]
pub extern "C" fn plugin_on_action(action_id: u32, idx: u32, user_data: u32) -> i32 {
    match action_id {
        ACT_CANVAS_KEY => {
            if user_data == KEY_MENU || idx == KEY_MENU {
                open_settings_menu();
            } else if user_data == KEY_BACK {
                ui::pop();
            }
        }
        ACT_CTX_PICK => match user_data {
            CTX_URL => {
                let initial = config_url();
                ui::push_t9_input(
                    &i18n::tr_key("mqtt_url"),
                    Some(&initial),
                    URL_MAX_LEN,
                    ACT_URL_DONE,
                );
            }
            CTX_USER => {
                let initial = config_user();
                ui::push_t9_input(
                    &i18n::tr_key("username"),
                    Some(&initial),
                    USER_MAX_LEN,
                    ACT_USER_DONE,
                );
            }
            CTX_PASS => {
                let initial = config_pass();
                ui::push_password(
                    &i18n::tr_key("password"),
                    Some(&initial),
                    PASS_MAX_LEN,
                    ACT_PASS_DONE,
                );
            }
            CTX_TOPIC => {
                let initial = config_topic();
                ui::push_t9_input(
                    &i18n::tr_key("topic"),
                    Some(&initial),
                    TOPIC_MAX_LEN,
                    ACT_TOPIC_DONE,
                );
            }
            _ => {}
        },
        ACT_URL_DONE => {
            if user_data == 1 {
                if let Some(value) = ui::consume_input_text(URL_MAX_LEN as usize) {
                    let _ = nvs::set_str(NVS_URL, value.trim());
                }
            }
            draw_canvas();
        }
        ACT_USER_DONE => {
            if user_data == 1 {
                if let Some(value) = ui::consume_input_text(USER_MAX_LEN as usize) {
                    let _ = nvs::set_str(NVS_USER, value.trim());
                }
            }
            draw_canvas();
        }
        ACT_PASS_DONE => {
            if user_data == 1 {
                if let Some(value) = ui::consume_input_text(PASS_MAX_LEN as usize) {
                    let _ = nvs::set_str(NVS_PASS, value.trim());
                }
            }
            draw_canvas();
        }
        ACT_TOPIC_DONE => {
            if user_data == 1 {
                if let Some(value) = ui::consume_input_text(TOPIC_MAX_LEN as usize) {
                    let _ = nvs::set_str(NVS_TOPIC, value.trim());
                }
            }
            draw_canvas();
        }
        _ => {}
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_mqtt_url_with_default_port() {
        let parsed = parse_mqtt_url("mqtt://broker.local").unwrap();
        assert_eq!(parsed.host, "broker.local");
        assert_eq!(parsed.port, 1883);
    }

    #[test]
    fn parses_mqtt_url_with_explicit_port() {
        let parsed = parse_mqtt_url("mqtt://10.0.0.5:1884").unwrap();
        assert_eq!(parsed.host, "10.0.0.5");
        assert_eq!(parsed.port, 1884);
    }

    #[test]
    fn rejects_unsupported_mqtt_scheme() {
        assert!(parse_mqtt_url("mqtts://broker.local").is_none());
    }

    #[test]
    fn parses_bare_host_with_default_scheme_and_port() {
        let parsed = parse_mqtt_url("10.0.0.4").unwrap();
        assert_eq!(parsed.host, "10.0.0.4");
        assert_eq!(parsed.port, 1883);
    }

    #[test]
    fn parses_bare_host_with_explicit_port() {
        let parsed = parse_mqtt_url("10.0.0.4:1884").unwrap();
        assert_eq!(parsed.host, "10.0.0.4");
        assert_eq!(parsed.port, 1884);
    }

    #[test]
    fn encodes_remaining_length_boundaries() {
        let mut out = alloc::vec::Vec::new();
        encode_remaining_len(127, &mut out);
        assert_eq!(out, alloc::vec![0x7f]);

        out.clear();
        encode_remaining_len(128, &mut out);
        assert_eq!(out, alloc::vec![0x80, 0x01]);
    }

    #[test]
    fn encodes_connect_without_auth() {
        let pkt = encode_connect("cdc-badge-gpio", None, None);
        assert_eq!(
            pkt,
            alloc::vec![
                0x10, 0x1a,
                0x00, 0x04, b'M', b'Q', b'T', b'T',
                0x04, 0x02,
                0x00, 0x3c,
                0x00, 0x0e, b'c', b'd', b'c', b'-', b'b', b'a', b'd', b'g', b'e', b'-', b'g', b'p', b'i', b'o',
            ]
        );
    }

    #[test]
    fn encodes_connect_with_username_and_password() {
        let pkt = encode_connect("cid", Some("user"), Some("secret"));
        assert_eq!(
            pkt,
            alloc::vec![
                0x10, 0x1d,
                0x00, 0x04, b'M', b'Q', b'T', b'T',
                0x04, 0xc2,
                0x00, 0x3c,
                0x00, 0x03, b'c', b'i', b'd',
                0x00, 0x04, b'u', b's', b'e', b'r',
                0x00, 0x06, b's', b'e', b'c', b'r', b'e', b't',
            ]
        );
    }

    #[test]
    fn encodes_qos0_publish_packet() {
        let pkt = encode_publish("cdc-badge/gpio", br#"{"pin":"grove0","value":1}"#);
        assert_eq!(
            pkt,
            alloc::vec![
                0x30, 0x2a,
                0x00, 0x0e, b'c', b'd', b'c', b'-', b'b', b'a', b'd', b'g', b'e', b'/', b'g', b'p', b'i', b'o',
                b'{', b'"', b'p', b'i', b'n', b'"', b':', b'"', b'g', b'r', b'o', b'v', b'e', b'0', b'"', b',',
                b'"', b'v', b'a', b'l', b'u', b'e', b'"', b':', b'1', b'}',
            ]
        );
    }
}
