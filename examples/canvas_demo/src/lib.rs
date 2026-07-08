//! \file
//! \brief Canvas graphics demo: steps through every canvas drawing capability.
//!
//! Ten pages, advanced with Y and left with N:
//!   1. Shapes  - one of each drawing primitive, labelled.
//!   2. Picture - the primitives composed into a little scene.
//!   3. Bitmaps - hand-drawn 1-bpp bitmaps via `canvas::draw_bitmap`.
//!   4. CP437   - the built-in symbol glyphs drawn as text (icons 0x01-0x1F,
//!      box-drawing, blocks, maths and Greek from the CP437 high half).
//!   5. Shades  - dithered grey fills via `canvas::set_shade` (rect/circle/triangle).
//!   6. Anim    - canvas elements (`canvas::elem_*`): a bouncing ball moved by
//!      delta, a wrapping sprite repositioned absolutely, a blinking label
//!      toggled via show/hide, and a counter that is removed and re-recorded
//!      each step - all driven by the plugin's own tick handler.
//!   7. Tweens  - host-driven animation (`anim::Anim`): the same slide run
//!      with three easing curves side by side, endlessly yoyo-ing, plus a
//!      staggered chained entrance - zero per-frame plugin code.
//!   8. Sprites - frame sheets (`sprite::Sprite`): a looping 4-frame icon
//!      cycler tweened across the screen that flips horizontally at each
//!      edge (completion actions), and a masked ghost floating over a
//!      dithered background (mask = white pixels really paint).
//!   9. Layers  - z-order (`canvas::elem_set_z`): a runner tween passing in
//!      front of one obstacle and behind another, a blinking tag
//!      (`anim::blink`) and a live counter re-recorded in place via
//!      `canvas::elem_clear`.
//!  10. Motion  - the animation extensions: a seamless text marquee
//!      (`canvas::marquee`), one sprite in all four 90-degree orientations,
//!      integer scaling, the SDK spinner preset, white-ink cut-outs and an
//!      endless white wipe bar erasing across the page.
//!
//! The plugin keeps a single canvas view and redraws its body on each page
//! change, so navigating between pages reuses one view instead of stacking.

#![no_std]

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::cell::Cell;
use core::ops::Deref;

use cdc_badge_plugin::{anim, canvas, log, plugin_main, sprite, ui};

plugin_main!();

const TAG: &str = "gfxdemo";

// Action id fired back on canvas key presses (user_data = ASCII key code).
const ACT_KEY: u32 = 9100;
// Completion actions from host-driven animations.
const ACT_WALK_LEG: u32 = 9101;
const K_Y: u32 = b'Y' as u32;
const K_N: u32 = b'N' as u32;

const PAGE_COUNT: u32 = 10;
const PAGE_ANIM: u32 = 5;
const PAGE_LAYERS: u32 = 8;

// Elements on the animation page (ids are plugin-chosen, unrelated to widgets).
const ELEM_BALL: u32 = 1;
const ELEM_SPRITE: u32 = 2;
const ELEM_BLINK: u32 = 3;
const ELEM_COUNTER: u32 = 4;
// Tween page.
const ELEM_EASE_LIN: u32 = 10;
const ELEM_EASE_CUBIC: u32 = 11;
const ELEM_EASE_BOUNCE: u32 = 12;
const ELEM_CHAIN_A: u32 = 13;
const ELEM_CHAIN_B: u32 = 14;
const ELEM_CHAIN_C: u32 = 15;
// Sprite page.
const ELEM_WALKER: u32 = 20;
const ELEM_GHOST: u32 = 21;
// Layers page.
const ELEM_RUNNER: u32 = 30;
const ELEM_TAG: u32 = 31;
const ELEM_LAYER_COUNTER: u32 = 32;
// Motion page.
const ELEM_WIPE: u32 = 40;

// One animation step roughly every other partial refresh worth of time.
const ANIM_STEP_MS: u64 = 400;
const BALL_R: i16 = 6;

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

// Sprite-page state: the walker sprite's handle and current direction.
static WALKER: PluginCell<u32> = PluginCell::new(0);
static WALKER_RIGHT: PluginCell<bool> = PluginCell::new(true);

// Layers-page state: seconds counter re-recorded in place.
static LAYER_SECS: PluginCell<u32> = PluginCell::new(0);
static LAST_LAYER_MS: PluginCell<u64> = PluginCell::new(0);

// Animation-page state: ball offset/velocity, sprite lane offset, step counter
// and the uptime of the last executed step.
static BALL_OX: PluginCell<i16> = PluginCell::new(0);
static BALL_OY: PluginCell<i16> = PluginCell::new(0);
static BALL_VX: PluginCell<i16> = PluginCell::new(11);
static BALL_VY: PluginCell<i16> = PluginCell::new(7);
static SPRITE_OX: PluginCell<i16> = PluginCell::new(0);
static ANIM_STEP: PluginCell<u32> = PluginCell::new(0);
static LAST_STEP_MS: PluginCell<u64> = PluginCell::new(0);

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

// Ghost sprite: data plane leaves the eyes unset (white), the mask covers the
// full silhouette, so the eyes paint white over any background.
const GHOST_DATA: [u16; 16] = [
    0b0000011111100000,
    0b0001111111111000,
    0b0011111111111100,
    0b0011100110011100,
    0b0111100110011110,
    0b0111100110011110,
    0b0111111111111110,
    0b0111111111111110,
    0b0111111111111110,
    0b0111111111111110,
    0b0111111111111110,
    0b0111111111111110,
    0b0111111111111110,
    0b0111111111111110,
    0b0110110110110110,
    0b0100100110010010,
];

const GHOST_MASK: [u16; 16] = [
    0b0000011111100000,
    0b0001111111111000,
    0b0011111111111100,
    0b0011111111111100,
    0b0111111111111110,
    0b0111111111111110,
    0b0111111111111110,
    0b0111111111111110,
    0b0111111111111110,
    0b0111111111111110,
    0b0111111111111110,
    0b0111111111111110,
    0b0111111111111110,
    0b0111111111111110,
    0b0110110110110110,
    0b0100100110010010,
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
    let stride = w.div_ceil(8);
    let mut out = alloc::vec![0u8; stride * h];
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
    canvas::draw_text(0, 0, "Shapes  1/10");
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
    canvas::draw_text(0, 0, "Picture  2/10");
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
    canvas::draw_text(0, 0, "Bitmaps  3/10");
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
    canvas::draw_text(0, 0, "CP437 symbols  4/10");
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
    canvas::draw_text(0, 0, "Shades  5/10");
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

/// Inner playfield of the animation page: the ball bounces inside this frame.
/// Returns (left, top, right, bottom) in body coordinates.
fn anim_field(w: i16, h: i16) -> (i16, i16, i16, i16) {
    (0, 14, w - 1, h - 24)
}

/// Record the animation page: static parts untagged, moving parts as elements.
/// The tick handler afterwards only adjusts element offsets and commits -
/// nothing on this page is ever re-drawn wholesale.
fn page_anim(w: i16, h: i16) {
    canvas::draw_text(0, 0, "Anim  6/10");
    canvas::hline(0, 10, w);

    let (fx0, fy0, fx1, fy1) = anim_field(w, h);
    canvas::draw_rect(fx0, fy0, fx1 - fx0 + 1, fy1 - fy0 + 1, false);

    // Bouncing ball, moved by delta (`elem_move`). Recorded at the field's
    // top-left corner; the offset is the position inside the field.
    canvas::elem_begin(ELEM_BALL).ok();
    canvas::draw_circle(fx0 + 1 + BALL_R, fy0 + 1 + BALL_R, BALL_R, true);
    canvas::elem_end();

    // Sprite lane below the field: a heart plus its label move as one group,
    // repositioned absolutely (`elem_set_offset`) and wrapping at the edge.
    canvas::elem_begin(ELEM_SPRITE).ok();
    canvas::draw_bitmap(0, h - 20, 16, 16, &pack(&HEART));
    canvas::draw_text(18, h - 16, "<3");
    canvas::elem_end();

    // Blinking label, toggled with `elem_show` - stays recorded while hidden.
    canvas::elem_begin(ELEM_BLINK).ok();
    canvas::draw_text_aligned(w - 60, 0, 60, "LIVE", canvas::ALIGN_RIGHT);
    canvas::elem_end();

    // Step counter, removed and re-recorded each step (`elem_remove`).
    canvas::elem_begin(ELEM_COUNTER).ok();
    canvas::draw_text(100, 0, "step 0");
    canvas::elem_end();
}

/// Advance the animation by one step: pure element updates plus one commit.
fn anim_tick() {
    let (w, h) = canvas::body_size();
    let (w, h) = (w as i16, h as i16);
    let (fx0, fy0, fx1, fy1) = anim_field(w, h);

    // Ball: reflect the offset inside the field, applied as a delta so this
    // exercises `elem_move` (the sprite below uses absolute `elem_set_offset`).
    let (old_ox, old_oy) = (BALL_OX.get(), BALL_OY.get());
    let max_ox = (fx1 - fx0 - 1) - 2 * BALL_R;
    let max_oy = (fy1 - fy0 - 1) - 2 * BALL_R;
    let mut ox = old_ox + BALL_VX.get();
    let mut oy = old_oy + BALL_VY.get();
    if ox <= 0 || ox >= max_ox {
        ox = ox.clamp(0, max_ox);
        BALL_VX.set(-BALL_VX.get());
    }
    if oy <= 0 || oy >= max_oy {
        oy = oy.clamp(0, max_oy);
        BALL_VY.set(-BALL_VY.get());
    }
    BALL_OX.set(ox);
    BALL_OY.set(oy);
    canvas::elem_move(ELEM_BALL, ox - old_ox, oy - old_oy).ok();

    // Sprite: advance absolutely and wrap at the right edge.
    let lane_max = (w - 34).max(1);
    let sx = (SPRITE_OX.get() + 16) % lane_max;
    SPRITE_OX.set(sx);
    canvas::elem_set_offset(ELEM_SPRITE, sx, 0).ok();

    // Blink + counter.
    let step = ANIM_STEP.get() + 1;
    ANIM_STEP.set(step);
    canvas::elem_show(ELEM_BLINK, step.is_multiple_of(2)).ok();
    canvas::elem_remove(ELEM_COUNTER).ok();
    canvas::elem_begin(ELEM_COUNTER).ok();
    canvas::draw_text(100, 0, &format!("step {}", step));
    canvas::elem_end();

    canvas::commit(false);
}

/// Host-driven tweens: the identical run with three easing curves, and a
/// staggered chained entrance. After recording, the plugin never touches
/// this page again - the host animates, commits and repeats.
fn page_tweens(w: i16, _h: i16) {
    canvas::draw_text(0, 0, "Tweens  7/10");
    canvas::hline(0, 10, w);

    let run = w - 96;
    let lanes: [(u32, &str, anim::Ease, i16); 3] = [
        (ELEM_EASE_LIN, "linear", anim::Ease::Linear, 18),
        (ELEM_EASE_CUBIC, "cubic", anim::Ease::CubicInOut, 40),
        (ELEM_EASE_BOUNCE, "bounce", anim::Ease::Bounce, 62),
    ];
    for (elem, label, ease, y) in lanes {
        canvas::draw_text(0, y, label);
        canvas::hline(56, y + 10, run + 14);
        canvas::elem_begin(elem).ok();
        canvas::draw_rect(56, y, 14, 10, true);
        canvas::elem_end();
        anim::Anim::element(elem)
            .from(0, 0)
            .to(run, 0)
            .duration_ms(2500)
            .ease(ease)
            .yoyo()
            .repeat(anim::REPEAT_FOREVER)
            .start()
            .ok();
    }

    // Chained entrance: three tags slide in from the left, one after another.
    let mut prev = 0u32;
    for (i, (elem, label)) in [
        (ELEM_CHAIN_A, "one"),
        (ELEM_CHAIN_B, "two"),
        (ELEM_CHAIN_C, "three"),
    ]
    .into_iter()
    .enumerate()
    {
        canvas::elem_begin(elem).ok();
        canvas::draw_rect(8 + (i as i16) * 64, 84, 56, 16, false);
        canvas::draw_text_aligned(8 + (i as i16) * 64, 88, 56, label, canvas::ALIGN_CENTER);
        canvas::elem_end();
        canvas::elem_show(elem, false).ok();
        let mut a = anim::Anim::element(elem)
            .from(-(80 + (i as i16) * 64), 0)
            .to(0, 0)
            .duration_ms(700)
            .ease(anim::Ease::CubicOut)
            .show_on_start();
        if prev != 0 {
            a = a.after(prev);
        }
        prev = a.start().unwrap_or(0);
    }
}

/// Kick off one leg of the walker's journey; called on page entry and from
/// every ACT_WALK_LEG completion. Flips the sprite to face its direction.
fn walker_leg() {
    let (w, _h) = canvas::body_size();
    let sprite = sprite::Sprite::from_handle(WALKER.get());
    let right = !WALKER_RIGHT.get();
    WALKER_RIGHT.set(right);
    sprite
        .set_flags(if right { 0 } else { sprite::FLAG_FLIP_H })
        .ok();
    let target = if right { (w as i16) - 22 } else { 0 };
    anim::Anim::element(ELEM_WALKER)
        .to(target, 0)
        .duration_ms(3000)
        .on_done(ACT_WALK_LEG)
        .start()
        .ok();
}

/// Frame sheets: a looping icon cycler that walks the screen and turns
/// around (completion actions flip it), plus a masked ghost whose white
/// pixels really paint over a dithered background.
fn page_sprites(w: i16, h: i16) {
    canvas::draw_text(0, 0, "Sprites  8/10");
    canvas::hline(0, 10, w);

    // Walker: one sheet built from the four demo icons, cycling as frames.
    let mut sheet = pack(&HEART);
    sheet.extend_from_slice(&pack(&DIAMOND));
    sheet.extend_from_slice(&pack(&ARROW));
    sheet.extend_from_slice(&pack(&CHECK));
    if let Ok(s) = sprite::Sprite::from_sheet(16, 16, 4, &sheet) {
        WALKER.set(s.handle());
        canvas::elem_begin(ELEM_WALKER).ok();
        canvas::draw_sprite(2, 20, s.handle()).ok();
        canvas::elem_end();
        s.play(sprite::Mode::Loop, 250, sprite::REPEAT_FOREVER, 0).ok();
        WALKER_RIGHT.set(false); // walker_leg() flips to "right" first
        walker_leg();
    }
    canvas::draw_text(0, 40, "4-frame loop + tween + flip");

    // Ghost: dithered backdrop, masked sprite floating over it.
    canvas::set_shade(canvas::SHADE_MEDIUM);
    canvas::draw_rect(w - 96, 56, 92, h - 60, true);
    canvas::set_shade(canvas::SHADE_SOLID);
    if let Ok(g) = sprite::Sprite::from_sheet(16, 16, 1, &pack(&GHOST_DATA)) {
        g.set_mask(&pack(&GHOST_MASK)).ok();
        canvas::elem_begin(ELEM_GHOST).ok();
        canvas::draw_sprite(w - 58, 62, g.handle()).ok();
        canvas::elem_end();
        anim::Anim::element(ELEM_GHOST)
            .from(0, 0)
            .to(0, (h - 84).min(18))
            .duration_ms(1400)
            .ease(anim::Ease::QuadInOut)
            .yoyo()
            .repeat(anim::REPEAT_FOREVER)
            .start()
            .ok();
    }
    canvas::draw_text(0, 72, "mask: white eyes");
    canvas::draw_text(0, 84, "over dither");
}

/// Z-order: the runner (layer 3) crosses in front of the layer-2 obstacle
/// and behind the layer-4 one; a tag blinks via the host and a seconds
/// counter is re-recorded in place with `elem_clear`.
fn page_layers(w: i16, _h: i16) {
    canvas::draw_text(0, 0, "Layers  9/10");
    canvas::hline(0, 10, w);

    canvas::elem_begin(100).ok();
    canvas::set_shade(canvas::SHADE_LIGHT);
    canvas::draw_rect(90, 30, 44, 56, true);
    canvas::set_shade(canvas::SHADE_SOLID);
    canvas::draw_text(94, 88, "z=2");
    canvas::elem_end();
    canvas::elem_set_z(100, 2).ok();

    // Solid black so the runner really vanishes behind it (a dithered fill
    // would let it shimmer through the unpainted pixels).
    canvas::elem_begin(101).ok();
    canvas::draw_rect(190, 30, 44, 56, true);
    canvas::set_text_inverted(true);
    canvas::draw_text(202, 52, "z=4");
    canvas::set_text_inverted(false);
    canvas::elem_end();
    canvas::elem_set_z(101, 4).ok();

    // Runner on layer 3: in front of z=2, behind z=4.
    canvas::elem_begin(ELEM_RUNNER).ok();
    canvas::draw_circle(12, 56, 9, true);
    canvas::elem_end();
    canvas::elem_set_z(ELEM_RUNNER, 3).ok();
    anim::Anim::element(ELEM_RUNNER)
        .from(0, 0)
        .to(w - 26, 0)
        .duration_ms(3500)
        .yoyo()
        .repeat(anim::REPEAT_FOREVER)
        .start()
        .ok();

    // Host-driven blink, no tick code involved.
    canvas::elem_begin(ELEM_TAG).ok();
    canvas::draw_text_aligned(w - 70, 0, 70, "z=3 runner", canvas::ALIGN_RIGHT);
    canvas::elem_end();
    anim::blink(ELEM_TAG, 600, anim::REPEAT_FOREVER, 0).ok();

    // Seconds counter, refreshed in place by the tick handler.
    canvas::elem_begin(ELEM_LAYER_COUNTER).ok();
    canvas::draw_text(100, 0, "0s");
    canvas::elem_end();
}

/// Re-record the layers-page counter in place: `elem_clear` keeps the
/// element (and its z) while dropping its draw commands.
fn layer_counter_tick() {
    let secs = LAYER_SECS.get() + 1;
    LAYER_SECS.set(secs);
    canvas::elem_clear(ELEM_LAYER_COUNTER).ok();
    canvas::elem_begin(ELEM_LAYER_COUNTER).ok();
    canvas::draw_text(100, 0, &format!("{}s", secs));
    canvas::elem_end();
    canvas::commit(false);
}

/// The 0.8 animation extensions in one scene: a seamless text marquee, the
/// same sprite in all four 90-degree orientations, integer scaling, the SDK
/// spinner preset, a white-ink cut-out and a white wipe bar erasing its way
/// across the page on an endless yoyo tween.
fn page_motion(w: i16, h: i16) {
    canvas::draw_text(0, 0, "Motion  10/10");
    canvas::hline(0, 10, w);

    canvas::marquee(
        0,
        16,
        w,
        "host-driven marquee +++ text scrolls without a single line of per-frame plugin code +++",
        4,
        120,
    )
    .ok();

    // Rotation: one arrow sheet, four sprites at 0/90/180/270 degrees.
    canvas::draw_text(0, 40, "rot");
    let rots: [u8; 4] = [
        0,
        sprite::FLAG_ROT_90,
        sprite::FLAG_FLIP_H | sprite::FLAG_FLIP_V,
        sprite::FLAG_ROT_90 | sprite::FLAG_FLIP_H | sprite::FLAG_FLIP_V,
    ];
    for (i, &flags) in rots.iter().enumerate() {
        if let Ok(s) = sprite::Sprite::from_sheet(16, 16, 1, &pack(&ARROW)) {
            s.set_flags(flags).ok();
            canvas::draw_sprite(30 + (i as i16) * 24, 34, s.handle()).ok();
        }
    }

    // Integer scaling: the same 16x16 heart at 1x and 2x.
    canvas::draw_text(140, 40, "scale");
    if let Ok(s) = sprite::Sprite::from_sheet(16, 16, 1, &pack(&HEART)) {
        canvas::draw_sprite(186, 34, s.handle()).ok();
    }
    if let Ok(s) = sprite::Sprite::from_sheet(16, 16, 1, &pack(&HEART)) {
        s.set_scale(2).ok();
        canvas::draw_sprite(210, 26, s.handle()).ok();
    }

    // SDK spinner preset, doubled in size, spinning forever.
    canvas::draw_text(0, 78, "spinner");
    if let Ok(s) = sprite::spinner() {
        s.set_scale(2).ok();
        canvas::draw_sprite(60, 62, s.handle()).ok();
        s.play(sprite::Mode::Loop, 150, sprite::REPEAT_FOREVER, 0).ok();
    }

    // White ink: a cut-out circle and label inside a black band.
    canvas::draw_rect(140, 62, 100, 32, true);
    canvas::set_ink_white(true).ok();
    canvas::draw_circle(156, 78, 10, true);
    canvas::set_ink_white(false).ok();
    canvas::set_text_inverted(true);
    canvas::draw_text(174, 74, "white ink");
    canvas::set_text_inverted(false);

    // Wipe bar: a white element on the top layer erases whatever it crosses.
    canvas::elem_begin(ELEM_WIPE).ok();
    canvas::set_ink_white(true).ok();
    canvas::draw_rect(-14, 30, 12, h - 30, true);
    canvas::set_ink_white(false).ok();
    canvas::elem_end();
    canvas::elem_set_z(ELEM_WIPE, 10).ok();
    anim::Anim::element(ELEM_WIPE)
        .from(0, 0)
        .to(w + 14, 0)
        .duration_ms(5000)
        .repeat(anim::REPEAT_FOREVER)
        .start()
        .ok();
}

fn draw(page: u32) {
    let (w, h) = canvas::body_size();
    let (w, h) = (w as i16, h as i16);
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
        4 => page_shades(w),
        5 => {
            BALL_OX.set(0);
            BALL_OY.set(0);
            BALL_VX.set(11);
            BALL_VY.set(7);
            SPRITE_OX.set(0);
            ANIM_STEP.set(0);
            page_anim(w, h);
        }
        6 => page_tweens(w, h),
        7 => page_sprites(w, h),
        8 => {
            LAYER_SECS.set(0);
            LAST_LAYER_MS.set(0);
            page_layers(w, h);
        }
        _ => page_motion(w, h),
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
pub extern "C" fn plugin_on_tick(uptime_ms: u64) -> i32 {
    match PAGE.get() {
        PAGE_ANIM => {
            if uptime_ms.saturating_sub(LAST_STEP_MS.get()) >= ANIM_STEP_MS {
                LAST_STEP_MS.set(uptime_ms);
                anim_tick();
            }
        }
        PAGE_LAYERS => {
            if uptime_ms.saturating_sub(LAST_LAYER_MS.get()) >= 1000 {
                LAST_LAYER_MS.set(uptime_ms);
                layer_counter_tick();
            }
        }
        _ => {}
    }
    0
}

#[no_mangle]
pub extern "C" fn plugin_on_action(action_id: u32, _idx: u32, user_data: u32) -> i32 {
    match action_id {
        ACT_WALK_LEG => {
            walker_leg();
            return 0;
        }
        ACT_KEY => {}
        _ => return 0,
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
