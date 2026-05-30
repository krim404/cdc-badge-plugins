//! \file
//! \brief Continuous BLE central scanner.
//!
//! Polls `ble::scan_*` in a loop driven by `plugin_on_tick`, accumulates every
//! discovered device and keeps a per-device sighting counter. The list is
//! sorted by RSSI (active devices first, stale ones below) and edited in place
//! via the insert / update / remove list primitives so the view updates live
//! without a full re-push.

#![no_std]

extern crate alloc;

use alloc::{format, string::String, vec::Vec};
use cdc_badge_plugin::{
    ble, fs, i18n, log, plugin_main, time,
    ui::{self, ListBuilder},
};

plugin_main!();

const TAG: &str = "ble_scanner";

const SCAN_MS: u32 = 2000;
const POLL_INTERVAL_MS: u64 = 250;
const RESCAN_INTERVAL_MS: u64 = 60_000;
const MAX_RESULTS: usize = 32;
const MAX_DEVICES: usize = 64;
const STALE_AFTER: u8 = 2;
// Row capacity at the built-in 6px font: (296 - 8 scroll - 20 icon/pad - 2) / 6.
const LABEL_W: usize = 42;

const ACT_SELECT: u32 = 1;

/// \brief One discovered peer and how often it has been seen.
#[derive(Clone)]
struct Device {
    addr: [u8; 6],
    addr_type: u8,
    rssi: i8,
    name: String,
    count: u32,
    missed: u8,
}

impl Device {
    fn is_stale(&self) -> bool {
        self.missed >= STALE_AFTER
    }
}

/// \brief One rendered list row, mirroring exactly what is on screen.
#[derive(Clone)]
struct Row {
    addr: [u8; 6],
    label: String,
    icon: u8,
}

/// \brief Aggregated runtime state. A single instance lives in a `static mut`
///        because WAMR serialises every plugin call.
struct State {
    devices: Vec<Device>,
    order: Vec<Row>,
    scanning: bool,
    last_poll_ms: u64,
    last_scan_ms: u64,
    active: bool,
    last_ble_enabled: bool,
    start: Option<time::LocalTime>,
}

impl State {
    const fn defaults() -> Self {
        Self {
            devices: Vec::new(),
            order: Vec::new(),
            scanning: false,
            last_poll_ms: 0,
            last_scan_ms: 0,
            active: false,
            last_ble_enabled: false,
            start: None,
        }
    }
}

static mut STATE: State = State::defaults();

#[inline]
fn s() -> &'static mut State {
    unsafe { &mut *(&raw mut STATE) }
}

// --- Formatting -----------------------------------------------------------

fn mac_short(addr: &[u8; 6]) -> String {
    format!("{:02X}{:02X}{:02X}", addr[3], addr[4], addr[5])
}

fn mac_full(addr: &[u8; 6]) -> String {
    format!(
        "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
        addr[0], addr[1], addr[2], addr[3], addr[4], addr[5]
    )
}

/// \brief Build a row label: name/MAC left, RSSI and `xN` flush right.
fn make_label(d: &Device) -> String {
    let raw = if d.name.is_empty() { mac_short(&d.addr) } else { d.name.clone() };
    let right = format!("{:>4} dBm  x{}", d.rssi, d.count);
    let name_w = LABEL_W.saturating_sub(right.chars().count() + 1);
    let name: String = raw.chars().take(name_w).collect();
    format!("{:<nw$} {}", name, right, nw = name_w)
}

fn icon_for(d: &Device) -> u8 {
    if d.is_stale() {
        ui::UI_ICON_CIRCLE
    } else {
        ui::UI_ICON_INVERSE_CIRCLE
    }
}

// --- Scan handling --------------------------------------------------------

/// \brief Fold one scan round into the device model and bump counters.
fn merge(results: &[ble::ScanResult]) {
    for r in results {
        if let Some(d) = s().devices.iter_mut().find(|d| d.addr == r.addr) {
            d.count = d.count.saturating_add(1);
            d.rssi = r.rssi;
            d.addr_type = r.addr_type;
            if !r.name.is_empty() {
                d.name = r.name.clone();
            }
            d.missed = 0;
        } else {
            if s().devices.len() >= MAX_DEVICES {
                evict_one();
                if s().devices.len() >= MAX_DEVICES {
                    continue;
                }
            }
            s().devices.push(Device {
                addr: r.addr,
                addr_type: r.addr_type,
                rssi: r.rssi,
                name: r.name.clone(),
                count: 1,
                missed: 0,
            });
        }
    }

    for d in s().devices.iter_mut() {
        if !results.iter().any(|r| r.addr == d.addr) {
            d.missed = d.missed.saturating_add(1);
        }
    }
}

/// \brief Drop the most-stale device to make room when the cap is hit.
fn evict_one() {
    let worst = s()
        .devices
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.missed.cmp(&b.1.missed).then(b.1.rssi.cmp(&a.1.rssi)));
    if let Some((idx, dev)) = worst {
        if dev.missed > 0 {
            s().devices.remove(idx);
        }
    }
}

// --- Rendering ------------------------------------------------------------

/// \brief Target rows in stable first-seen order.
///
/// Devices keep the position they were first discovered at, so a live RSSI
/// change only updates a row in place instead of reshuffling the whole list
/// (which previously caused the list to churn every scan round).
fn build_desired() -> Vec<Row> {
    s().devices
        .iter()
        .map(|d| Row { addr: d.addr, label: make_label(d), icon: icon_for(d) })
        .collect()
}

/// \brief Edit the on-screen list to match `desired` using insert/update/remove.
fn render() {
    let desired = build_desired();

    // First fill builds the whole list in one call; later rounds edit in place.
    if s().order.is_empty() {
        if desired.is_empty() {
            return;
        }
        let mut lb = ListBuilder::new(i18n::tr_meta("name")).on_select(ACT_SELECT);
        for d in &desired {
            lb = lb.item(&d.label, 0, d.icon);
        }
        lb.replace();
        s().order = desired;
        return;
    }

    // Remove rows whose device no longer exists (evicted by the cap).
    let mut i = s().order.len();
    while i > 0 {
        i -= 1;
        if !desired.iter().any(|d| d.addr == s().order[i].addr) {
            ui::remove_list_item(i as u16);
            s().order.remove(i);
        }
    }

    for (i, d) in desired.iter().enumerate() {
        match s().order.iter().position(|r| r.addr == d.addr) {
            None => {
                ui::insert_list_item(i as u16, &d.label, 0, d.icon);
                s().order.insert(i, d.clone());
            }
            Some(j) if j != i => {
                ui::remove_list_item(j as u16);
                s().order.remove(j);
                ui::insert_list_item(i as u16, &d.label, 0, d.icon);
                s().order.insert(i, d.clone());
            }
            Some(_) => {
                if s().order[i].label != d.label || s().order[i].icon != d.icon {
                    ui::update_list_item(i as u16, &d.label, 0, d.icon);
                    s().order[i] = d.clone();
                }
            }
        }
    }
}

fn placeholder_for(enabled: bool) -> &'static str {
    if enabled {
        i18n::tr_key("empty")
    } else {
        i18n::tr_key("disabled")
    }
}

fn push_list() {
    ListBuilder::new(i18n::tr_meta("name"))
        .on_select(ACT_SELECT)
        .push();
    s().order.clear();
    let enabled = ble::is_enabled();
    s().last_ble_enabled = enabled;
    ui::set_list_empty(placeholder_for(enabled));
}

// --- Export ---------------------------------------------------------------

/// \brief Format a local time as "DD.MM HH:MM".
fn fmt_ts(t: &time::LocalTime) -> String {
    format!("{:02}.{:02} {:02}:{:02}", t.day, t.month, t.hour, t.minute)
}

/// \brief Write the collected devices to a timestamped text file in the
///        plugin's vFAT folder, headed by the scan start and end times so the
///        duration is visible. Called when the user leaves the scanner.
fn save_results() {
    let end = match time::local_time() {
        Some(t) => t,
        None => return,  // no RTC -> no meaningful filename / timestamps
    };
    if s().devices.is_empty() {
        return;
    }

    let fname = format!("{:02}{:02}-{:02}{:02}.txt", end.day, end.month, end.hour, end.minute);

    let start = s().start.map(|t| fmt_ts(&t)).unwrap_or_else(|| String::from("?"));
    let mut out = String::new();
    out.push_str(&format!("Start: {}\n", start));
    out.push_str(&format!("End:   {}\n", fmt_ts(&end)));
    out.push_str(&format!("Devices: {}\n\n", s().devices.len()));
    for d in &s().devices {
        let name = if d.name.is_empty() { "" } else { d.name.as_str() };
        out.push_str(&format!("{} {:>4}dBm x{} {}\n", mac_full(&d.addr), d.rssi, d.count, name));
    }

    if fs::write_str(&fname, &out).is_ok() {
        ui::push_toast(format!("{} {}", i18n::tr_core("core.saved"), fname),
                       ui::UI_ICON_SUCCESS, 2500);
    }
}

// --- Lifecycle ------------------------------------------------------------

/// \brief Lifecycle hook fired once when the plugin is loaded.
/// \return `0` on success.
#[no_mangle]
pub extern "C" fn plugin_init() -> i32 {
    log::info(TAG, "ble_scanner initialised");
    0
}

/// \brief Lifecycle hook fired once when the plugin is unloaded.
/// \return `0` on success.
#[no_mangle]
pub extern "C" fn plugin_deinit() -> i32 {
    0
}

/// \brief Lifecycle hook fired every time the user opens the plugin.
///
/// Pushes a fresh empty list and arms continuous scanning.
/// \return `0` on success.
#[no_mangle]
pub extern "C" fn plugin_on_enter() -> i32 {
    s().active = true;
    s().scanning = false;
    s().start = time::local_time();
    push_list();
    render();
    if ble::is_enabled() {
        ui::push_toast(i18n::tr_key("empty"), ui::UI_ICON_SENSOR, 2000);
        if ble::scan_start(SCAN_MS).is_ok() {
            s().scanning = true;
            s().last_poll_ms = 0;
        }
    }
    0
}

/// \brief Lifecycle hook fired when the user leaves the plugin view.
///
/// Stops starting new scans so the radio rests while the plugin is not on top.
/// \return `0` on success.
#[no_mangle]
pub extern "C" fn plugin_on_exit() -> i32 {
    save_results();
    s().active = false;
    0
}

/// \brief Background tick driving the scan loop and live list updates.
/// \param uptime_ms Current uptime in milliseconds.
/// \return `0` on success.
#[no_mangle]
pub extern "C" fn plugin_on_tick(uptime_ms: u64) -> i32 {
    let enabled = ble::is_enabled();
    if s().active && enabled != s().last_ble_enabled {
        s().last_ble_enabled = enabled;
        ui::set_list_empty(placeholder_for(enabled));
    }
    if !enabled {
        return 0;
    }

    if s().scanning {
        if uptime_ms.saturating_sub(s().last_poll_ms) < POLL_INTERVAL_MS {
            return 0;
        }
        s().last_poll_ms = uptime_ms;
        if !ble::scan_done() {
            return 0;
        }
        s().scanning = false;
        s().last_scan_ms = uptime_ms;
        if let Ok(results) = ble::scan_results(MAX_RESULTS) {
            merge(&results);
            if s().active {
                render();
            }
        }
    } else if uptime_ms.saturating_sub(s().last_scan_ms) >= RESCAN_INTERVAL_MS {
        if ble::scan_start(SCAN_MS).is_ok() {
            s().scanning = true;
            s().last_poll_ms = uptime_ms;
        }
    }
    0
}

/// \brief Action dispatch: open a detail screen for the picked device.
/// \param action_id  Identifier set when pushing the list.
/// \param idx        Picked row index.
/// \param _user_data Unused.
/// \return `0` on success.
#[no_mangle]
pub extern "C" fn plugin_on_action(action_id: u32, idx: u32, _user_data: u32) -> i32 {
    if action_id != ACT_SELECT {
        return 0;
    }
    let addr = match s().order.get(idx as usize) {
        Some(row) => row.addr,
        None => return 0,
    };
    let d = match s().devices.iter().find(|d| d.addr == addr) {
        Some(d) => d,
        None => return 0,
    };

    let type_str = if d.addr_type == 0 {
        i18n::tr_key("type_pub")
    } else {
        i18n::tr_key("type_rnd")
    };
    let body = format!(
        "{}\n{} dBm  x{}  ({})",
        mac_full(&d.addr),
        d.rssi,
        d.count,
        type_str,
    );
    ui::push_toast(body, ui::UI_ICON_SENSOR, 3000);
    0
}
