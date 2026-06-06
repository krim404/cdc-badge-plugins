//! \file
//! \brief Scientific calculator example plugin.
//!
//! A tape/ledger calculator drawn on a canvas: the top line is the live
//! input/result, and each completed calculation drops into the history below.
//! Short keys edit (0-9 digits, Y =, N backspace); long keys are quick
//! functions (long 1-4 = + - x /, 5 ^, 6 sqrt, 7 x^2, 8 1/x, 9 %, 0 decimal),
//! long Y opens the categorized function menu, long N clears all. The full
//! scrollable history is reachable from the menu and exported as a .txt file.

#![no_std]

extern crate alloc;

use alloc::format;
use alloc::string::String;
use core::cell::RefCell;

use cdc_badge_plugin::{canvas, fs, i18n, log, nvs, plugin_main, time, ui};

mod engine;
use engine::{AngleMode, Calc, CalcError, Constant, Op, Unary};

plugin_main!();

const TAG: &str = "sci_calc";

// Action ids routed back through plugin_on_action.
const ACT_CANVAS_KEY: u32 = 1; // short key: user_data = ASCII key
const ACT_CANVAS_LONG: u32 = 2; // long key: user_data = ASCII key
const ACT_CAT: u32 = 3; // category list select: user_data = CAT_*
const ACT_LEAF: u32 = 4; // leaf list select: user_data = leaf id

// ASCII key codes delivered by the canvas key/long-press callbacks.
const K_Y: u32 = b'Y' as u32;
const K_N: u32 = b'N' as u32;

// Categories (user_data on ACT_CAT).
const CAT_ARITH: u32 = 1;
const CAT_TRIG: u32 = 2;
const CAT_LOGEXP: u32 = 3;
const CAT_CONST: u32 = 4;
const CAT_MEM: u32 = 5;
const CAT_ANGLE: u32 = 6;
const CAT_HISTORY: u32 = 7;
const CAT_EXPORT: u32 = 8;
const CAT_QUIT: u32 = 9;

// Leaf operations (user_data on ACT_LEAF).
const L_ADD: u32 = 100;
const L_SUB: u32 = 101;
const L_MUL: u32 = 102;
const L_DIV: u32 = 103;
const L_POW: u32 = 104;
const L_MOD: u32 = 105;
const L_PERCENT: u32 = 106;
const L_NEG: u32 = 107;
const L_DOT: u32 = 108;
const L_BKSP: u32 = 109;
const L_CE: u32 = 110;
const L_AC: u32 = 111;

const L_SIN: u32 = 120;
const L_COS: u32 = 121;
const L_TAN: u32 = 122;
const L_ASIN: u32 = 123;
const L_ACOS: u32 = 124;
const L_ATAN: u32 = 125;

const L_LN: u32 = 130;
const L_LOG10: u32 = 131;
const L_EXP: u32 = 132;
const L_EXP10: u32 = 133;
const L_SQR: u32 = 134;
const L_SQRT: u32 = 135;
const L_CBRT: u32 = 136;
const L_RECIP: u32 = 137;
const L_FACT: u32 = 138;

const L_PI: u32 = 140;
const L_E: u32 = 141;

const L_MS: u32 = 150;
const L_MR: u32 = 151;
const L_MPLUS: u32 = 152;
const L_MMINUS: u32 = 153;
const L_MC: u32 = 154;

const NVS_ANGLE: &str = "angle";
const HISTORY_FILE: &str = "history.txt";

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

static CALC: PluginCell<Calc> = PluginCell::new(Calc::new());

fn error_key(e: CalcError) -> &'static str {
    match e {
        CalcError::DivZero => "err_divzero",
        CalcError::Domain => "err_domain",
        CalcError::Overflow => "err_overflow",
    }
}

fn draw_face(full: bool) {
    let calc = CALC.borrow();
    let (w, h) = canvas::body_size();
    let w = w as i16;
    let h = h as i16;
    canvas::clear();

    // Status line: angle mode, memory flag, pending operator.
    let _ = canvas::set_font(canvas::FONT_BUILTIN);
    canvas::set_text_size(1);
    let mut status = String::from(if calc.angle == AngleMode::Deg {
        i18n::tr_key("deg")
    } else {
        i18n::tr_key("rad")
    });
    if calc.mem_set {
        status.push_str("  M");
    }
    let psym = calc.pending_symbol();
    if !psym.is_empty() {
        status.push_str("  ");
        status.push_str(psym);
    }
    canvas::draw_text(0, 0, &status);
    canvas::hline(0, 10, w);

    // Top line: current value, or the error message, right-aligned and shrunk
    // to fit the width. Capped at 12pt so it stays proportionate on the 128px
    // tall panel. Keep it ASCII (the bold fonts are ASCII-only).
    let shown: &str = match calc.error() {
        Some(e) => i18n::tr_key(error_key(e)),
        None => calc.display(),
    };
    let font = canvas::pick_font_that_fits(
        shown,
        w - 8,
        &[
            canvas::FONT_BOLD_12PT,
            canvas::FONT_BOLD_9PT,
            canvas::FONT_BUILTIN,
        ],
    )
    .unwrap_or(canvas::FONT_BUILTIN);
    let _ = canvas::set_font(font);
    // 4px margin each side so the right-most digit is not clipped at the edge.
    canvas::draw_text_aligned(4, 26, w - 8, shown, canvas::ALIGN_RIGHT);

    // History tape: newest first, below the divider, never into the footer.
    let _ = canvas::set_font(canvas::FONT_BUILTIN);
    canvas::set_text_size(1);
    canvas::hline(0, 32, w);
    const TOP: i16 = 36;
    const ROW: i16 = 10;
    let rows = (((h - TOP - 2) / ROW).max(0)) as usize;
    for (i, line) in calc.history.iter().rev().take(rows).enumerate() {
        canvas::draw_text(0, TOP + (i as i16) * ROW, line);
    }

    canvas::commit(full);
}

fn redraw() {
    draw_face(false);
}

fn open_menu() {
    let angle_label = {
        let calc = CALC.borrow();
        format!(
            "{}: {}",
            i18n::tr_key("angle"),
            if calc.angle == AngleMode::Deg {
                i18n::tr_key("deg")
            } else {
                i18n::tr_key("rad")
            }
        )
    };
    ui::ListBuilder::new(i18n::tr_key("menu"))
        .on_select(ACT_CAT)
        .item(i18n::tr_key("cat_arith"), CAT_ARITH, ui::UI_ICON_ANGLE)
        .item(i18n::tr_key("cat_trig"), CAT_TRIG, ui::UI_ICON_ANGLE)
        .item(i18n::tr_key("cat_logexp"), CAT_LOGEXP, ui::UI_ICON_ANGLE)
        .item(i18n::tr_key("cat_const"), CAT_CONST, ui::UI_ICON_ANGLE)
        .item(i18n::tr_key("cat_mem"), CAT_MEM, ui::UI_ICON_ANGLE)
        .item(angle_label, CAT_ANGLE, ui::UI_ICON_SWITCH)
        .item(i18n::tr_key("history"), CAT_HISTORY, ui::UI_ICON_NOTES)
        .item(i18n::tr_key("export"), CAT_EXPORT, ui::UI_ICON_TASK)
        .item(i18n::tr_key("quit"), CAT_QUIT, ui::UI_ICON_BACK)
        .push();
}

fn open_leaf_list(cat: u32) {
    let mut b = ui::ListBuilder::new(i18n::tr_key("menu")).on_select(ACT_LEAF);
    b = match cat {
        CAT_ARITH => b
            .item("+", L_ADD, ui::UI_ICON_NONE)
            .item("-", L_SUB, ui::UI_ICON_NONE)
            .item("x", L_MUL, ui::UI_ICON_NONE)
            .item("/", L_DIV, ui::UI_ICON_NONE)
            .item("x^y", L_POW, ui::UI_ICON_NONE)
            .item("mod", L_MOD, ui::UI_ICON_NONE)
            .item("%", L_PERCENT, ui::UI_ICON_NONE)
            .item(i18n::tr_key("negate"), L_NEG, ui::UI_ICON_NONE)
            .item(i18n::tr_key("dot"), L_DOT, ui::UI_ICON_NONE)
            .item(i18n::tr_key("backspace"), L_BKSP, ui::UI_ICON_NONE)
            .item(i18n::tr_key("clear_entry"), L_CE, ui::UI_ICON_NONE)
            .item(i18n::tr_key("clear_all"), L_AC, ui::UI_ICON_REMOVE),
        CAT_TRIG => b
            .item("sin", L_SIN, ui::UI_ICON_NONE)
            .item("cos", L_COS, ui::UI_ICON_NONE)
            .item("tan", L_TAN, ui::UI_ICON_NONE)
            .item("asin", L_ASIN, ui::UI_ICON_NONE)
            .item("acos", L_ACOS, ui::UI_ICON_NONE)
            .item("atan", L_ATAN, ui::UI_ICON_NONE),
        CAT_LOGEXP => b
            .item("ln", L_LN, ui::UI_ICON_NONE)
            .item("log10", L_LOG10, ui::UI_ICON_NONE)
            .item("e^x", L_EXP, ui::UI_ICON_NONE)
            .item("10^x", L_EXP10, ui::UI_ICON_NONE)
            .item("x^2", L_SQR, ui::UI_ICON_NONE)
            .item("sqrt", L_SQRT, ui::UI_ICON_NONE)
            .item("cbrt", L_CBRT, ui::UI_ICON_NONE)
            .item("1/x", L_RECIP, ui::UI_ICON_NONE)
            .item("x!", L_FACT, ui::UI_ICON_NONE),
        CAT_CONST => b
            .item("pi", L_PI, ui::UI_ICON_NONE)
            .item("e", L_E, ui::UI_ICON_NONE),
        CAT_MEM => b
            .item("MS", L_MS, ui::UI_ICON_NONE)
            .item("MR", L_MR, ui::UI_ICON_NONE)
            .item("M+", L_MPLUS, ui::UI_ICON_NONE)
            .item("M-", L_MMINUS, ui::UI_ICON_NONE)
            .item("MC", L_MC, ui::UI_ICON_NONE),
        _ => b,
    };
    b.push();
}

fn apply_leaf(id: u32) {
    let mut calc = CALC.borrow_mut();
    match id {
        L_ADD => calc.apply_op(Op::Add),
        L_SUB => calc.apply_op(Op::Sub),
        L_MUL => calc.apply_op(Op::Mul),
        L_DIV => calc.apply_op(Op::Div),
        L_POW => calc.apply_op(Op::Pow),
        L_MOD => calc.apply_op(Op::Mod),
        L_PERCENT => calc.apply_op(Op::Percent),
        L_NEG => calc.negate(),
        L_DOT => calc.input_dot(),
        L_BKSP => calc.backspace(),
        L_CE => calc.clear_entry(),
        L_AC => calc.clear_all(),
        L_SIN => calc.apply_unary(Unary::Sin),
        L_COS => calc.apply_unary(Unary::Cos),
        L_TAN => calc.apply_unary(Unary::Tan),
        L_ASIN => calc.apply_unary(Unary::Asin),
        L_ACOS => calc.apply_unary(Unary::Acos),
        L_ATAN => calc.apply_unary(Unary::Atan),
        L_LN => calc.apply_unary(Unary::Ln),
        L_LOG10 => calc.apply_unary(Unary::Log10),
        L_EXP => calc.apply_unary(Unary::Exp),
        L_EXP10 => calc.apply_unary(Unary::Exp10),
        L_SQR => calc.apply_unary(Unary::Sqr),
        L_SQRT => calc.apply_unary(Unary::Sqrt),
        L_CBRT => calc.apply_unary(Unary::Cbrt),
        L_RECIP => calc.apply_unary(Unary::Recip),
        L_FACT => calc.apply_unary(Unary::Fact),
        L_PI => calc.input_constant(Constant::Pi),
        L_E => calc.input_constant(Constant::E),
        L_MS => calc.mem_store(),
        L_MR => calc.mem_recall(),
        L_MPLUS => calc.mem_add(),
        L_MMINUS => calc.mem_sub(),
        L_MC => calc.mem_clear(),
        _ => {}
    }
}

// Long-press quick keys: digits map to operators / functions.
fn apply_quick_key(key: u32) {
    let mut calc = CALC.borrow_mut();
    match key {
        x if x == b'1' as u32 => calc.apply_op(Op::Add),
        x if x == b'2' as u32 => calc.apply_op(Op::Sub),
        x if x == b'3' as u32 => calc.apply_op(Op::Mul),
        x if x == b'4' as u32 => calc.apply_op(Op::Div),
        x if x == b'5' as u32 => calc.apply_op(Op::Pow),
        x if x == b'6' as u32 => calc.apply_unary(Unary::Sqrt),
        x if x == b'7' as u32 => calc.apply_unary(Unary::Sqr),
        x if x == b'8' as u32 => calc.apply_unary(Unary::Recip),
        x if x == b'9' as u32 => calc.apply_op(Op::Percent),
        x if x == b'0' as u32 => calc.input_dot(),
        _ => {}
    }
}

fn build_history_text() -> String {
    let calc = CALC.borrow();
    let mut out = String::new();
    for line in calc.history.iter() {
        out.push_str(line);
        out.push('\n');
    }
    out
}

fn do_export() {
    if CALC.borrow().history.is_empty() {
        ui::push_toast(i18n::tr_key("empty"), ui::UI_ICON_INFO, 1500);
        return;
    }
    let name = match time::local_time() {
        Some(t) => format!("{:02}{:02}-{:02}{:02}.txt", t.day, t.month, t.hour, t.minute),
        None => String::from("calc.txt"),
    };
    let text = build_history_text();
    if fs::write_str(&name, &text).is_ok() {
        ui::push_toast(
            format!("{} {}", i18n::tr_key("saved"), name),
            ui::UI_ICON_SUCCESS,
            2500,
        );
    } else {
        log::error(TAG, "history export failed");
    }
}

fn do_history() {
    if CALC.borrow().history.is_empty() {
        ui::push_toast(i18n::tr_key("empty"), ui::UI_ICON_INFO, 1500);
        return;
    }
    let text = build_history_text();
    if fs::write_str(HISTORY_FILE, &text).is_ok() {
        let _ = fs::view(HISTORY_FILE);
    }
}

#[no_mangle]
pub extern "C" fn plugin_init() -> i32 {
    if let Some(v) = nvs::get_u32(NVS_ANGLE) {
        if v == 1 {
            CALC.borrow_mut().angle = AngleMode::Rad;
        }
    }
    0
}

#[no_mangle]
pub extern "C" fn plugin_deinit() -> i32 {
    0
}

#[no_mangle]
pub extern "C" fn plugin_on_enter() -> i32 {
    canvas::push("", ACT_CANVAS_KEY, 0);
    canvas::set_long_press_action(ACT_CANVAS_LONG);
    canvas::set_footer(i18n::tr_key("hint"));
    draw_face(true);
    0
}

#[no_mangle]
pub extern "C" fn plugin_on_exit() -> i32 {
    0
}

#[no_mangle]
pub extern "C" fn plugin_on_action(action_id: u32, _idx: u32, user_data: u32) -> i32 {
    match action_id {
        ACT_CANVAS_KEY => {
            match user_data {
                d if (b'0' as u32..=b'9' as u32).contains(&d) => {
                    CALC.borrow_mut().input_digit((d - b'0' as u32) as u8);
                }
                K_Y => CALC.borrow_mut().equals(),
                K_N => CALC.borrow_mut().backspace(),
                _ => return 0,
            }
            redraw();
        }
        ACT_CANVAS_LONG => match user_data {
            K_Y => open_menu(),
            K_N => {
                CALC.borrow_mut().clear_all();
                draw_face(true);
            }
            _ => {
                apply_quick_key(user_data);
                redraw();
            }
        },
        ACT_CAT => match user_data {
            CAT_QUIT => {
                ui::pop_to_plugin();
                ui::pop();
            }
            CAT_ANGLE => {
                let rad = {
                    let mut calc = CALC.borrow_mut();
                    calc.toggle_angle();
                    calc.angle == AngleMode::Rad
                };
                let _ = nvs::set_u32(NVS_ANGLE, if rad { 1 } else { 0 });
                ui::pop_to_plugin();
                redraw();
            }
            CAT_HISTORY => do_history(),
            CAT_EXPORT => {
                do_export();
                ui::pop_to_plugin();
                redraw();
            }
            cat => open_leaf_list(cat),
        },
        ACT_LEAF => {
            apply_leaf(user_data);
            ui::pop_to_plugin();
            redraw();
        }
        _ => {}
    }
    0
}
