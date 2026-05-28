//! \file
//! \brief Home Assistant WASM plugin: setup wizard, favorites dashboard,
//!        browse/search of every entity, light/cover detail editors.
//!
//! Storage:
//!   - URL parts in plugin NVS: `url` (host), `port`, `https` (u32 0/1)
//!   - Long-lived access token in TROPIC01 rmem slot `ha_token`
//!   - Favorites in plugin NVS `favs` as line-delimited
//!     `entity_id|friendly_name|domain_idx`.

#![no_std]

extern crate alloc;

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::cell::RefCell;

use cdc_badge_plugin::{canvas, http, i18n, log, nvs, plugin_main, rmem, ui};

plugin_main!();

const TAG: &str = "ha";

const NVS_HOST: &str = "url";
const NVS_PORT: &str = "port";
const NVS_HTTPS: &str = "https";
const NVS_FAVS: &str = "favs";
const TOKEN_NAME: &str = "ha_token";

const MAX_FAVS: usize = 32;

const ACT_HOME_SELECT: u32 = 10;
const ACT_HOME_MENU: u32 = 11;
const ACT_HOME_CTX_PICK: u32 = 12;

const ACT_BROWSE_SELECT: u32 = 20;
const ACT_BROWSE_MENU: u32 = 21;
const ACT_BROWSE_FILTER_PICK: u32 = 22;
const ACT_BROWSE_SEARCH_DONE: u32 = 23;

const ACT_SETUP_HOST_DONE: u32 = 30;
const ACT_SETUP_TOKEN_DONE: u32 = 33;

const ACT_LIGHT_BRIGHT_DONE: u32 = 40;
const ACT_LIGHT_COLOR_PICK: u32 = 41;

const ACT_COVER_POS_DONE: u32 = 50;

const ACT_REMOVE_FAV_CONFIRM: u32 = 60;
const ACT_RESET_CONFIRM: u32 = 61;

const ACT_CLIMATE_KEY: u32 = 70;
const ACT_CLIMATE_WIDGET: u32 = 71;

const W_CLIMATE_SLIDER: u32 = 100;

const CTX_BRIGHTNESS: u32 = 1;
const CTX_COLOR: u32 = 2;
const CTX_COVER_POSITION: u32 = 3;
const CTX_BROWSE: u32 = 4;
const CTX_REMOVE: u32 = 5;
const CTX_RESET: u32 = 6;
const CTX_SETUP: u32 = 7;
const CTX_EDIT_TOKEN: u32 = 8;

const FILTER_ALL: u32 = 0xFF;
const FILTER_SEARCH: u32 = 0xFE;

// =================== Domain ===================

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u8)]
enum Domain {
    Light = 0,
    Switch = 1,
    InputBoolean = 2,
    Button = 3,
    Scene = 4,
    Script = 5,
    Automation = 6,
    Sensor = 7,
    BinarySensor = 8,
    Cover = 9,
    Climate = 10,
    Unknown = 0xFF,
}

impl Domain {
    fn parse(entity_id: &str) -> Self {
        let prefix = entity_id.split('.').next().unwrap_or("");
        match prefix {
            "light" => Domain::Light,
            "switch" => Domain::Switch,
            "input_boolean" => Domain::InputBoolean,
            "button" => Domain::Button,
            "scene" => Domain::Scene,
            "script" => Domain::Script,
            "automation" => Domain::Automation,
            "sensor" => Domain::Sensor,
            "binary_sensor" => Domain::BinarySensor,
            "cover" => Domain::Cover,
            "climate" => Domain::Climate,
            _ => Domain::Unknown,
        }
    }

    fn from_idx(idx: u8) -> Self {
        match idx {
            0 => Domain::Light,
            1 => Domain::Switch,
            2 => Domain::InputBoolean,
            3 => Domain::Button,
            4 => Domain::Scene,
            5 => Domain::Script,
            6 => Domain::Automation,
            7 => Domain::Sensor,
            8 => Domain::BinarySensor,
            9 => Domain::Cover,
            10 => Domain::Climate,
            _ => Domain::Unknown,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Domain::Light => "light",
            Domain::Switch => "switch",
            Domain::InputBoolean => "input_boolean",
            Domain::Button => "button",
            Domain::Scene => "scene",
            Domain::Script => "script",
            Domain::Automation => "automation",
            Domain::Sensor => "sensor",
            Domain::BinarySensor => "binary_sensor",
            Domain::Cover => "cover",
            Domain::Climate => "climate",
            Domain::Unknown => "?",
        }
    }

    fn is_toggleable(self) -> bool {
        matches!(
            self,
            Domain::Light | Domain::Switch | Domain::InputBoolean
        )
    }

    fn is_readonly(self) -> bool {
        matches!(self, Domain::Sensor | Domain::BinarySensor)
    }

    fn icon(self) -> u8 {
        match self {
            Domain::Light => ui::UI_ICON_LIGHT,
            Domain::Switch | Domain::InputBoolean | Domain::Button => ui::UI_ICON_SWITCH,
            Domain::Scene | Domain::Script | Domain::Automation => ui::UI_ICON_SCENE,
            Domain::Sensor | Domain::BinarySensor => ui::UI_ICON_SENSOR,
            Domain::Cover => ui::UI_ICON_COVER,
            Domain::Climate => ui::UI_ICON_SUN,
            Domain::Unknown => ui::UI_ICON_NONE,
        }
    }

    fn primary_service(self) -> Option<(&'static str, &'static str)> {
        match self {
            Domain::Light | Domain::Switch | Domain::InputBoolean => {
                Some((self.as_str(), "toggle"))
            }
            Domain::Scene | Domain::Script | Domain::Automation => Some((self.as_str(), "turn_on")),
            Domain::Button => Some(("button", "press")),
            _ => None,
        }
    }

    fn state_glyph(self, state: &str) -> char {
        match self {
            Domain::Scene | Domain::Script | Domain::Automation => '>',
            _ if state == "unavailable" || state == "unknown" => '!',
            _ if state == "on" => '*',
            _ if state == "off" => 'o',
            _ if state == "open" => '*',
            _ if state == "closed" => 'o',
            _ if state == "?" => '?',
            _ => '?',
        }
    }
}

// =================== Entity / Favorite ===================

#[derive(Clone)]
struct Entity {
    id: String,
    name: String,
    state: String,
    unit: String,
    brightness_pct: u8,
    position: u8,
    domain: Domain,
}

impl Entity {
    fn placeholder(id: &str, domain: Domain) -> Self {
        Self {
            id: id.to_string(),
            name: id.to_string(),
            state: "?".to_string(),
            unit: String::new(),
            brightness_pct: 0,
            position: 0,
            domain,
        }
    }

    fn label(&self) -> Vec<u8> {
        let s = match self.domain {
            Domain::Sensor | Domain::BinarySensor => {
                if self.unit.is_empty() {
                    format!("{}: {}", self.name, self.state)
                } else {
                    format!("{}: {}{}", self.name, self.state, self.unit)
                }
            }
            Domain::Scene | Domain::Script | Domain::Automation => format!("> {}", self.name),
            Domain::Light if self.state == "on" && self.brightness_pct > 0 => {
                format!("* {} ({}%)", self.name, self.brightness_pct)
            }
            Domain::Cover => {
                let glyph = self.domain.state_glyph(&self.state);
                format!("{} {} ({}%)", glyph, self.name, self.position)
            }
            _ => {
                let glyph = self.domain.state_glyph(&self.state);
                format!("{} {}", glyph, self.name)
            }
        };
        ui::to_display(&s)
    }
}

#[derive(Clone)]
struct Favorite {
    id: String,
    name: String,
    domain: Domain,
}

// =================== Plugin state ===================

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

struct BrowseState {
    all: Vec<Entity>,
    visible: Vec<u32>,
    filter_domain: u32,
    search: String,
}

impl BrowseState {
    const fn new() -> Self {
        Self {
            all: Vec::new(),
            visible: Vec::new(),
            filter_domain: FILTER_ALL,
            search: String::new(),
        }
    }
}

struct WizardState {
    host: String,
    port: String,
    https: bool,
}

impl WizardState {
    const fn new() -> Self {
        Self {
            host: String::new(),
            port: String::new(),
            https: false,
        }
    }
}

static FAVS: PluginCell<Vec<Favorite>> = PluginCell::new(Vec::new());
static ENTITIES: PluginCell<Vec<Entity>> = PluginCell::new(Vec::new());
static BROWSE: PluginCell<BrowseState> = PluginCell::new(BrowseState::new());
static WIZARD: PluginCell<WizardState> = PluginCell::new(WizardState::new());
static SELECTED: PluginCell<Option<Entity>> = PluginCell::new(None);
static PENDING_REMOVE: PluginCell<Option<usize>> = PluginCell::new(None);
static CLIMATE: PluginCell<Option<ClimateState>> = PluginCell::new(None);

struct ClimateState {
    entity_id: String,
    current_temp_tenths: i32,
    current_mode: String,
    pending_mode: String,
    current_fan: Option<String>,
    pending_fan: Option<String>,
    fans: Vec<String>,
    min_temp_tenths: i32,
    max_temp_tenths: i32,
    step_tenths: i32,
    range_spread_tenths: i32,
    unit: String,
    slot_modes: [Option<String>; 3],
    has_range: bool,
}

// =================== Storage ===================

fn ha_host() -> Option<String> {
    let raw = nvs::get_str(NVS_HOST, 200)?;
    let host = parse_host_from_url(&raw).0;
    if host.is_empty() {
        None
    } else {
        Some(host)
    }
}

fn ha_port() -> String {
    if let Some(raw) = nvs::get_str(NVS_HOST, 200) {
        let parsed = parse_host_from_url(&raw);
        if let Some(p) = parsed.2 {
            return p;
        }
    }
    nvs::get_str(NVS_PORT, 16).unwrap_or_default()
}

fn ha_https() -> bool {
    if let Some(raw) = nvs::get_str(NVS_HOST, 200) {
        let parsed = parse_host_from_url(&raw);
        if parsed.1 {
            return true;
        }
    }
    nvs::get_u32(NVS_HTTPS).unwrap_or(0) != 0
}

fn parse_host_from_url(raw: &str) -> (String, bool, Option<String>) {
    let trimmed = raw.trim();
    let (rest, https) = if let Some(r) = trimmed.strip_prefix("https://") {
        (r, true)
    } else if let Some(r) = trimmed.strip_prefix("http://") {
        (r, false)
    } else {
        (trimmed, false)
    };
    let host_and_port = rest.split('/').next().unwrap_or(rest);
    let mut parts = host_and_port.split(':');
    let host = parts.next().unwrap_or("").trim().to_string();
    let port = parts.next().map(|p| p.trim().to_string());
    (host, https, port)
}

fn ha_token() -> Option<String> {
    let bytes = rmem::read(TOKEN_NAME, 320).ok()?;
    String::from_utf8(bytes)
        .ok()
        .map(|s| s.trim_end_matches('\0').trim().to_string())
        .filter(|s| !s.is_empty())
}

fn base_url() -> Option<String> {
    let host = ha_host()?;
    let scheme = if ha_https() { "https" } else { "http" };
    let port = ha_port();
    if port.trim().is_empty() {
        Some(format!("{}://{}", scheme, host.trim()))
    } else {
        Some(format!("{}://{}:{}", scheme, host.trim(), port.trim()))
    }
}

fn favs_load() {
    let raw = nvs::get_str(NVS_FAVS, 4096).unwrap_or_default();
    let mut out: Vec<Favorite> = Vec::new();
    for line in raw.split('\n').filter(|l| !l.is_empty()) {
        let mut parts = line.split('|');
        let id = parts.next().unwrap_or("").to_string();
        let name = parts.next().unwrap_or(&id).to_string();
        let domain_idx: u8 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0xFF);
        if id.is_empty() {
            continue;
        }
        out.push(Favorite {
            id,
            name,
            domain: Domain::from_idx(domain_idx),
        });
        if out.len() >= MAX_FAVS {
            break;
        }
    }
    *FAVS.borrow_mut() = out;
}

fn favs_save() {
    let favs = FAVS.borrow();
    let mut s = String::new();
    for f in favs.iter() {
        s.push_str(&format!("{}|{}|{}\n", f.id, f.name, f.domain as u8));
    }
    let _ = nvs::set_str(NVS_FAVS, &s);
}

fn favs_contains(id: &str) -> bool {
    FAVS.borrow().iter().any(|f| f.id == id)
}

// =================== HTTP ===================

fn http_get(path: &str) -> Option<String> {
    let url = format!("{}{}", base_url()?, path);
    let token = ha_token()?;
    log::info(TAG, &format!("GET {}", url));
    let req = http::Request::open(http::GET, &url, 8000).ok()?;
    let _ = req.header("Authorization", &format!("Bearer {}", token));
    let _ = req.header("Accept", "application/json");
    let status = req.perform().ok()?;
    if status != 200 {
        log::warn(TAG, &format!("HTTP {} for {}", status, path));
        return None;
    }
    req.read_to_string().ok()
}

fn http_post(path: &str, body: &str) -> Result<i32, ()> {
    let url = format!("{}{}", base_url().ok_or(())?, path);
    let token = ha_token().ok_or(())?;
    log::info(TAG, &format!("POST {}", url));
    let req = http::Request::open(http::POST, &url, 6000).map_err(|_| ())?;
    let _ = req.header("Authorization", &format!("Bearer {}", token));
    let _ = req.header("Content-Type", "application/json");
    let _ = req.body(body.as_bytes());
    let status = req.perform().map_err(|_| ())?;
    if status / 100 == 2 {
        Ok(status)
    } else {
        log::warn(TAG, &format!("POST {} -> {}", path, status));
        Err(())
    }
}

fn fetch_one(entity_id: &str) -> Option<Entity> {
    let path = format!("/api/states/{}", entity_id);
    let body = http_get(&path)?;
    parse_state_json(entity_id, &body)
}

fn parse_state_json(entity_id: &str, body: &str) -> Option<Entity> {
    let val: serde_json::Value = serde_json::from_str(body).ok()?;
    let state = val
        .get("state")
        .and_then(|v| v.as_str())
        .unwrap_or("?")
        .to_string();
    let attrs = val.get("attributes");
    let name = attrs
        .and_then(|a| a.get("friendly_name"))
        .and_then(|n| n.as_str())
        .unwrap_or(entity_id)
        .to_string();
    let unit = attrs
        .and_then(|a| a.get("unit_of_measurement"))
        .and_then(|n| n.as_str())
        .unwrap_or("")
        .to_string();
    let brightness_pct = attrs
        .and_then(|a| a.get("brightness"))
        .and_then(|n| n.as_u64())
        .map(|b| ((b.min(255) * 100 + 127) / 255) as u8)
        .unwrap_or(0);
    let position = attrs
        .and_then(|a| a.get("current_position"))
        .and_then(|n| n.as_u64())
        .map(|p| p.min(100) as u8)
        .unwrap_or(0);
    Some(Entity {
        id: entity_id.to_string(),
        name,
        state,
        unit,
        brightness_pct,
        position,
        domain: Domain::parse(entity_id),
    })
}

fn fetch_template_all() -> Option<Vec<Entity>> {
    let tmpl = "{% for s in states %}{{ s.entity_id }}|{{ s.attributes.friendly_name | default(s.entity_id) }}|{{ s.state }}|{{ s.attributes.unit_of_measurement | default('') }}|{{ s.attributes.brightness | default(0) }}|{{ s.attributes.current_position | default(0) }}\n{% endfor %}";
    let body = format!("{{\"template\":{}}}", json_string(tmpl));
    let url_path = "/api/template";
    let url = format!("{}{}", base_url()?, url_path);
    let token = ha_token()?;
    log::info(TAG, &format!("POST {}", url));
    let req = http::Request::open(http::POST, &url, 10000).ok()?;
    let _ = req.header("Authorization", &format!("Bearer {}", token));
    let _ = req.header("Content-Type", "application/json");
    let _ = req.body(body.as_bytes());
    let status = req.perform().ok()?;
    if status != 200 {
        log::warn(TAG, &format!("template POST -> {}", status));
        return None;
    }
    let text = req.read_to_string().ok()?;
    let mut out: Vec<Entity> = Vec::with_capacity(256);
    for line in text.split('\n') {
        if line.is_empty() {
            continue;
        }
        let mut parts = line.split('|');
        let id = parts.next().unwrap_or("");
        if id.is_empty() {
            continue;
        }
        let name = parts.next().unwrap_or(id).to_string();
        let state = parts.next().unwrap_or("?").to_string();
        let unit = parts.next().unwrap_or("").to_string();
        let brightness_raw: u32 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
        let position: u8 = parts
            .next()
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(0)
            .min(100) as u8;
        let brightness_pct = if brightness_raw > 0 {
            ((brightness_raw.min(255) * 100 + 127) / 255) as u8
        } else {
            0
        };
        out.push(Entity {
            id: id.to_string(),
            name,
            state,
            unit,
            brightness_pct,
            position,
            domain: Domain::parse(id),
        });
    }
    log::info(TAG, &format!("template fetched {} entities", out.len()));
    Some(out)
}

fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn call_service(
    domain: &str,
    service: &str,
    entity_id: &str,
    extra: Option<&str>,
) -> Result<(), ()> {
    let body = match extra {
        Some(e) => format!("{{\"entity_id\":\"{}\",{}}}", entity_id, e),
        None => format!("{{\"entity_id\":\"{}\"}}", entity_id),
    };
    let path = format!("/api/services/{}/{}", domain, service);
    http_post(&path, &body).map(|_| ())
}

// =================== Setup Wizard ===================

fn wizard_start() {
    let initial = match (ha_host(), ha_port(), ha_https()) {
        (Some(h), p, https) if !p.is_empty() => {
            let scheme = if https { "https" } else { "http" };
            format!("{}://{}:{}", scheme, h, p)
        }
        (Some(h), _, https) => {
            let scheme = if https { "https" } else { "http" };
            format!("{}://{}", scheme, h)
        }
        _ => String::new(),
    };
    ui::push_t9_input(
        i18n::tr_key("host"),
        if initial.is_empty() { None } else { Some(&initial) },
        96,
        ACT_SETUP_HOST_DONE,
    );
}

fn wizard_step_token() {
    let existing = ha_token();
    ui::push_t9_input(
        i18n::tr_key("token_title"),
        existing.as_deref(),
        320,
        ACT_SETUP_TOKEN_DONE,
    );
}

fn probe_url(url: &str) -> bool {
    log::info(TAG, &format!("probe {}", url));
    let req = match http::Request::open(http::GET, url, 3000) {
        Ok(r) => r,
        Err(_) => return false,
    };
    match req.perform() {
        Ok(s) => {
            log::info(TAG, &format!("probe {} -> {}", url, s));
            (200..500).contains(&s)
        }
        Err(_) => false,
    }
}

fn wizard_finalize_with_probe() {
    let (host, port, user_https) = {
        let w = WIZARD.borrow();
        (w.host.clone(), w.port.clone(), w.https)
    };

    let port_suffix = if port.trim().is_empty() {
        String::new()
    } else {
        format!(":{}", port.trim())
    };

    ui::push_toast(i18n::tr_key("probing"), ui::UI_ICON_INFO, 1500);

    let https_url = format!("https://{}{}/api/", host.trim(), port_suffix);
    let https_ok = probe_url(&https_url);
    let http_ok = if !https_ok && !user_https {
        let http_url = format!("http://{}{}/api/", host.trim(), port_suffix);
        probe_url(&http_url)
    } else {
        false
    };

    if !https_ok && !http_ok {
        ui::push_toast(i18n::tr_key("ha_unreachable"), ui::UI_ICON_ERROR, 2000);
        home_render();
        return;
    }

    let _ = nvs::set_str(NVS_HOST, host.trim());
    let _ = nvs::set_str(NVS_PORT, port.trim());
    let _ = nvs::set_u32(NVS_HTTPS, if https_ok { 1 } else { 0 });

    ui::push_toast(i18n::tr_key("url_saved"), ui::UI_ICON_SUCCESS, 1200);

    if ha_token().is_none() {
        wizard_step_token();
    } else {
        home_render();
    }
}

// =================== Home View ===================

fn home_render() {
    let configured = ha_host().is_some() && ha_token().is_some();
    let favs = FAVS.borrow().clone();

    ENTITIES.borrow_mut().clear();
    let mut any_ok = false;
    if configured {
        for f in favs.iter() {
            match fetch_one(&f.id) {
                Some(e) => {
                    ENTITIES.borrow_mut().push(e);
                    any_ok = true;
                }
                None => {
                    let mut ph = Entity::placeholder(&f.id, f.domain);
                    ph.name = f.name.clone();
                    ENTITIES.borrow_mut().push(ph);
                }
            }
        }
        if !favs.is_empty() && !any_ok {
            ui::push_toast(i18n::tr_key("ha_unreachable"), ui::UI_ICON_ERROR, 1500);
        }
    }

    let title = i18n::tr_meta("name");
    let mut builder = ui::ListBuilder::new(title)
        .on_select(ACT_HOME_SELECT)
        .on_menu(ACT_HOME_MENU);
    for (i, e) in ENTITIES.borrow().iter().enumerate() {
        builder = builder.item(&e.label(), i as u32, e.domain.icon());
    }
    builder.replace();

    let empty_text = if !configured {
        i18n::tr_key("status_not_configured")
    } else {
        i18n::tr_key("status_no_favorites")
    };
    ui::set_list_empty(empty_text);
    ui::set_footer(i18n::tr_key("hint_home"));
}

fn home_handle_select(idx: u32) {
    let entity = match ENTITIES.borrow().get(idx as usize).cloned() {
        Some(e) => e,
        None => {
            home_open_menu();
            return;
        }
    };
    *SELECTED.borrow_mut() = Some(entity.clone());

    if let Some((domain, service)) = entity.domain.primary_service() {
        match call_service(domain, service, &entity.id, None) {
            Ok(()) => {
                let mut ents = ENTITIES.borrow_mut();
                if let Some(e) = ents.get_mut(idx as usize) {
                    if e.domain.is_toggleable() {
                        e.state = if e.state == "on" {
                            "off".to_string()
                        } else {
                            "on".to_string()
                        };
                    }
                }
                drop(ents);
                ui::push_toast(i18n::tr_key("toggle_ok"), ui::UI_ICON_SUCCESS, 800);
                home_redraw_only();
            }
            Err(()) => ui::push_toast(i18n::tr_key("toggle_failed"), ui::UI_ICON_ERROR, 1500),
        }
    } else if entity.domain.is_readonly() {
        let msg = if entity.unit.is_empty() {
            format!("{}: {}", entity.name, entity.state)
        } else {
            format!("{}: {}{}", entity.name, entity.state, entity.unit)
        };
        ui::push_toast(&msg, ui::UI_ICON_INFO, 1500);
    } else if entity.domain == Domain::Cover {
        cover_position_start(&entity);
    } else if entity.domain == Domain::Climate {
        climate_open(&entity);
    } else {
        ui::push_toast(i18n::tr_key("unsupported"), ui::UI_ICON_INFO, 1200);
    }
}

fn home_redraw_only() {
    let title = i18n::tr_meta("name");
    let mut builder = ui::ListBuilder::new(title)
        .on_select(ACT_HOME_SELECT)
        .on_menu(ACT_HOME_MENU);
    for (i, e) in ENTITIES.borrow().iter().enumerate() {
        builder = builder.item(&e.label(), i as u32, e.domain.icon());
    }
    builder.replace();
    ui::set_footer(i18n::tr_key("hint_home"));
}

fn home_open_menu() {
    let configured = ha_host().is_some() && ha_token().is_some();
    let entity = SELECTED.borrow().clone();
    let mut builder =
        ui::ContextMenuBuilder::new(i18n::tr_key("actions")).on_select(ACT_HOME_CTX_PICK);

    if let Some(e) = entity.as_ref() {
        if e.domain == Domain::Light {
            builder = builder.item(i18n::tr_key("brightness"), CTX_BRIGHTNESS, ui::UI_ICON_SUN);
            builder = builder.item(i18n::tr_key("color"), CTX_COLOR, ui::UI_ICON_DIAMOND);
        }
        if e.domain == Domain::Cover {
            builder = builder.item(i18n::tr_key("cover_position"), CTX_COVER_POSITION, ui::UI_ICON_UPDOWN);
        }
    }
    if configured {
        builder = builder.item(i18n::tr_key("browse_all"), CTX_BROWSE, ui::UI_ICON_ARROW_RIGHT);
    }
    if entity.is_some() {
        builder = builder.item(i18n::tr_key("remove"), CTX_REMOVE, ui::UI_ICON_REMOVE);
    }
    builder = builder.item(i18n::tr_key("setup_wizard"), CTX_SETUP, ui::UI_ICON_PLAY);
    builder = builder.item(i18n::tr_key("token_title"), CTX_EDIT_TOKEN, ui::UI_ICON_PARAGRAPH);
    builder = builder.item(i18n::tr_key("reset_module"), CTX_RESET, ui::UI_ICON_ALERT);
    builder.push();
}

fn home_ctx_pick(item_id: u32) {
    let entity = SELECTED.borrow().clone();
    let selected_idx = entity
        .as_ref()
        .and_then(|sel| ENTITIES.borrow().iter().position(|e| e.id == sel.id))
        .unwrap_or(0);
    match item_id {
        CTX_BRIGHTNESS => {
            if let Some(e) = entity {
                if e.domain == Domain::Light {
                    light_brightness_start(&e);
                }
            }
        }
        CTX_COLOR => {
            if let Some(e) = entity {
                if e.domain == Domain::Light {
                    light_color_start(&e);
                }
            }
        }
        CTX_COVER_POSITION => {
            if let Some(e) = entity {
                if e.domain == Domain::Cover {
                    cover_position_start(&e);
                }
            }
        }
        CTX_BROWSE => browse_open(),
        CTX_SETUP => wizard_start(),
        CTX_EDIT_TOKEN => wizard_step_token(),
        CTX_REMOVE => {
            if let Some(e) = entity {
                *PENDING_REMOVE.borrow_mut() = Some(selected_idx);
                let prompt = format!("{}\n{}", e.name, i18n::tr_key("remove_confirm"));
                ui::push_confirm(&prompt, ui::UI_ICON_ALERT, ACT_REMOVE_FAV_CONFIRM);
            }
        }
        CTX_RESET => {
            ui::push_confirm(
                i18n::tr_key("reset_confirm"),
                ui::UI_ICON_ALERT,
                ACT_RESET_CONFIRM,
            );
        }
        _ => {}
    }
}


// =================== Browse View ===================

fn browse_open() {
    ui::push_toast(i18n::tr_key("loading"), ui::UI_ICON_INFO, 800);
    match fetch_template_all() {
        Some(mut entities) => {
            entities.sort_by(|a, b| {
                a.name
                    .to_lowercase()
                    .cmp(&b.name.to_lowercase())
                    .then(a.id.cmp(&b.id))
            });
            let mut b = BROWSE.borrow_mut();
            b.all = entities;
            b.filter_domain = FILTER_ALL;
            b.search.clear();
            drop(b);
            browse_rebuild_visible();
            browse_render();
        }
        None => {
            ui::push_toast(i18n::tr_key("ha_unreachable"), ui::UI_ICON_ERROR, 1500);
        }
    }
}

fn browse_rebuild_visible() {
    let mut b = BROWSE.borrow_mut();
    let filter = b.filter_domain;
    let search = b.search.to_lowercase();
    let mut visible: Vec<u32> = Vec::with_capacity(b.all.len());
    for (i, e) in b.all.iter().enumerate() {
        if filter != FILTER_ALL && e.domain as u32 != filter {
            continue;
        }
        if !search.is_empty()
            && !e.name.to_lowercase().contains(&search)
            && !e.id.to_lowercase().contains(&search)
        {
            continue;
        }
        visible.push(i as u32);
    }
    b.visible = visible;
}

fn browse_render() {
    let b = BROWSE.borrow();
    let suffix = match b.filter_domain {
        FILTER_ALL => "all".to_string(),
        d => Domain::from_idx(d as u8).as_str().to_string(),
    };
    let title = format!("{} [{}/{}]", i18n::tr_key("browse"), b.visible.len(), b.all.len());
    let mut builder = ui::ListBuilder::new(&title)
        .on_select(ACT_BROWSE_SELECT)
        .on_menu(ACT_BROWSE_MENU);
    for &idx in b.visible.iter() {
        let e = &b.all[idx as usize];
        let label = e.label();
        let icon = if favs_contains(&e.id) {
            ui::UI_ICON_HEART
        } else {
            e.domain.icon()
        };
        builder = builder.item(&label, idx, icon);
    }
    builder.push();
    let footer = format!("{}  {}", suffix, i18n::tr_key("hint_browse"));
    ui::set_footer(&footer);
}

fn browse_select(idx: u32) {
    let b = BROWSE.borrow();
    let entity = match b.all.get(idx as usize).cloned() {
        Some(e) => e,
        None => return,
    };
    drop(b);
    if favs_contains(&entity.id) {
        ui::push_toast(i18n::tr_key("already_fav"), ui::UI_ICON_INFO, 1000);
        return;
    }
    {
        let mut favs = FAVS.borrow_mut();
        if favs.len() >= MAX_FAVS {
            drop(favs);
            ui::push_toast(i18n::tr_key("max_favs"), ui::UI_ICON_ALERT, 1500);
            return;
        }
        favs.push(Favorite {
            id: entity.id.clone(),
            name: entity.name.clone(),
            domain: entity.domain,
        });
    }
    favs_save();
    ui::pop();
    home_render();
    ui::push_toast(i18n::tr_key("added"), ui::UI_ICON_SUCCESS, 800);
}

fn browse_open_filter_menu() {
    let mut builder =
        ui::ListBuilder::new(i18n::tr_key("filter")).on_select(ACT_BROWSE_FILTER_PICK);
    builder = builder.item(i18n::tr_key("search"), FILTER_SEARCH, ui::UI_ICON_CIRCLE);
    builder = builder.item(i18n::tr_key("all_domains"), FILTER_ALL, ui::UI_ICON_BAR);
    builder = builder.item(i18n::tr_key("lights"), Domain::Light as u32, ui::UI_ICON_LIGHT);
    builder = builder.item(i18n::tr_key("switches"), Domain::Switch as u32, ui::UI_ICON_SWITCH);
    builder = builder.item(i18n::tr_key("scenes"), Domain::Scene as u32, ui::UI_ICON_SCENE);
    builder = builder.item(i18n::tr_key("scripts"), Domain::Script as u32, ui::UI_ICON_MUSIC);
    builder = builder.item(i18n::tr_key("buttons"), Domain::Button as u32, ui::UI_ICON_PLAY);
    builder = builder.item(i18n::tr_key("covers"), Domain::Cover as u32, ui::UI_ICON_COVER);
    builder = builder.item(i18n::tr_key("climates"), Domain::Climate as u32, ui::UI_ICON_SUN);
    builder = builder.item(i18n::tr_key("sensors"), Domain::Sensor as u32, ui::UI_ICON_SENSOR);
    builder.push();
}

fn browse_filter_pick(item_id: u32) {
    ui::pop();
    if item_id == FILTER_SEARCH {
        let initial = BROWSE.borrow().search.clone();
        ui::push_t9_input(
            i18n::tr_key("search"),
            if initial.is_empty() { None } else { Some(&initial) },
            48,
            ACT_BROWSE_SEARCH_DONE,
        );
        return;
    }
    BROWSE.borrow_mut().filter_domain = item_id;
    browse_rebuild_visible();
    browse_render();
}

fn browse_search_done(text: Option<String>) {
    if let Some(t) = text {
        BROWSE.borrow_mut().search = t.trim().to_string();
    }
    browse_rebuild_visible();
    browse_render();
}

// =================== Light Detail ===================

fn light_brightness_start(e: &Entity) {
    *SELECTED.borrow_mut() = Some(e.clone());
    let initial = e.brightness_pct.max(1).min(100) as i32;
    ui::SliderBuilder::new(i18n::tr_key("brightness"))
        .range(0, 100)
        .initial(initial)
        .step(5)
        .unit("%")
        .on_save(ACT_LIGHT_BRIGHT_DONE)
        .push();
}

fn light_brightness_done(value: i32) {
    let entity = match SELECTED.borrow().clone() {
        Some(e) => e,
        None => return,
    };
    let value = value.clamp(0, 100);
    let result = if value == 0 {
        call_service("light", "turn_off", &entity.id, None)
    } else {
        let extra = format!("\"brightness_pct\":{}", value);
        call_service("light", "turn_on", &entity.id, Some(&extra))
    };
    match result {
        Ok(()) => {
            ui::push_toast(i18n::tr_key("saved"), ui::UI_ICON_SUCCESS, 800);
            home_render();
        }
        Err(()) => ui::push_toast(i18n::tr_key("save_failed"), ui::UI_ICON_ERROR, 1500),
    }
}

fn light_color_start(e: &Entity) {
    *SELECTED.borrow_mut() = Some(e.clone());
    ui::push_color_picker(255, 255, 255, ACT_LIGHT_COLOR_PICK);
}

fn light_color_done(packed: u32) {
    let entity = match SELECTED.borrow().clone() {
        Some(e) => e,
        None => return,
    };
    let r = (packed >> 16) & 0xFF;
    let g = (packed >> 8) & 0xFF;
    let b = packed & 0xFF;
    let result = if r == 0 && g == 0 && b == 0 {
        call_service("light", "turn_off", &entity.id, None)
    } else {
        let extra = format!("\"rgb_color\":[{},{},{}]", r, g, b);
        call_service("light", "turn_on", &entity.id, Some(&extra))
    };
    match result {
        Ok(()) => {
            ui::push_toast(i18n::tr_key("saved"), ui::UI_ICON_SUCCESS, 800);
            home_render();
        }
        Err(()) => ui::push_toast(i18n::tr_key("save_failed"), ui::UI_ICON_ERROR, 1500),
    }
}

// =================== Cover Detail ===================

fn cover_position_start(e: &Entity) {
    *SELECTED.borrow_mut() = Some(e.clone());
    let initial = e.position.min(100) as i32;
    ui::SliderBuilder::new(i18n::tr_key("cover_position"))
        .range(0, 100)
        .initial(initial)
        .step(5)
        .unit("%")
        .on_save(ACT_COVER_POS_DONE)
        .push();
}

fn cover_position_done(value: i32) {
    let entity = match SELECTED.borrow().clone() {
        Some(e) => e,
        None => return,
    };
    let value = value.clamp(0, 100);
    let extra = format!("\"position\":{}", value);
    match call_service("cover", "set_cover_position", &entity.id, Some(&extra)) {
        Ok(()) => {
            ui::push_toast(i18n::tr_key("saved"), ui::UI_ICON_SUCCESS, 800);
            home_render();
        }
        Err(()) => ui::push_toast(i18n::tr_key("save_failed"), ui::UI_ICON_ERROR, 1500),
    }
}

// =================== Climate Detail ===================

fn parse_climate_state(entity_id: &str, body: &str) -> Option<ClimateState> {
    let val: serde_json::Value = serde_json::from_str(body).ok()?;
    let state = val
        .get("state")
        .and_then(|v| v.as_str())
        .unwrap_or("off")
        .to_string();
    let attrs = val.get("attributes")?;
    let current = attrs
        .get("current_temperature")
        .and_then(|v| v.as_f64())
        .unwrap_or(20.0);
    let target = attrs
        .get("temperature")
        .and_then(|v| v.as_f64())
        .unwrap_or(current);
    let target_low = attrs.get("target_temp_low").and_then(|v| v.as_f64());
    let target_high = attrs.get("target_temp_high").and_then(|v| v.as_f64());
    let min = attrs
        .get("min_temp")
        .and_then(|v| v.as_f64())
        .unwrap_or(5.0);
    let max = attrs
        .get("max_temp")
        .and_then(|v| v.as_f64())
        .unwrap_or(30.0);
    let step = attrs
        .get("target_temp_step")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.5);
    let unit = attrs
        .get("temperature_unit")
        .and_then(|v| v.as_str())
        .unwrap_or("\u{00b0}C")
        .to_string();
    let modes: Vec<String> = attrs
        .get("hvac_modes")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|x| x.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    let fans: Vec<String> = attrs
        .get("fan_modes")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|x| x.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    let current_fan = attrs
        .get("fan_mode")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let (has_range, spread_tenths) = match (target_low, target_high) {
        (Some(lo), Some(hi)) => (true, ((hi - lo) * 10.0) as i32),
        _ => (false, 0),
    };
    let _ = target;

    let slot_modes = resolve_mode_slots(&modes);

    Some(ClimateState {
        entity_id: entity_id.to_string(),
        current_temp_tenths: (current * 10.0) as i32,
        current_mode: state.clone(),
        pending_mode: state,
        current_fan: current_fan.clone(),
        pending_fan: current_fan,
        fans,
        min_temp_tenths: (min * 10.0) as i32,
        max_temp_tenths: (max * 10.0) as i32,
        step_tenths: ((step * 10.0) as i32).max(1),
        range_spread_tenths: spread_tenths,
        unit,
        slot_modes,
        has_range,
    })
}

fn resolve_mode_slots(modes: &[String]) -> [Option<String>; 3] {
    let mut slots: [Option<String>; 3] = [Some("off".to_string()), None, None];

    if modes.iter().any(|m| m == "heat") {
        slots[1] = Some("heat".to_string());
    } else {
        for fb in ["heat_cool", "auto", "dry", "fan_only"].iter() {
            if modes.iter().any(|m| m.as_str() == *fb) {
                slots[1] = Some((*fb).to_string());
                break;
            }
        }
    }

    if modes.iter().any(|m| m == "cool") {
        slots[2] = Some("cool".to_string());
    } else {
        for fb in ["auto", "heat_cool", "dry", "fan_only"].iter() {
            if modes.iter().any(|m| m.as_str() == *fb)
                && slots[1].as_deref() != Some(*fb)
            {
                slots[2] = Some((*fb).to_string());
                break;
            }
        }
    }
    slots
}

fn climate_open(e: &Entity) {
    let body = match http_get(&format!("/api/states/{}", e.id)) {
        Some(b) => b,
        None => {
            ui::push_toast(i18n::tr_key("ha_unreachable"), ui::UI_ICON_ERROR, 1500);
            return;
        }
    };
    let mut st = match parse_climate_state(&e.id, &body) {
        Some(s) => s,
        None => {
            ui::push_toast(i18n::tr_key("ha_error"), ui::UI_ICON_ERROR, 1500);
            return;
        }
    };

    let target_tenths = if st.has_range {
        ((st.min_temp_tenths + st.max_temp_tenths) / 2).clamp(
            st.min_temp_tenths + st.range_spread_tenths / 2,
            st.max_temp_tenths - st.range_spread_tenths / 2,
        )
    } else {
        st.current_temp_tenths.clamp(st.min_temp_tenths, st.max_temp_tenths)
    };

    canvas::push(&e.name, ACT_CLIMATE_KEY, ACT_CLIMATE_WIDGET);
    canvas::add_slider(
        W_CLIMATE_SLIDER,
        st.min_temp_tenths,
        st.max_temp_tenths,
        target_tenths,
        st.step_tenths,
    );
    canvas::set_focus(W_CLIMATE_SLIDER);
    canvas::set_key_repeat(400, 120);
    canvas::set_footer(i18n::tr_key("climate_footer"));

    if !st.has_range && st.range_spread_tenths == 0 {
        st.range_spread_tenths = 0;
    }
    *CLIMATE.borrow_mut() = Some(st);
    climate_render();
}

fn climate_render() {
    let st_ref = CLIMATE.borrow();
    let st = match st_ref.as_ref() {
        Some(s) => s,
        None => return,
    };
    let (w, h) = canvas::body_size();
    let target = canvas::get_value(W_CLIMATE_SLIDER).unwrap_or(st.current_temp_tenths);

    canvas::clear();
    canvas::set_text_inverted(false);

    canvas::set_text_size(1);
    let cur_label = format_temp(st.current_temp_tenths, &st.unit);
    let cur_hdr = format!("{}  {}", i18n::tr_key("climate_current"), cur_label);
    canvas::draw_text_aligned(0, 2, w as i16, &cur_hdr, canvas::ALIGN_RIGHT);

    canvas::set_text_size(2);
    let target_label = format_temp(target, &st.unit);
    canvas::draw_text_aligned(0, 14, w as i16, &target_label, canvas::ALIGN_CENTER);

    let bar_y = 32;
    let bar_h = 8;
    let bar_x = 20;
    let bar_w = (w as i16) - 40;
    canvas::draw_rect(bar_x, bar_y, bar_w, bar_h, false);
    let range = (st.max_temp_tenths - st.min_temp_tenths).max(1);
    let fill = ((target - st.min_temp_tenths) as i32 * (bar_w as i32 - 4) / range as i32) as i16;
    canvas::draw_rect(bar_x + 2, bar_y + 2, fill.max(0), bar_h - 4, true);

    canvas::set_text_size(1);
    canvas::draw_text(2, bar_y + 1, "[4]");
    canvas::draw_text(w as i16 - 18, bar_y + 1, "[6]");

    let btn_y = 46;
    let btn_h = 14;
    let btn_w = (w as i16 - 8) / 3;
    for i in 0..3 {
        let x = 4 + i as i16 * btn_w;
        let mode = &st.slot_modes[i];
        let active = mode
            .as_deref()
            .map(|m| m == st.pending_mode)
            .unwrap_or(false);
        let key_label = format!("[{}]", i + 1);
        if let Some(m) = mode {
            canvas::draw_rect(x, btn_y, btn_w - 4, btn_h, active);
            canvas::set_text_inverted(active);
            let label = mode_label(m);
            let lbl = format!("{} {}", key_label, label);
            canvas::draw_text_aligned(x + 2, btn_y + 3, btn_w - 8, &lbl, canvas::ALIGN_CENTER);
        } else {
            canvas::draw_rect(x, btn_y, btn_w - 4, btn_h, false);
            canvas::set_text_inverted(false);
            canvas::draw_text_aligned(x + 2, btn_y + 3, btn_w - 8, key_label.as_str(), canvas::ALIGN_CENTER);
        }
    }
    canvas::set_text_inverted(false);

    if !st.fans.is_empty() {
        let fan_y = 64;
        let fan_label = st.pending_fan.as_deref().unwrap_or("-");
        let label = format!("[5] {}: < {} >", i18n::tr_key("climate_fan"), fan_label);
        canvas::draw_text(2, fan_y, &label);
    }

    drop(st_ref);
    let _ = h;
    canvas::commit(false);
}

fn mode_label(mode: &str) -> &str {
    match mode {
        "off" => "Aus",
        "heat" => "Heizen",
        "cool" => "Kuehlen",
        "auto" => "Auto",
        "dry" => "Dry",
        "fan_only" => "Fan",
        "heat_cool" => "Auto",
        _ => mode,
    }
}

fn format_temp(tenths: i32, unit: &str) -> String {
    let int_part = tenths / 10;
    let frac = (tenths.abs() % 10) as u32;
    if tenths < 0 && int_part == 0 {
        format!("-0.{}{}", frac, unit)
    } else {
        format!("{}.{}{}", int_part, frac, unit)
    }
}

fn climate_handle_key(key: char) {
    let mut st_ref = CLIMATE.borrow_mut();
    let st = match st_ref.as_mut() {
        Some(s) => s,
        None => return,
    };
    match key {
        '1' | '2' | '3' => {
            let slot = (key as u8 - b'1') as usize;
            if let Some(mode) = st.slot_modes[slot].clone() {
                st.pending_mode = mode;
            }
        }
        '5' => {
            if st.fans.is_empty() {
                return;
            }
            let cur = st.pending_fan.clone().unwrap_or_default();
            let idx = st.fans.iter().position(|f| f == &cur).unwrap_or(0);
            let next = (idx + 1) % st.fans.len();
            st.pending_fan = Some(st.fans[next].clone());
        }
        'Y' => {
            drop(st_ref);
            climate_commit();
            return;
        }
        'N' => {
            drop(st_ref);
            *CLIMATE.borrow_mut() = None;
            ui::pop();
            home_render();
            return;
        }
        _ => return,
    }
    drop(st_ref);
    climate_render();
}

fn climate_commit() {
    let st = match CLIMATE.borrow().as_ref() {
        Some(s) => ClimateSnapshot::from(s),
        None => return,
    };

    let target_tenths = canvas::get_value(W_CLIMATE_SLIDER).unwrap_or(st.current_temp_tenths);

    let mut any_err = false;

    if st.pending_mode != st.current_mode {
        let extra = format!("\"hvac_mode\":\"{}\"", st.pending_mode);
        if call_service("climate", "set_hvac_mode", &st.entity_id, Some(&extra)).is_err() {
            any_err = true;
        }
    }
    if st.pending_mode != "off" {
        let target_changed = (target_tenths - st.start_target_tenths).abs() >= 1;
        if target_changed {
            let extra = if st.has_range && st.range_spread_tenths > 0 {
                let low = target_tenths - st.range_spread_tenths / 2;
                let high = target_tenths + st.range_spread_tenths / 2;
                format!(
                    "\"target_temp_low\":{}.{},\"target_temp_high\":{}.{}",
                    low / 10,
                    (low.abs() % 10),
                    high / 10,
                    (high.abs() % 10)
                )
            } else {
                format!(
                    "\"temperature\":{}.{}",
                    target_tenths / 10,
                    (target_tenths.abs() % 10)
                )
            };
            if call_service("climate", "set_temperature", &st.entity_id, Some(&extra)).is_err() {
                any_err = true;
            }
        }
    }
    if st.pending_fan != st.current_fan {
        if let Some(fan) = st.pending_fan.as_ref() {
            let extra = format!("\"fan_mode\":\"{}\"", fan);
            if call_service("climate", "set_fan_mode", &st.entity_id, Some(&extra)).is_err() {
                any_err = true;
            }
        }
    }

    if any_err {
        ui::push_toast(i18n::tr_key("save_failed"), ui::UI_ICON_ERROR, 1500);
    } else {
        ui::push_toast(i18n::tr_key("saved"), ui::UI_ICON_SUCCESS, 800);
    }

    *CLIMATE.borrow_mut() = None;
    ui::pop();
    home_render();
}

struct ClimateSnapshot {
    entity_id: String,
    current_temp_tenths: i32,
    current_mode: String,
    pending_mode: String,
    current_fan: Option<String>,
    pending_fan: Option<String>,
    range_spread_tenths: i32,
    has_range: bool,
    start_target_tenths: i32,
}

impl From<&ClimateState> for ClimateSnapshot {
    fn from(s: &ClimateState) -> Self {
        Self {
            entity_id: s.entity_id.clone(),
            current_temp_tenths: s.current_temp_tenths,
            current_mode: s.current_mode.clone(),
            pending_mode: s.pending_mode.clone(),
            current_fan: s.current_fan.clone(),
            pending_fan: s.pending_fan.clone(),
            range_spread_tenths: s.range_spread_tenths,
            has_range: s.has_range,
            start_target_tenths: if s.has_range {
                (s.min_temp_tenths + s.max_temp_tenths) / 2
            } else {
                s.current_temp_tenths
            },
        }
    }
}

// =================== Reset ===================

fn reset_all() {
    let _ = nvs::erase(NVS_HOST);
    let _ = nvs::erase(NVS_PORT);
    let _ = nvs::erase(NVS_HTTPS);
    let _ = nvs::erase(NVS_FAVS);
    let _ = rmem::erase(TOKEN_NAME);
    FAVS.borrow_mut().clear();
    ENTITIES.borrow_mut().clear();
    BROWSE.borrow_mut().all.clear();
    BROWSE.borrow_mut().visible.clear();
    SELECTED.borrow_mut().take();
}

// =================== Plugin entry points ===================

#[no_mangle]
pub extern "C" fn plugin_init() -> i32 {
    log::info(TAG, "init");
    favs_load();
    0
}

#[no_mangle]
pub extern "C" fn plugin_deinit() -> i32 {
    log::info(TAG, "deinit");
    0
}

#[no_mangle]
pub extern "C" fn plugin_on_enter() -> i32 {
    log::info(TAG, "enter");
    favs_load();
    home_render();
    0
}

#[no_mangle]
pub extern "C" fn plugin_on_exit() -> i32 {
    SELECTED.borrow_mut().take();
    PENDING_REMOVE.borrow_mut().take();
    BROWSE.borrow_mut().all.clear();
    BROWSE.borrow_mut().visible.clear();
    0
}

#[no_mangle]
pub extern "C" fn plugin_on_action(action_id: u32, idx: u32, user_data: u32) -> i32 {
    match action_id {
        ACT_HOME_SELECT => {
            if let Some(e) = ENTITIES.borrow().get(idx as usize).cloned() {
                *SELECTED.borrow_mut() = Some(e);
            }
            home_handle_select(idx);
        }
        ACT_HOME_MENU => {
            if let Some(e) = ENTITIES.borrow().get(idx as usize).cloned() {
                *SELECTED.borrow_mut() = Some(e);
            }
            home_open_menu();
        }
        ACT_HOME_CTX_PICK => home_ctx_pick(user_data),

        ACT_BROWSE_SELECT => browse_select(user_data),
        ACT_BROWSE_MENU => browse_open_filter_menu(),
        ACT_BROWSE_FILTER_PICK => browse_filter_pick(user_data),
        ACT_BROWSE_SEARCH_DONE => {
            let text = if user_data == 1 { ui::consume_input_text(48) } else { None };
            browse_search_done(text);
        }

        ACT_SETUP_HOST_DONE => {
            if user_data == 1 {
                if let Some(t) = ui::consume_input_text(96) {
                    let raw = t.trim();
                    if raw.is_empty() {
                        ui::push_toast(i18n::tr_key("host_required"), ui::UI_ICON_ERROR, 1500);
                        home_render();
                        return 0;
                    }
                    let (host, explicit_https, explicit_port) = parse_host_from_url(raw);
                    if host.is_empty() {
                        ui::push_toast(i18n::tr_key("host_required"), ui::UI_ICON_ERROR, 1500);
                        home_render();
                        return 0;
                    }
                    {
                        let mut w = WIZARD.borrow_mut();
                        w.host = host;
                        w.port = explicit_port.unwrap_or_default();
                        w.https = explicit_https;
                    }
                    wizard_finalize_with_probe();
                }
            } else {
                home_render();
            }
        }
        ACT_SETUP_TOKEN_DONE => {
            if user_data == 1 {
                if let Some(t) = ui::consume_input_text(320) {
                    let trimmed = t.trim();
                    if !trimmed.is_empty() {
                        let _ = rmem::write(TOKEN_NAME, trimmed.as_bytes());
                        ui::push_toast(i18n::tr_key("token_ok"), ui::UI_ICON_SUCCESS, 1000);
                    }
                }
            }
            home_render();
        }

        ACT_LIGHT_BRIGHT_DONE => {
            if user_data == 1 {
                if let Some(v) = ui::consume_input_int() {
                    light_brightness_done(v);
                }
            } else {
                home_render();
            }
        }
        ACT_LIGHT_COLOR_PICK => {
            if user_data == 1 {
                if let Some(v) = ui::consume_input_int() {
                    light_color_done(v as u32);
                }
            } else {
                home_render();
            }
        }

        ACT_COVER_POS_DONE => {
            if user_data == 1 {
                if let Some(v) = ui::consume_input_int() {
                    cover_position_done(v);
                }
            } else {
                home_render();
            }
        }

        ACT_REMOVE_FAV_CONFIRM => {
            if idx == 1 {
                if let Some(i) = PENDING_REMOVE.borrow_mut().take() {
                    let mut favs = FAVS.borrow_mut();
                    if i < favs.len() {
                        favs.remove(i);
                    }
                    drop(favs);
                    favs_save();
                    ui::push_toast(i18n::tr_key("removed"), ui::UI_ICON_SUCCESS, 800);
                }
            } else {
                PENDING_REMOVE.borrow_mut().take();
            }
            home_render();
        }
        ACT_CLIMATE_KEY => {
            let key_code = idx as u8;
            let ch = if key_code == 10 {
                'Y'
            } else if key_code == 11 {
                'N'
            } else if (b'0'..=b'9').contains(&key_code) {
                key_code as char
            } else {
                key_code as char
            };
            climate_handle_key(ch);
        }
        ACT_CLIMATE_WIDGET => {
            let event = user_data;
            if event == canvas::WIDGET_COMMITTED {
                climate_commit();
            } else if event == canvas::WIDGET_CANCELLED {
                *CLIMATE.borrow_mut() = None;
                ui::pop();
                home_render();
            } else {
                climate_render();
            }
        }

        ACT_RESET_CONFIRM => {
            if idx == 1 {
                reset_all();
                ui::push_toast(i18n::tr_key("reset_done"), ui::UI_ICON_SUCCESS, 1000);
            }
            home_render();
        }

        _ => {}
    }
    0
}
