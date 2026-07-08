//! \file
//! \brief Thermal cat-printer plugin: prints vCards, free QR codes and vFAT
//!        files on BLE thermal printers (GB01/GB02/GB03/GT01/YT01/MX05/06/08/
//!        10/MXTP), and provides the `thermo_print` external feature so other
//!        plugins can print raw rasters via `use_ext_feature`.

#![cfg_attr(target_arch = "wasm32", no_std)]

extern crate alloc;

mod layout;
mod proto;
mod raster;
mod url;
mod wire;

#[cfg(target_arch = "wasm32")]
mod plugin {

    use crate::{layout, proto, raster, url, wire};
    use alloc::string::{String, ToString};
    use alloc::{format, vec::Vec};
    use cdc_badge_plugin::{
        ble, feature, fs, i18n, image, lifecycle, log, net, nvs, plugin_main, qr,
        surface::Surface,
        time,
        ui::{self, ContextMenuBuilder, ListBuilder, SliderBuilder},
        vcard, wifi,
    };

    plugin_main!();

    const TAG: &str = "thermo_printer";

    // Actions. Menus are multi-level: a top menu opens Print / Printer /
    // Settings submenus (back = view pop, host-managed).
    const ACT_TOP: u32 = 1; // top menu
    const ACT_T9_QR: u32 = 2;
    const ACT_VCARD_LIST: u32 = 3;
    const ACT_MODE_OWN: u32 = 4;
    const ACT_MODE_RECV: u32 = 5;
    const ACT_FILE_LIST: u32 = 6;
    const ACT_SCAN_LIST: u32 = 7;
    const ACT_SETTINGS: u32 = 8;
    const ACT_SLIDER_ENERGY: u32 = 9;
    const ACT_SLIDER_CHUNK: u32 = 10;
    const ACT_SLIDER_FEED: u32 = 11;
    const ACT_OUTPUT_MENU: u32 = 12;
    const ACT_PRINT_MENU: u32 = 13; // Print submenu
    const ACT_PRINTER_MENU: u32 = 14; // Printer submenu
    const ACT_PORT_INPUT: u32 = 15;
    const ACT_SCAN_HIDE: u32 = 16; // scan list covered or left
    const ACT_SCAN_SHOW: u32 = 17; // scan list visible again
    const ACT_JOB: u32 = 20;
    const ACT_DISCOVERED: u32 = 21;
    const ACT_NOTIFY: u32 = 22;
    const ACT_WRITE_DONE: u32 = 23;
    const ACT_NET: u32 = 24; // inbound print-server connection

    // Top menu rows.
    const TOP_PRINT: u32 = 0;
    const TOP_PRINTER: u32 = 1;
    const TOP_SETTINGS: u32 = 2;

    // Print submenu rows.
    const PR_OWN: u32 = 0;
    const PR_RECV: u32 = 1;
    const PR_QR: u32 = 2;
    const PR_FILE: u32 = 3;

    // Printer submenu rows.
    const PT_SCAN: u32 = 0;
    const PT_SERVER: u32 = 1;

    // Settings submenu rows.
    const SET_ENERGY: u32 = 0;
    const SET_CHUNK: u32 = 1;
    const SET_FEED: u32 = 2;
    const SET_PORT: u32 = 3;
    const SET_FONT: u32 = 4;
    const SET_FORGET: u32 = 5;

    // vCard print modes (context-menu item ids).
    const MODE_TEXT: u32 = 0;
    const MODE_QR: u32 = 1;
    const MODE_TEXT_QR: u32 = 2;

    // Output choices for a composed raster.
    const OUT_PRINT: u32 = 0;
    const OUT_SAVE_JPG: u32 = 1;
    const OUT_SEND_JPG: u32 = 2;

    const NVS_PRINTER: &str = "printer_addr";
    const NVS_ENERGY: &str = "energy";
    const NVS_CHUNK: &str = "chunk";
    const NVS_FEED: &str = "feed";
    const NVS_PORT: &str = "port";
    const NVS_FONT: &str = "font"; // 0 = FreeMonoBold 9pt, 1 = compact 6x8
    const NVS_SERVER: &str = "server"; // 1 = print server enabled

    const DEFAULT_PRINT_PORT: u16 = 9100;

    /// vFAT print-file size cap: the file buffer must fit the 256 KiB linear
    /// memory alongside the decoded raster and the framed BLE job.
    const FILE_MAX: usize = 128 * 1024;
    /// Network print-job cap (PJ header + payload), same memory rationale.
    const MAX_JOB: usize = FILE_MAX;
    /// Serial `PLUGIN CMD` line cap (text-only channel).
    const CMD_LINE_MAX: usize = 32 * 1024;
    /// Byte budget for the '\n'-separated vFAT listing from the host.
    const FS_LIST_MAX: usize = 4096;

    const SCAN_MS: u32 = 3000;
    const POLL_INTERVAL_MS: u64 = 200;
    /// Gap between repeated background scans on the "Scan printers" screen.
    const RESCAN_INTERVAL_MS: u64 = 3000;
    const CONNECT_TIMEOUT_MS: u64 = 10_000;
    const DISCOVER_TIMEOUT_MS: u64 = 8_000;
    /// With-response gate every N write-without-response chunks.
    const GATE_EVERY: u32 = 8;
    /// Chunks pushed per tick between gates.
    const BURST_PER_TICK: u32 = 2;
    /// Text page height cap in pixels (surface cap is 64 KiB = 1365 rows).
    const PAGE_MAX_ROWS: usize = 1200;
    const LINE_H: i16 = 20;
    const HEAD_H: i16 = 26;
    const MARGIN: i16 = 4;

    /// A raster ready to print: packed rows, MSB-first, 1 = black.
    struct Ras {
        bits: Vec<u8>,
        stride: usize,
        height: usize,
    }

    /// Print driver state machine, advanced by ticks and BLE actions.
    enum Drive {
        Idle,
        Scanning,
        Connecting {
            since: u64,
        },
        Discovering {
            since: u64,
        },
        Sending {
            data: Vec<u8>,
            off: usize,
            gated: bool,
            since_gate: u32,
        },
    }

    /// What happens when the current print (or page) finishes.
    enum AfterPrint {
        Nothing,
        /// More text pages to render + print.
        NextPage,
        /// Report the ext-feature job result to the caller.
        ReportJob,
    }

    struct State {
        drive: Drive,
        after: AfterPrint,
        /// Raster queued for printing once the printer link is up.
        pending: Option<Ras>,
        /// Remaining wrapped text lines for multi-page printing.
        pending_lines: Vec<layout::VcardLine>,
        /// Composed raster waiting for the user's output choice, plus its JPEG.
        composed: Option<Ras>,
        composed_jpg: Option<Vec<u8>>,
        /// True while handling an ext-feature job (report result when done).
        job_active: bool,
        conn: u32,
        tx_handle: u16,
        rx_handle: u16,
        last_poll_ms: u64,
        last_scan_ms: u64,
        now_ms: u64,
        /// True while the "Scan printers" screen is open: the scan repeats in
        /// the background and the list grows as printers are discovered.
        scan_menu: bool,
        /// Scan results filtered to known models, in list order.
        found: Vec<(String, [u8; 6], u8)>,
        /// Received-vCard index picked from the list, pending mode choice.
        picked_vcard: u16,
        /// vFAT file names in list order.
        files: Vec<String>,
        /// Print server (network listener) enabled.
        server_on: bool,
        /// Port the running listener is actually bound to (0 = not bound).
        /// The NVS port may change while the server runs.
        bound_port: u16,
    }

    impl State {
        const fn defaults() -> Self {
            Self {
                drive: Drive::Idle,
                after: AfterPrint::Nothing,
                pending: None,
                pending_lines: Vec::new(),
                composed: None,
                composed_jpg: None,
                job_active: false,
                conn: 0,
                tx_handle: 0,
                rx_handle: 0,
                last_poll_ms: 0,
                last_scan_ms: 0,
                now_ms: 0,
                scan_menu: false,
                found: Vec::new(),
                picked_vcard: 0,
                files: Vec::new(),
                server_on: false,
                bound_port: 0,
            }
        }
    }

    static mut STATE: State = State::defaults();

    #[inline]
    fn s() -> &'static mut State {
        unsafe { &mut *(&raw mut STATE) }
    }

    // --- Settings ---------------------------------------------------------------

    fn energy() -> u16 {
        nvs::get_u32(NVS_ENERGY)
            .map(|v| v as u16)
            .unwrap_or(proto::DEFAULT_ENERGY)
    }

    fn chunk_size() -> usize {
        let cfg = nvs::get_u32(NVS_CHUNK).unwrap_or(180) as usize;
        let mtu = ble::get_mtu(s().conn) as usize;
        if mtu > 3 {
            cfg.min(mtu - 3)
        } else {
            cfg
        }
    }

    fn feed_steps() -> u16 {
        nvs::get_u32(NVS_FEED)
            .map(|v| v as u16)
            .unwrap_or(proto::DEFAULT_FEED_STEPS)
    }

    /// Print-server TCP port (NVS, default 9100 = JetDirect).
    fn print_port() -> u16 {
        nvs::get_u32(NVS_PORT)
            .map(|v| v as u16)
            .filter(|&p| p != 0)
            .unwrap_or(DEFAULT_PRINT_PORT)
    }

    /// Body text font id: 0 = FreeMonoBold 9pt (default), 1 = compact 6x8.
    fn body_font() -> u8 {
        // NVS stores the setting (0 = normal, 1 = compact); the host font ids
        // are the inverse mapping: 0 = built-in 6x8, 1 = FreeMonoBold 9pt.
        if nvs::get_u32(NVS_FONT).unwrap_or(0) == 1 {
            0 // ui FONT_BUILTIN (6x8 dense)
        } else {
            1 // FreeMonoBold 9pt
        }
    }

    /// Line height in pixels for the chosen body font.
    fn body_line_h() -> i16 {
        if nvs::get_u32(NVS_FONT).unwrap_or(0) == 1 {
            12
        } else {
            LINE_H
        }
    }

    fn saved_printer() -> Option<([u8; 6], u8)> {
        let blob = nvs::get_blob(NVS_PRINTER, 7)?;
        if blob.len() != 7 {
            return None;
        }
        let mut addr = [0u8; 6];
        addr.copy_from_slice(&blob[..6]);
        Some((addr, blob[6]))
    }

    // --- Composition -------------------------------------------------------------

    /// Render a QR for `data` centered on a full-width surface. With
    /// `with_jpg` the JPEG side channel is encoded while the surface exists.
    fn compose_qr_surface(data: &str, with_jpg: bool) -> Option<(Ras, Option<Vec<u8>>)> {
        let modules = qr::measure(data, 20, qr::Ecc::Low)?;
        let scale = proto::qr_scale(modules);
        let bmp = qr::render_bitmap(data, 20, qr::Ecc::Low, scale, 2).ok()?;
        let surf = Surface::create(proto::WIDTH_PX, bmp.side_px + 8).ok()?;
        let x = ((proto::WIDTH_PX - bmp.side_px) / 2) as i16;
        surf.draw_bitmap(x, 4, bmp.side_px as i16, bmp.side_px as i16, &bmp.data)
            .ok()?;
        let jpg = if with_jpg {
            surf.export_jpg(85).ok()
        } else {
            None
        };
        Some((export_ras(&surf)?, jpg))
    }

    /// Render a QR for `data` centered on a full-width surface.
    fn compose_qr(data: &str) -> Option<Ras> {
        compose_qr_surface(data, false).map(|(ras, _)| ras)
    }

    fn export_ras(surf: &Surface) -> Option<Ras> {
        let r = surf.export().ok()?;
        Some(Ras {
            bits: r.data,
            stride: r.stride_bytes as usize,
            height: r.height_px as usize,
        })
    }

    /// Render up to one page of lines onto a surface; leftover lines stay queued.
    fn compose_lines_page(lines: &mut Vec<layout::VcardLine>) -> Option<Ras> {
        if lines.is_empty() {
            return None;
        }
        // Measure with a scratch surface to word-wrap against the real fonts.
        let scratch = Surface::create(proto::WIDTH_PX, 8).ok()?;
        let max_w = (proto::WIDTH_PX as i16 - 2 * MARGIN) as u16;
        let mut rendered: Vec<(bool, String)> = Vec::new();
        let mut y: i16 = MARGIN;
        let mut consumed = 0usize;
        while let Some(line) = lines.get(consumed) {
            let headline = line.headline;
            let (font, h) = if headline {
                (2u8, HEAD_H)
            } else {
                (body_font(), body_line_h())
            };
            scratch.set_font(font).ok()?;
            let wrapped = layout::wrap_text(&line.text, max_w, &|t| {
                scratch.measure_text(t).map(|(w, _)| w).unwrap_or(u16::MAX)
            });
            let needed = (wrapped.len().max(1) as i16) * h;
            // Blank source lines are paragraph separators. Consume them on the
            // current page even at the bottom, otherwise the visual gap moves to
            // the top of the next page and the paragraph break disappears.
            if y + needed > (PAGE_MAX_ROWS as i16) && !wrapped.is_empty() {
                break; // page full; rest goes to the next page
            }
            if wrapped.is_empty() {
                // Blank source line: keep the paragraph gap on the page.
                rendered.push((headline, String::new()));
                y += h;
            } else {
                for w in wrapped {
                    rendered.push((headline, w));
                    y += h;
                }
            }
            consumed += 1;
        }
        lines.drain(..consumed);
        drop(scratch);

        let height = (y + MARGIN).max(8) as u16;
        let surf = Surface::create(proto::WIDTH_PX, height).ok()?;
        let mut cy: i16 = MARGIN;
        for (headline, text) in &rendered {
            let (font, h) = if *headline {
                (2u8, HEAD_H)
            } else {
                (body_font(), body_line_h())
            };
            surf.set_font(font).ok()?;
            // GFX fonts draw from the baseline: offset by the line height.
            surf.draw_text(MARGIN, cy + h - 4, text).ok()?;
            cy += h;
        }
        export_ras(&surf)
    }

    /// Stack two rasters vertically (text above QR for Text+QR mode).
    fn stack(a: Ras, b: Ras) -> Ras {
        let stride = a.stride.max(b.stride);
        let height = a.height + b.height;
        let mut bits = alloc::vec![0u8; stride * height];
        for y in 0..a.height {
            bits[y * stride..y * stride + a.stride]
                .copy_from_slice(&a.bits[y * a.stride..(y + 1) * a.stride]);
        }
        for y in 0..b.height {
            let dy = a.height + y;
            bits[dy * stride..dy * stride + b.stride]
                .copy_from_slice(&b.bits[y * b.stride..(y + 1) * b.stride]);
        }
        Ras {
            bits,
            stride,
            height,
        }
    }

    fn compose_vcard(raw: &str, mode: u32) -> Option<Ras> {
        let text = || -> Option<Ras> {
            let mut lines = layout::vcard_to_lines(raw);
            compose_lines_page(&mut lines)
        };
        match mode {
            MODE_TEXT => text(),
            MODE_QR => compose_qr(raw),
            MODE_TEXT_QR => {
                let t = text()?;
                let q = compose_qr(raw)?;
                Some(stack(t, q))
            }
            _ => None,
        }
    }

    // --- Output routing ----------------------------------------------------------

    /// Offer Print / Save JPG / Send JPG for a composed surface raster.
    fn offer_output(ras: Ras, jpg: Option<Vec<u8>>) {
        s().composed = Some(ras);
        s().composed_jpg = jpg;
        let mut menu = ContextMenuBuilder::new(i18n::tr_key("output"))
            .on_select(ACT_OUTPUT_MENU)
            .item(i18n::tr_key("out_print"), OUT_PRINT, ui::UI_ICON_PLAY);
        if s().composed_jpg.is_some() {
            menu = menu.item(i18n::tr_key("out_save"), OUT_SAVE_JPG, ui::UI_ICON_SUCCESS);
            if s()
                .composed_jpg
                .as_ref()
                .map(|j| j.len() <= 4096)
                .unwrap_or(false)
            {
                menu = menu.item(
                    i18n::tr_key("out_send"),
                    OUT_SEND_JPG,
                    ui::UI_ICON_ARROW_RIGHT,
                );
            }
        }
        menu.push();
    }

    /// True when the print driver has nothing in flight.
    fn drive_idle() -> bool {
        matches!(s().drive, Drive::Idle)
    }

    /// Queue a raster and bring the printer link up. Refused (`false`) while
    /// the driver is not idle, so a second job cannot replace `pending`/`after`
    /// mid-transfer and corrupt the running print; the caller reports the
    /// rejection on its own channel (toast, `ERR\n`, ext-feature status).
    fn start_print(ras: Ras, after: AfterPrint) -> bool {
        if !drive_idle() {
            return false;
        }
        s().pending = Some(ras);
        s().after = after;
        connect_or_scan();
        true
    }

    /// [`start_print`] for menu flows: toasts "Printer busy" on rejection.
    fn start_print_ui(ras: Ras, after: AfterPrint) {
        if !start_print(ras, after) {
            ui::push_message(i18n::tr_key("busy"), ui::UI_ICON_ERROR, 2500);
        }
    }

    // --- Printer link ------------------------------------------------------------

    fn connect_or_scan() {
        if !ble::is_enabled() {
            ui::push_message(i18n::tr_key("ble_off"), ui::UI_ICON_ERROR, 2500);
            finish_job(feature::STATUS_ERROR);
            return;
        }
        if s().conn != 0 && s().tx_handle != 0 {
            begin_send();
            return;
        }
        if let Some((addr, addr_type)) = saved_printer() {
            if ble::connect(addr, addr_type).is_ok() {
                ui::push_toast(i18n::tr_key("connecting"), ui::UI_ICON_SENSOR, 2000);
                s().drive = Drive::Connecting { since: s().now_ms };
                return;
            }
        }
        start_scan(true);
    }

    fn start_scan(auto_pick: bool) {
        if ble::scan_start(SCAN_MS).is_err() {
            ui::push_message(i18n::tr_key("scan_failed"), ui::UI_ICON_ERROR, 2500);
            finish_job(feature::STATUS_ERROR);
            return;
        }
        s().found.clear();
        s().drive = Drive::Scanning;
        // auto_pick drives the strongest match automatically (print flow).
        // Otherwise the "Scan printers" screen is opened: the scan repeats in
        // the background and the result list grows as printers are found.
        if auto_pick {
            s().scan_menu = false;
            ui::push_toast(i18n::tr_key("scanning"), ui::UI_ICON_SENSOR, 2000);
        } else {
            s().scan_menu = true;
            ListBuilder::new(i18n::tr_key("printers"))
                .on_select(ACT_SCAN_LIST)
                .push();
            // Pause the background rescan whenever the list is covered or
            // left (Back fires no select action), resume when visible again.
            ui::set_view_lifecycle(ACT_SCAN_HIDE, ACT_SCAN_SHOW);
            ui::set_list_empty(i18n::tr_key("scanning"));
        }
    }

    /// Rebuild the on-screen printer list from `found` (background rescan).
    fn render_scan_list() {
        let mut lb = ListBuilder::new(i18n::tr_key("printers")).on_select(ACT_SCAN_LIST);
        for (name, addr, _) in &s().found {
            let label = format!("{} {:02X}{:02X}", name, addr[4], addr[5]);
            lb = lb.item(&label, 0, ui::UI_ICON_SENSOR);
        }
        lb.replace();
        // replace() rebuilds the view: re-arm the hide/show hooks on it.
        ui::set_view_lifecycle(ACT_SCAN_HIDE, ACT_SCAN_SHOW);
        if s().found.is_empty() {
            ui::set_list_empty(i18n::tr_key("scanning"));
        }
    }

    fn scan_finished() {
        let results = ble::scan_results(32).unwrap_or_default();

        if s().pending.is_some() {
            // Print flow: pick the strongest known printer automatically.
            match results
                .iter()
                .filter(|r| proto::is_known_model(&r.name))
                .max_by_key(|r| r.rssi)
            {
                Some(best) => {
                    let mut blob = [0u8; 7];
                    blob[..6].copy_from_slice(&best.addr);
                    blob[6] = best.addr_type;
                    let _ = nvs::set_blob(NVS_PRINTER, &blob);
                    if ble::connect(best.addr, best.addr_type).is_ok() {
                        s().drive = Drive::Connecting { since: s().now_ms };
                        return;
                    }
                    ui::push_message(i18n::tr_key("connect_failed"), ui::UI_ICON_ERROR, 2500);
                }
                None => {
                    ui::push_message(i18n::tr_key("no_printer"), ui::UI_ICON_ERROR, 2500);
                }
            }
            s().pending = None;
            s().drive = Drive::Idle;
            finish_job(feature::STATUS_ERROR);
            return;
        }

        // Scan menu flow: merge newly seen printers into the list (keep the
        // ones already found), refresh it, and arm the next background scan.
        for r in results.iter().filter(|r| proto::is_known_model(&r.name)) {
            match s().found.iter_mut().find(|e| e.1 == r.addr) {
                Some(e) => {
                    e.0 = r.name.clone();
                    e.2 = r.addr_type;
                }
                None => s().found.push((r.name.clone(), r.addr, r.addr_type)),
            }
        }
        s().drive = Drive::Idle;
        s().last_scan_ms = s().now_ms;
        if s().scan_menu {
            render_scan_list();
        }
    }

    fn on_connected() {
        s().conn = ble::conn_handle();
        if s().conn == 0 {
            return;
        }
        if ble::discover(s().conn, proto::SERVICE_UUID_LE, ACT_DISCOVERED).is_ok() {
            s().drive = Drive::Discovering { since: s().now_ms };
        } else {
            fail_print("discover");
        }
    }

    fn on_discovered() {
        let chars = ble::consume_discovery(8).unwrap_or_default();
        s().tx_handle = 0;
        s().rx_handle = 0;
        for c in &chars {
            if c.uuid == proto::TX_UUID_LE {
                s().tx_handle = c.value_handle;
            } else if c.uuid == proto::RX_UUID_LE {
                s().rx_handle = c.value_handle;
            }
        }
        if s().tx_handle == 0 {
            fail_print("no tx characteristic");
            return;
        }
        if s().rx_handle != 0 {
            let _ = ble::subscribe_char(s().conn, s().rx_handle, ACT_NOTIFY);
        }
        let _ = ble::on_write_complete(ACT_WRITE_DONE);
        begin_send();
    }

    fn begin_send() {
        let Some(ras) = s().pending.take() else {
            s().drive = Drive::Idle;
            return;
        };
        let data = proto::build_job(
            &ras.bits,
            ras.stride,
            ras.height,
            energy(),
            proto::DEFAULT_QUALITY,
            feed_steps(),
        );
        log::info(
            TAG,
            &format!("printing {} rows, {} bytes", ras.height, data.len()),
        );
        ui::push_toast(i18n::tr_key("printing"), ui::UI_ICON_PLAY, 2000);
        s().drive = Drive::Sending {
            data,
            off: 0,
            gated: false,
            since_gate: 0,
        };
    }

    fn fail_print(why: &str) {
        log::warn(TAG, &format!("print failed: {}", why));
        ui::push_message(i18n::tr_key("print_failed"), ui::UI_ICON_ERROR, 2500);
        s().pending = None;
        s().pending_lines.clear();
        s().drive = Drive::Idle;
        s().after = AfterPrint::Nothing;
        finish_job(feature::STATUS_ERROR);
    }

    /// Report the ext-feature result when a job drove this print.
    fn finish_job(status: i32) {
        if s().job_active {
            s().job_active = false;
            let _ = feature::report_result(status);
        }
    }

    fn on_send_complete() {
        match core::mem::replace(&mut s().after, AfterPrint::Nothing) {
            AfterPrint::NextPage => {
                let mut lines = core::mem::take(&mut s().pending_lines);
                if let Some(ras) = compose_lines_page(&mut lines) {
                    s().pending_lines = lines;
                    s().after = if s().pending_lines.is_empty() {
                        AfterPrint::Nothing
                    } else {
                        AfterPrint::NextPage
                    };
                    s().pending = Some(ras);
                    begin_send();
                    return;
                }
                ui::push_toast(i18n::tr_key("printed"), ui::UI_ICON_SUCCESS, 2000);
            }
            AfterPrint::ReportJob => {
                ui::push_toast(i18n::tr_key("printed"), ui::UI_ICON_SUCCESS, 2000);
                finish_job(feature::STATUS_DONE);
            }
            AfterPrint::Nothing => {
                ui::push_toast(i18n::tr_key("printed"), ui::UI_ICON_SUCCESS, 2000);
            }
        }
        s().drive = Drive::Idle;
    }

    /// Push queued job bytes: BURST_PER_TICK chunks per tick, one gated
    /// with-response write every GATE_EVERY chunks for flow control.
    fn pump_send() {
        let conn = s().conn;
        let chunk = chunk_size().max(20);
        let tx = s().tx_handle;
        let Drive::Sending {
            data,
            off,
            gated,
            since_gate,
        } = &mut s().drive
        else {
            return;
        };
        if *gated {
            return; // waiting for the write-complete action
        }
        for _ in 0..BURST_PER_TICK {
            if *off >= data.len() {
                on_send_complete();
                return;
            }
            let end = (*off + chunk).min(data.len());
            let with_response = *since_gate + 1 >= GATE_EVERY || end == data.len();
            if ble::write_char(conn, tx, &data[*off..end], with_response).is_err() {
                fail_print("write");
                return;
            }
            *off = end;
            if with_response {
                *gated = true;
                *since_gate = 0;
                return;
            }
            *since_gate += 1;
        }
    }

    // --- Menus & flows -----------------------------------------------------------

    fn push_menu() {
        ListBuilder::new(i18n::tr_meta("name"))
            .on_select(ACT_TOP)
            .item(i18n::tr_key("m_print"), TOP_PRINT, ui::UI_ICON_PLAY)
            .item(i18n::tr_key("m_printer"), TOP_PRINTER, ui::UI_ICON_SENSOR)
            .item(i18n::tr_key("m_settings"), TOP_SETTINGS, ui::UI_ICON_INFO)
            .push();
    }

    fn push_print_menu() {
        ListBuilder::new(i18n::tr_key("m_print"))
            .on_select(ACT_PRINT_MENU)
            .item(i18n::tr_key("m_own"), PR_OWN, ui::UI_ICON_INFO)
            .item(i18n::tr_key("m_recv"), PR_RECV, ui::UI_ICON_INFO)
            .item(i18n::tr_key("m_qr"), PR_QR, ui::UI_ICON_PLAY)
            .item(i18n::tr_key("m_file"), PR_FILE, ui::UI_ICON_INFO)
            .push();
    }

    fn printer_menu_builder() -> ListBuilder {
        let server_label = if s().server_on {
            // Show where the running server listens ("no IP" until WiFi is up).
            let ip = wifi::ip().unwrap_or_else(|| i18n::tr_key("no_ip").into());
            format!("{} {}:{}", i18n::tr_key("srv_on"), ip, s().bound_port)
        } else {
            i18n::tr_key("srv_off").into()
        };
        ListBuilder::new(i18n::tr_key("m_printer"))
            .on_select(ACT_PRINTER_MENU)
            .item(i18n::tr_key("m_scan"), PT_SCAN, ui::UI_ICON_SENSOR)
            .item(&server_label, PT_SERVER, ui::UI_ICON_PLAY)
    }

    fn push_printer_menu() {
        printer_menu_builder().push();
    }

    fn mode_menu(action: u32) {
        ContextMenuBuilder::new(i18n::tr_key("mode"))
            .on_select(action)
            .item(i18n::tr_key("mode_text"), MODE_TEXT, ui::UI_ICON_INFO)
            .item(i18n::tr_key("mode_qr"), MODE_QR, ui::UI_ICON_PLAY)
            .item(i18n::tr_key("mode_both"), MODE_TEXT_QR, ui::UI_ICON_INFO)
            .push();
    }

    fn open_recv_list() {
        let count = vcard::received_count();
        let mut lb = ListBuilder::new(i18n::tr_key("m_recv")).on_select(ACT_VCARD_LIST);
        for i in 0..count {
            let label = vcard::received_display(i).unwrap_or_else(|| format!("#{}", i));
            lb = lb.item(&label, i as u32, ui::UI_ICON_INFO);
        }
        lb.push();
        if count == 0 {
            ui::set_list_empty(i18n::tr_key("no_cards"));
        }
    }

    fn open_file_list() {
        let names = fs::list(FS_LIST_MAX).unwrap_or_default();
        // The host cuts the listing silently at the byte cap: a (nearly) full
        // buffer means entries beyond it are missing.
        let listed: usize = names.iter().map(|n| n.len() + 1).sum();
        if listed + 1 >= FS_LIST_MAX {
            log::warn(
                TAG,
                "vFAT listing filled the 4 KiB cap; files may be missing",
            );
        }
        s().files = names
            .into_iter()
            .filter(|f| {
                let lower = f.to_ascii_lowercase();
                lower.ends_with(".png")
                    || lower.ends_with(".jpg")
                    || lower.ends_with(".jpeg")
                    || lower.ends_with(".txt")
                    || lower.ends_with(".md")
            })
            .collect();
        let mut lb = ListBuilder::new(i18n::tr_key("m_file")).on_select(ACT_FILE_LIST);
        for f in &s().files {
            lb = lb.item(f, 0, ui::UI_ICON_INFO);
        }
        lb.push();
        if s().files.is_empty() {
            ui::set_list_empty(i18n::tr_key("no_files"));
        }
    }

    fn print_file(name: &str) {
        if !drive_idle() {
            ui::push_message(i18n::tr_key("busy"), ui::UI_ICON_ERROR, 2500);
            return;
        }
        let lower = name.to_ascii_lowercase();
        if lower.ends_with(".txt") || lower.ends_with(".md") {
            let Some(text) = fs::read_str(name, 32 * 1024) else {
                ui::push_message(i18n::tr_key("read_failed"), ui::UI_ICON_ERROR, 2500);
                return;
            };
            if !print_text(&text) {
                ui::push_message(i18n::tr_key("read_failed"), ui::UI_ICON_ERROR, 2500);
            }
            return;
        }
        // Image file: host decodes, scales to print width and dithers. Check
        // the size first - fs::read pre-allocates max_len, which must stay
        // well below the 256 KiB linear memory.
        let Some(size) = fs::size(name).filter(|&sz| sz <= FILE_MAX) else {
            ui::push_message(i18n::tr_key("read_failed"), ui::UI_ICON_ERROR, 2500);
            return;
        };
        let Some(data) = fs::read(name, size) else {
            ui::push_message(i18n::tr_key("read_failed"), ui::UI_ICON_ERROR, 2500);
            return;
        };
        match image::render(&data, proto::WIDTH_PX) {
            Ok(img) => start_print_ui(
                Ras {
                    bits: img.data,
                    stride: img.stride_bytes as usize,
                    height: img.height_px as usize,
                },
                AfterPrint::Nothing,
            ),
            Err(_) => ui::push_message(i18n::tr_key("img_failed"), ui::UI_ICON_ERROR, 2500),
        }
    }

    fn open_settings() {
        let font_label = if nvs::get_u32(NVS_FONT).unwrap_or(0) == 1 {
            i18n::tr_key("font_compact")
        } else {
            i18n::tr_key("font_normal")
        };
        ContextMenuBuilder::new(i18n::tr_key("m_settings"))
            .on_select(ACT_SETTINGS)
            .item(i18n::tr_key("s_energy"), SET_ENERGY, ui::UI_ICON_INFO)
            .item(i18n::tr_key("s_chunk"), SET_CHUNK, ui::UI_ICON_INFO)
            .item(i18n::tr_key("s_feed"), SET_FEED, ui::UI_ICON_INFO)
            .item(i18n::tr_key("s_port"), SET_PORT, ui::UI_ICON_INFO)
            .item(font_label, SET_FONT, ui::UI_ICON_INFO)
            .item(i18n::tr_key("s_forget"), SET_FORGET, ui::UI_ICON_ERROR)
            .push();
    }

    /// Enable/disable the network print server (opt-in background residency).
    fn toggle_server() {
        if s().server_on {
            s().server_on = false;
            let _ = nvs::set_u32(NVS_SERVER, 0);
            // Port 0: the host closes this plugin's listener regardless of
            // the port (the NVS value may have changed while it ran).
            let _ = net::close(0);
            s().bound_port = 0;
            let _ = lifecycle::set_resident(false);
            ui::push_toast(i18n::tr_key("srv_stopped"), ui::UI_ICON_INFO, 2000);
            return;
        }
        let _ = nvs::set_u32(NVS_SERVER, 1);
        let _ = wifi::request(0);
        let port = print_port();
        match net::listen(port, ACT_NET) {
            Ok(()) => {
                s().server_on = true;
                s().bound_port = port;
                let _ = lifecycle::set_resident(true);
                let ip = wifi::ip().unwrap_or_else(|| i18n::tr_key("no_ip").into());
                ui::push_message(
                    format!("{} {}:{}", i18n::tr_key("srv_started"), ip, port),
                    ui::UI_ICON_SUCCESS,
                    3000,
                );
            }
            Err(_) => {
                ui::push_message(i18n::tr_key("srv_failed"), ui::UI_ICON_ERROR, 2500);
            }
        }
    }

    /// Apply a changed server port: a running listener is rebound so the NVS
    /// value and the live server never drift apart. When rebinding fails the
    /// server goes off (with a toast) instead of pretending to listen.
    fn apply_port(port: u16) {
        if !s().server_on {
            return; // saved only; picked up on the next server start
        }
        let _ = net::close(0);
        match net::listen(port, ACT_NET) {
            Ok(()) => {
                s().bound_port = port;
                let ip = wifi::ip().unwrap_or_else(|| i18n::tr_key("no_ip").into());
                ui::push_message(
                    format!("{} {}:{}", i18n::tr_key("srv_started"), ip, port),
                    ui::UI_ICON_SUCCESS,
                    3000,
                );
            }
            Err(_) => {
                s().server_on = false;
                s().bound_port = 0;
                let _ = lifecycle::set_resident(false);
                ui::push_message(i18n::tr_key("srv_failed"), ui::UI_ICON_ERROR, 2500);
            }
        }
    }

    /// Decode a print-job wire frame (text / image / raster) and start printing.
    fn handle_job_frame(frame: &[u8]) -> bool {
        let Some(job) = wire::parse(frame) else {
            return false;
        };
        match job.kind {
            wire::TYPE_TEXT => print_text(core::str::from_utf8(job.payload).unwrap_or("")),
            wire::TYPE_IMAGE => match image::render(job.payload, proto::WIDTH_PX) {
                // start_print is false while a print runs: the client gets ERR.
                Ok(img) => start_print(
                    Ras {
                        bits: img.data,
                        stride: img.stride_bytes as usize,
                        height: img.height_px as usize,
                    },
                    AfterPrint::Nothing,
                ),
                Err(_) => false,
            },
            wire::TYPE_RASTER => match raster::parse(job.payload) {
                Some(j) => start_print(
                    Ras {
                        bits: j.rows.to_vec(),
                        stride: j.stride_bytes as usize,
                        height: j.height_px as usize,
                    },
                    AfterPrint::Nothing,
                ),
                None => false,
            },
            _ => false,
        }
    }

    /// Compose a QR raster and hand it to the output chooser (with JPEG export).
    fn compose_and_offer_qr(data: &str) {
        match compose_qr_surface(data, true) {
            Some((ras, jpg)) => offer_output(ras, jpg),
            None => ui::push_message(i18n::tr_key("qr_failed"), ui::UI_ICON_ERROR, 2500),
        }
    }

    /// Render + print a text block (word-wrapped, paginated). Shared by the
    /// text wire job, the serial command and file printing.
    fn print_text(text: &str) -> bool {
        let mut lines: Vec<layout::VcardLine> = text
            .lines()
            .map(|l| layout::VcardLine {
                headline: false,
                text: l.to_string(),
            })
            .collect();
        let Some(first) = compose_lines_page(&mut lines) else {
            return false;
        };
        let after = if lines.is_empty() {
            AfterPrint::Nothing
        } else {
            AfterPrint::NextPage
        };
        s().pending_lines = lines;
        start_print(first, after)
    }

    /// Read one PJ frame from an accepted print-server connection and print
    /// it. Bounded on every axis: 1 s per read, 10 s per connection, jobs
    /// capped at [`MAX_JOB`] - a slow or hostile client cannot stall the badge
    /// UI or balloon the receive buffer.
    fn serve_net_job(stream: &cdc_badge_plugin::socket::TcpStream) {
        const READ_TIMEOUT_MS: u32 = 1000;
        const CONN_DEADLINE_MS: u64 = 10_000;
        let deadline = time::uptime_ms() + CONN_DEADLINE_MS;
        let mut buf: Vec<u8> = Vec::new();
        let mut chunk = [0u8; 2048];
        // Full frame size, known once the header has been received.
        let mut want: Option<usize> = None;
        loop {
            if time::uptime_ms() >= deadline {
                break; // frame still incomplete: the parse below answers ERR
            }
            match stream.read(&mut chunk, READ_TIMEOUT_MS) {
                Ok(0) => break, // EOF
                Ok(n) => {
                    buf.extend_from_slice(&chunk[..n]);
                    if want.is_none() && buf.len() >= wire::PJ_HEADER_LEN {
                        // Validate magic and the announced payload length
                        // before buffering anything beyond the header.
                        let magic = u16::from_le_bytes([buf[0], buf[1]]);
                        let len = u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]) as usize;
                        if magic != wire::MAGIC || len > MAX_JOB - wire::PJ_HEADER_LEN {
                            let _ = stream.write(b"ERR\n", 2000);
                            return;
                        }
                        let total = wire::PJ_HEADER_LEN + len;
                        buf.reserve(total - buf.len());
                        want = Some(total);
                    }
                    if want.is_some_and(|w| buf.len() >= w) {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        // "OK" means the job was parsed and queued, not that it printed.
        let ok = handle_job_frame(&buf);
        let _ = stream.write(if ok { b"OK\n" } else { b"ERR\n" }, 2000);
    }

    /// Start the network print server if it was enabled (NVS), on init after
    /// a reboot (the manifest's `autoload` loads the plugin, `set_resident`
    /// keeps it). `server_on` flips only once the listener really runs.
    fn restore_server() {
        if nvs::get_u32(NVS_SERVER).unwrap_or(0) != 1 {
            return;
        }
        let _ = wifi::request(0);
        let port = print_port();
        if net::listen(port, ACT_NET).is_ok() {
            s().server_on = true;
            s().bound_port = port;
            let _ = lifecycle::set_resident(true);
        }
    }

    // --- Lifecycle ----------------------------------------------------------------

    #[no_mangle]
    pub extern "C" fn plugin_init() -> i32 {
        if let Err(e) = feature::register_provider("thermo_print", ACT_JOB) {
            log::warn(TAG, &format!("provider registration failed: {:?}", e));
        }
        // Bring the print server back up after a reboot / autoload if enabled.
        restore_server();
        0
    }

    #[no_mangle]
    pub extern "C" fn plugin_on_cmd(_len: u32) -> i32 {
        // Serial print via `PLUGIN CMD thermo_printer ...`. The CMD channel is
        // line-based (text only); binary jobs go over the network listener or
        // are staged as a vFAT file and printed via `file <name>`.
        let Some(cmd) = cdc_badge_plugin::cmd::consume(CMD_LINE_MAX) else {
            return 0;
        };
        let cmd = cmd.trim();
        if let Some(rest) = cmd.strip_prefix("text ") {
            if !print_text(rest) {
                log::warn(TAG, "serial text print rejected (busy or empty)");
            }
        } else if let Some(name) = cmd.strip_prefix("file ") {
            print_file(name.trim());
        } else if !cmd.is_empty() && !print_text(cmd) {
            log::warn(TAG, "serial text print rejected (busy or empty)");
        }
        0
    }

    #[no_mangle]
    pub extern "C" fn plugin_deinit() -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn plugin_on_enter() -> i32 {
        push_menu();
        0
    }

    #[no_mangle]
    pub extern "C" fn plugin_on_exit() -> i32 {
        s().scan_menu = false;
        if matches!(s().drive, Drive::Sending { .. }) {
            // A transfer is running (e.g. a job the resident print server
            // accepted): leaving the UI must not kill it - let it finish.
            return 0;
        }
        if s().conn != 0 {
            let _ = ble::disconnect(s().conn);
            s().conn = 0;
            s().tx_handle = 0;
            s().rx_handle = 0;
        }
        let _ = ble::on_write_complete(0);
        s().drive = Drive::Idle;
        s().pending = None;
        s().pending_lines.clear();
        s().after = AfterPrint::Nothing;
        finish_job(feature::STATUS_ERROR);
        0
    }

    #[no_mangle]
    pub extern "C" fn plugin_on_tick(uptime_ms: u64) -> i32 {
        s().now_ms = uptime_ms;
        match &s().drive {
            Drive::Idle => {
                // Repeat the scan in the background while the "Scan printers"
                // screen is open, so the list keeps filling in.
                if s().scan_menu
                    && uptime_ms.saturating_sub(s().last_scan_ms) >= RESCAN_INTERVAL_MS
                    && ble::scan_start(SCAN_MS).is_ok()
                {
                    s().drive = Drive::Scanning;
                }
            }
            Drive::Scanning => {
                if uptime_ms.saturating_sub(s().last_poll_ms) >= POLL_INTERVAL_MS {
                    s().last_poll_ms = uptime_ms;
                    if ble::scan_done() {
                        scan_finished();
                    }
                }
            }
            Drive::Connecting { since } => {
                if ble::conn_handle() != 0 {
                    on_connected();
                } else if uptime_ms.saturating_sub(*since) > CONNECT_TIMEOUT_MS {
                    fail_print("connect timeout");
                }
            }
            Drive::Discovering { since } => {
                if uptime_ms.saturating_sub(*since) > DISCOVER_TIMEOUT_MS {
                    fail_print("discover timeout");
                }
            }
            Drive::Sending { .. } => pump_send(),
        }
        0
    }

    #[no_mangle]
    pub extern "C" fn plugin_on_action(action_id: u32, idx: u32, user_data: u32) -> i32 {
        match action_id {
            ACT_TOP => {
                // Leaving the top menu into any branch closes the scan screen.
                s().scan_menu = false;
                match user_data {
                    TOP_PRINT => push_print_menu(),
                    TOP_PRINTER => push_printer_menu(),
                    TOP_SETTINGS => open_settings(),
                    _ => {}
                }
            }
            ACT_PRINT_MENU => match user_data {
                PR_OWN => match vcard::own() {
                    Some(_) => mode_menu(ACT_MODE_OWN),
                    None => ui::push_message(i18n::tr_key("no_own"), ui::UI_ICON_ERROR, 2500),
                },
                PR_RECV => open_recv_list(),
                PR_QR => ui::push_t9_input(i18n::tr_key("qr_input"), None, 128, ACT_T9_QR),
                PR_FILE => open_file_list(),
                _ => {}
            },
            ACT_PRINTER_MENU => {
                s().scan_menu = false;
                match user_data {
                    PT_SCAN => start_scan(false),
                    PT_SERVER => {
                        toggle_server();
                        // Refresh the list in place: server row shows the new
                        // state (and IP:port while running).
                        printer_menu_builder().replace();
                    }
                    _ => {}
                }
            }
            ACT_NET => {
                // Inbound print-server connections: read one framed job each.
                while let Some(stream) = net::accept() {
                    serve_net_job(&stream);
                }
            }
            ACT_T9_QR => {
                if user_data == 1 {
                    if let Some(text) = ui::consume_input_text(128) {
                        let payload = match url::normalize(&text) {
                            url::Normalized::Url(u) => u,
                            url::Normalized::Text(t) => t,
                        };
                        if !payload.is_empty() {
                            compose_and_offer_qr(&payload);
                        }
                    }
                }
            }
            ACT_VCARD_LIST => {
                s().picked_vcard = idx as u16;
                mode_menu(ACT_MODE_RECV);
            }
            ACT_MODE_OWN => {
                if let Some(raw) = vcard::own() {
                    match compose_vcard(&raw, user_data) {
                        Some(ras) => start_print_ui(ras, AfterPrint::Nothing),
                        None => ui::push_message(
                            i18n::tr_key("compose_failed"),
                            ui::UI_ICON_ERROR,
                            2500,
                        ),
                    }
                }
            }
            ACT_MODE_RECV => {
                if let Some(raw) = vcard::received(s().picked_vcard) {
                    match compose_vcard(&raw, user_data) {
                        Some(ras) => start_print_ui(ras, AfterPrint::Nothing),
                        None => ui::push_message(
                            i18n::tr_key("compose_failed"),
                            ui::UI_ICON_ERROR,
                            2500,
                        ),
                    }
                }
            }
            ACT_FILE_LIST => {
                if let Some(name) = s().files.get(idx as usize).cloned() {
                    print_file(&name);
                }
            }
            ACT_SCAN_LIST => {
                s().scan_menu = false; // printer chosen: stop the background scan
                if let Some(&(_, addr, addr_type)) = s().found.get(idx as usize) {
                    let mut blob = [0u8; 7];
                    blob[..6].copy_from_slice(&addr);
                    blob[6] = addr_type;
                    let _ = nvs::set_blob(NVS_PRINTER, &blob);
                    ui::pop(); // done with the scan list: back to the printer menu
                    ui::push_toast(i18n::tr_key("saved_printer"), ui::UI_ICON_SUCCESS, 2000);
                }
            }
            ACT_SCAN_HIDE => {
                // The scan list was covered or left: pause the background
                // rescan so no replace() clobbers whatever is on top now.
                s().scan_menu = false;
            }
            ACT_SCAN_SHOW => {
                // The scan list is visible again: resume rescanning promptly.
                s().scan_menu = true;
                s().last_scan_ms = 0;
            }
            ACT_SETTINGS => match user_data {
                SET_ENERGY => SliderBuilder::new(i18n::tr_key("s_energy"))
                    .range(0x1000, 0xFFFF)
                    .initial(energy() as i32)
                    .step(0x800)
                    .on_save(ACT_SLIDER_ENERGY)
                    .push(),
                SET_CHUNK => SliderBuilder::new(i18n::tr_key("s_chunk"))
                    .range(20, 244)
                    .initial(nvs::get_u32(NVS_CHUNK).unwrap_or(180) as i32)
                    .step(10)
                    .unit("B")
                    .on_save(ACT_SLIDER_CHUNK)
                    .push(),
                SET_FEED => SliderBuilder::new(i18n::tr_key("s_feed"))
                    .range(0, 400)
                    .initial(feed_steps() as i32)
                    .step(16)
                    .on_save(ACT_SLIDER_FEED)
                    .push(),
                SET_PORT => {
                    // Numeric T9 entry; a 1024..65535 slider with step 1 is
                    // unusable on the badge keys.
                    let initial = format!("{}", print_port());
                    ui::push_t9_input(i18n::tr_key("s_port"), Some(&initial), 5, ACT_PORT_INPUT);
                }
                SET_FONT => {
                    // Toggle body font (0 = FreeMonoBold 9pt, 1 = compact 6x8).
                    let next = if nvs::get_u32(NVS_FONT).unwrap_or(0) == 1 {
                        0
                    } else {
                        1
                    };
                    let _ = nvs::set_u32(NVS_FONT, next);
                    ui::push_toast(i18n::tr_key("font_saved"), ui::UI_ICON_SUCCESS, 1500);
                    // Rebuild the menu so the font row shows the new label.
                    ui::pop();
                    open_settings();
                }
                SET_FORGET => {
                    let _ = nvs::erase(NVS_PRINTER);
                    ui::push_toast(i18n::tr_key("forgot_printer"), ui::UI_ICON_SUCCESS, 2000);
                }
                _ => {}
            },
            ACT_PORT_INPUT => {
                if user_data == 1 {
                    if let Some(text) = ui::consume_input_text(8) {
                        match text.trim().parse::<u32>() {
                            Ok(p) if (1024..=65535).contains(&p) => {
                                let _ = nvs::set_u32(NVS_PORT, p);
                                apply_port(p as u16);
                            }
                            _ => {
                                ui::push_message(i18n::tr_key("bad_port"), ui::UI_ICON_ERROR, 2500)
                            }
                        }
                    }
                }
            }
            ACT_SLIDER_ENERGY | ACT_SLIDER_CHUNK | ACT_SLIDER_FEED => {
                if user_data == 1 {
                    if let Some(v) = ui::consume_input_int() {
                        let key = match action_id {
                            ACT_SLIDER_ENERGY => NVS_ENERGY,
                            ACT_SLIDER_CHUNK => NVS_CHUNK,
                            _ => NVS_FEED,
                        };
                        let _ = nvs::set_u32(key, v as u32);
                    }
                }
            }
            ACT_OUTPUT_MENU => {
                let ras = s().composed.take();
                let jpg = s().composed_jpg.take();
                match user_data {
                    OUT_PRINT => {
                        if let Some(ras) = ras {
                            start_print_ui(ras, AfterPrint::Nothing);
                        }
                    }
                    OUT_SAVE_JPG => {
                        if let Some(jpg) = jpg {
                            let name = format!("print_{}.jpg", s().now_ms / 1000);
                            match fs::write(&name, &jpg) {
                                Ok(()) => ui::push_toast(
                                    format!("{} {}", i18n::tr_key("saved"), name),
                                    ui::UI_ICON_SUCCESS,
                                    2500,
                                ),
                                Err(_) => ui::push_message(
                                    i18n::tr_key("save_failed"),
                                    ui::UI_ICON_ERROR,
                                    2500,
                                ),
                            }
                        }
                    }
                    OUT_SEND_JPG => {
                        if let Some(jpg) = jpg {
                            let _ = cdc_badge_plugin::msg::send_interactive("image/jpeg", &jpg);
                        }
                    }
                    _ => {}
                }
            }
            ACT_JOB => {
                // ext-feature job: pull the raster payload and print it.
                if let Some(job) = feature::consume_job(feature::PAYLOAD_MAX) {
                    if !drive_idle() {
                        // A transfer is running: reject instead of corrupting it.
                        log::warn(TAG, "thermo_print job rejected: printer busy");
                        let _ = feature::report_result(feature::STATUS_ERROR);
                        return 0;
                    }
                    match raster::parse(&job.data) {
                        Some(j) => {
                            s().job_active = true;
                            let ras = Ras {
                                bits: j.rows.to_vec(),
                                stride: j.stride_bytes as usize,
                                height: j.height_px as usize,
                            };
                            start_print(ras, AfterPrint::ReportJob);
                        }
                        None => {
                            log::warn(TAG, "malformed thermo_print job");
                            let _ = feature::report_result(feature::STATUS_ERROR);
                        }
                    }
                }
            }
            ACT_DISCOVERED => on_discovered(),
            ACT_NOTIFY => {
                // Printer status notifications: drain the queue, log each frame.
                let mut buf = [0u8; 64];
                while let Ok(Some((_, n))) = ble::consume_notification(&mut buf) {
                    if n == 0 {
                        break;
                    }
                    log::hex(TAG, "printer status", &buf[..n]);
                }
            }
            ACT_WRITE_DONE => {
                // user_data carries the NimBLE status (0 = success).
                if let Drive::Sending { gated, .. } = &mut s().drive {
                    if user_data != 0 {
                        fail_print("write status");
                    } else {
                        *gated = false;
                    }
                }
            }
            _ => {}
        }
        0
    }
} // mod plugin
