//! \file
//! \brief Canvas graphics demo: steps through every canvas drawing capability.
//!
//! Five pages, advanced with Y and left with N:
//!   1. Shapes  - one of each drawing primitive, labelled.
//!   2. Picture - the primitives composed into a little scene.
//!   3. Bitmaps - hand-drawn 1-bpp bitmaps via `canvas::draw_bitmap`.
//!   4. CP437   - the built-in symbol glyphs drawn as text (icons 0x01-0x1F,
//!                box-drawing, blocks, maths and Greek from the CP437 high half).
//!   5. Shades  - dithered grey fills via `canvas::set_shade` (rect/circle/triangle).
//!
//! The plugin keeps a single canvas view and redraws its body on each page
//! change, so navigating between pages reuses one view instead of stacking.

#![no_std]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use core::cell::Cell;
use core::ops::Deref;

use cdc_badge_plugin::{canvas, log, plugin_main, ui};

plugin_main!();

const TAG: &str = "gfxdemo";

// Action id fired back on canvas key presses (user_data = ASCII key code).
const ACT_KEY: u32 = 9100;
const K_Y: u32 = b'Y' as u32;
const K_N: u32 = b'N' as u32;

const PAGE_COUNT: u32 = 5;

// Single-threaded WASM plugin: a Sync wrapper around Cell is enough for the
// one-page-index of state. Mirrors the PluginCell pattern from sci_calc.
struct PluginCell<T>(Cell<T>);
unsafe impl<T> Sync for PluginCell<T> {}
impl<T: Copy> PluginCell<T> {
    const fn new(v: T) -> Self {
        Self(Cell::new(v))
    }
}
impl<T> Deref for PluginCell<T> {
    type Target = Cell<T>;
    fn deref(&self) -> &Cell<T> {
        &self.0
    }
}

static PAGE: PluginCell<u32> = PluginCell::new(0);

// --- 1-bpp bitmaps (16x16, MSB-first, bit 15 = leftmost pixel) --------------

const HEART: [u16; 16] = [
    0b0000000000000000,
    0b0001110001110000,
    0b0011111011111000,
    0b0111111111111100,
    0b1111111111111110,
    0b1111111111111110,
    0b1111111111111110,
    0b0111111111111100,
    0b0011111111111000,
    0b0001111111110000,
    0b0000111111100000,
    0b0000011111000000,
    0b0000001110000000,
    0b0000000100000000,
    0b0000000000000000,
    0b0000000000000000,
];

const DIAMOND: [u16; 16] = [
    0b0000000110000000,
    0b0000001111000000,
    0b0000011111100000,
    0b0000111111110000,
    0b0001111111111000,
    0b0011111111111100,
    0b0111111111111110,
    0b1111111111111111,
    0b1111111111111111,
    0b0111111111111110,
    0b0011111111111100,
    0b0001111111111000,
    0b0000111111110000,
    0b0000011111100000,
    0b0000001111000000,
    0b0000000110000000,
];

const ARROW: [u16; 16] = [
    0b0000000000000000,
    0b0000000100000000,
    0b0000000110000000,
    0b0000000111000000,
    0b0000000111100000,
    0b0111111111110000,
    0b0111111111111000,
    0b0111111111111100,
    0b0111111111111100,
    0b0111111111111000,
    0b0111111111110000,
    0b0000000111100000,
    0b0000000111000000,
    0b0000000110000000,
    0b0000000100000000,
    0b0000000000000000,
];

const CHECK: [u16; 16] = [
    0xF0F0, 0xF0F0, 0xF0F0, 0xF0F0,
    0x0F0F, 0x0F0F, 0x0F0F, 0x0F0F,
    0xF0F0, 0xF0F0, 0xF0F0, 0xF0F0,
    0x0F0F, 0x0F0F, 0x0F0F, 0x0F0F,
];

/// Pack 16-wide bitmap rows into the byte layout `draw_bitmap` expects
/// (two bytes per row, most significant bit = leftmost pixel).
fn pack(rows: &[u16]) -> Vec<u8> {
    let mut out = Vec::with_capacity(rows.len() * 2);
    for &r in rows {
        out.push((r >> 8) as u8);
        out.push((r & 0xff) as u8);
    }
    out
}

/// Mask a solid 16x16 shape with a 50% checkerboard so the panel renders it as
/// a mid-gray: dithering is how a 1-bpp display fakes grey tones.
fn gray_fill(rows: &[u16]) -> Vec<u8> {
    let mut masked = [0u16; 16];
    for (i, &r) in rows.iter().enumerate() {
        let m = if i % 2 == 0 { 0xAAAA } else { 0x5555 };
        masked[i] = r & m;
    }
    pack(&masked)
}

/// Build a `w x h` ordered-dither ramp, white on the left fading to black on the
/// right, demonstrating simulated grayscale via a 4x4 Bayer matrix.
fn dither_ramp(w: usize, h: usize) -> Vec<u8> {
    const BAYER: [[u8; 4]; 4] = [
        [0, 8, 2, 10],
        [12, 4, 14, 6],
        [3, 11, 1, 9],
        [15, 7, 13, 5],
    ];
    let stride = (w + 7) / 8;
    let mut out = Vec::new();
    out.resize(stride * h, 0u8);
    for y in 0..h {
        for x in 0..w {
            let gray = ((x * 16) / w) as u8; // 0 (white) .. 15 (black)
            if gray > BAYER[y % 4][x % 4] {
                out[y * stride + x / 8] |= 0x80 >> (x % 8);
            }
        }
    }
    out
}

// --- pages ------------------------------------------------------------------

fn page_shapes(w: i16) {
    canvas::draw_text(0, 0, "Shapes  1/5");
    canvas::hline(0, 10, w);

    // Row 1: pixel cluster, diagonal line, H/V lines, rect outline, rect fill.
    for (px, py) in [(20, 28), (18, 28), (22, 28), (20, 26), (20, 30)] {
        canvas::draw_pixel(px, py);
    }
    canvas::draw_text(6, 44, "pixel");

    canvas::draw_line(64, 40, 92, 18);
    canvas::draw_text(66, 44, "line");

    canvas::hline(124, 30, 30);
    canvas::vline(140, 18, 24);
    canvas::draw_text(126, 44, "h/v");

    canvas::draw_rect(180, 18, 32, 22, false);
    canvas::draw_text(182, 44, "rect");

    canvas::draw_rect(238, 18, 32, 22, true);
    canvas::draw_text(240, 44, "fill");

    // Row 2: circle outline, disc, triangle outline, triangle fill, round rect.
    canvas::draw_circle(24, 72, 12, false);
    canvas::draw_text(6, 88, "circle");

    canvas::draw_circle(80, 72, 12, true);
    canvas::draw_text(66, 88, "disc");

    canvas::draw_triangle(124, 84, 140, 60, 156, 84, false);
    canvas::draw_text(128, 88, "tri");

    canvas::draw_triangle(180, 84, 196, 60, 212, 84, true);
    canvas::draw_text(182, 88, "ftri");

    canvas::draw_round_rect(238, 60, 40, 24, 7, false);
    canvas::draw_text(240, 88, "round");
}

fn page_picture(w: i16) {
    canvas::draw_text(0, 0, "Picture  2/5");
    canvas::hline(0, 10, w);

    // Ground line.
    canvas::hline(0, 100, w);

    // Sun: filled disc with eight rays.
    canvas::draw_circle(44, 30, 13, true);
    canvas::draw_line(44, 12, 44, 4);
    canvas::draw_line(44, 48, 44, 56);
    canvas::draw_line(62, 30, 70, 30);
    canvas::draw_line(26, 30, 18, 30);
    canvas::draw_line(57, 17, 63, 11);
    canvas::draw_line(31, 17, 25, 11);
    canvas::draw_line(57, 43, 63, 49);
    canvas::draw_line(31, 43, 25, 49);

    // House: body, roof, door, window with cross bars.
    canvas::draw_rect(120, 56, 76, 44, false);
    canvas::draw_triangle(114, 56, 158, 22, 202, 56, false);
    canvas::draw_rect(142, 76, 18, 24, true);
    canvas::draw_rect(170, 64, 16, 14, false);
    canvas::hline(170, 71, 16);
    canvas::vline(178, 64, 14);

    // Tree: trunk and round foliage.
    canvas::draw_rect(244, 80, 8, 20, true);
    canvas::draw_circle(248, 66, 16, false);
}

fn page_bitmaps(w: i16) {
    canvas::draw_text(0, 0, "Bitmaps  3/5");
    canvas::hline(0, 10, w);

    let items: [(&[u16], i16, &str); 4] = [
        (&HEART, 24, "heart"),
        (&DIAMOND, 80, "diam"),
        (&ARROW, 136, "arrow"),
        (&CHECK, 192, "check"),
    ];
    for (rows, x, label) in items {
        canvas::draw_bitmap(x, 16, 16, 16, &pack(rows));
        canvas::draw_text(x, 36, label);
    }

    // Simulated grayscale through dithering: a checker-masked diamond and an
    // ordered-dither ramp from white to black.
    canvas::draw_bitmap(24, 52, 16, 16, &gray_fill(&DIAMOND));
    canvas::draw_text(16, 72, "gray");

    canvas::draw_bitmap(70, 52, 200, 16, &dither_ramp(200, 16));
    canvas::draw_text(70, 72, "dither ramp: white -> black");

    canvas::draw_text(0, 92, "1-bpp via canvas::draw_bitmap");
}

fn page_symbols(_w: i16) {
    canvas::draw_text(0, 0, "CP437 symbols  4/5");
    canvas::hline(0, 10, _w);

    // Icon pictographs 0x01-0x1F sent as raw bytes (skip 0x0A/0x0D: the text
    // renderer consumes those as newline / carriage-return).
    let mut icons = String::from("ico ");
    for b in 0x01u8..=0x1f {
        if b == 0x0a || b == 0x0d {
            continue;
        }
        icons.push(b as char);
    }
    canvas::draw_text(0, 16, &icons);

    // CP437 high half reached via the glyphs' normal Unicode characters; the
    // host's UTF-8 -> CP437 converter maps them to the right bytes.
    canvas::draw_text(0, 32, "box \u{250C}\u{252C}\u{2510}\u{251C}\u{253C}\u{2524}\u{2514}\u{2534}\u{2518}\u{2500}\u{2502}");
    canvas::draw_text(0, 48, "blk \u{2591}\u{2592}\u{2593}\u{2588}\u{2584}\u{258C}\u{2590}\u{2580}\u{25A0}");
    canvas::draw_text(0, 64, "mth \u{00B0}\u{00B1}\u{00F7}\u{2248}\u{221A}\u{207F}\u{00B2}\u{00B7}\u{221E}\u{2261}\u{2264}\u{2265}");
    canvas::draw_text(0, 80, "grk \u{03B1}\u{00DF}\u{03C0}\u{03A3}\u{00B5}\u{03A9}\u{0398}\u{03B4}\u{03C6}\u{03B5}");
}

fn page_shades(w: i16) {
    canvas::draw_text(0, 0, "Shades  5/5");
    canvas::hline(0, 10, w);

    // Grey ramp built from filled rects, each at a different shade level.
    let shades = [255u8, 223, 191, 159, 127, 95, 63, 31];
    let bw = w / 8;
    for (i, &s) in shades.iter().enumerate() {
        canvas::set_shade(s);
        canvas::draw_rect((i as i16) * bw, 16, bw - 1, 18, true);
    }
    canvas::set_shade(canvas::SHADE_SOLID);
    canvas::draw_text(0, 38, "rects 255->31 via set_shade");

    // Filled circles at descending shades.
    let cs = [255u8, 191, 127, 63, 24];
    for (i, &s) in cs.iter().enumerate() {
        canvas::set_shade(s);
        canvas::draw_circle(20 + (i as i16) * 56, 64, 11, true);
    }
    canvas::set_shade(canvas::SHADE_SOLID);
    canvas::draw_text(0, 80, "circles");

    // Filled triangles at descending shades.
    let ts = [255u8, 150, 70, 24];
    for (i, &s) in ts.iter().enumerate() {
        let x = 150 + (i as i16) * 34;
        canvas::set_shade(s);
        canvas::draw_triangle(x, 100, x + 14, 84, x + 28, 100, true);
    }
    canvas::set_shade(canvas::SHADE_SOLID);
    canvas::draw_text(150, 80, "triangles");
}

fn draw(page: u32) {
    let (w, _h) = canvas::body_size();
    let w = w as i16;
    canvas::clear();
    canvas::set_font(canvas::FONT_BUILTIN).ok();
    canvas::set_text_size(1);
    canvas::set_text_inverted(false);
    canvas::set_shade(canvas::SHADE_SOLID);
    match page {
        0 => page_shapes(w),
        1 => page_picture(w),
        2 => page_bitmaps(w),
        3 => page_symbols(w),
        _ => page_shades(w),
    }
    canvas::commit(true);
}

// --- lifecycle --------------------------------------------------------------

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
    log::info(TAG, "enter");
    PAGE.set(0);
    canvas::push("", ACT_KEY, 0);
    canvas::set_footer("Y next   N exit");
    draw(0);
    0
}

#[no_mangle]
pub extern "C" fn plugin_on_exit() -> i32 {
    0
}

#[no_mangle]
pub extern "C" fn plugin_on_action(action_id: u32, _idx: u32, user_data: u32) -> i32 {
    if action_id != ACT_KEY {
        return 0;
    }
    match user_data {
        K_Y => {
            let next = (PAGE.get() + 1) % PAGE_COUNT;
            PAGE.set(next);
            draw(next);
        }
        K_N => {
            ui::pop();
        }
        _ => {}
    }
    0
}
