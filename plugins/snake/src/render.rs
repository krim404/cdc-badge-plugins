//! \file
//! \brief Canvas/animation layer: elements, sprites and host-driven tweens.
//!
//! Rendering strategy (e-paper friendly, no full redraw per step):
//!   * static background (frame, header line) is recorded untagged, once
//!   * the head is one element whose offset is TWEENED cell-to-cell by the
//!     host clock, so motion stays smooth without per-frame plugin code
//!   * the body is one element re-recorded in place (`elem_clear`) per step
//!   * food/score/effects are elements touched only when they change
//!   * `set_anim_policy(AUTO, ..)` lets the host pace refreshes and clean
//!     up ghosting after animations end

use cdc_badge_plugin::{anim, canvas, sprite, surface};

use crate::game::{Direction, Game};
use crate::sprites;
use crate::{PluginCell, ACT_BOOM_DONE};

// --- element ids (plugin-chosen, max 16 alive at once) -----------------------

// In-game elements.
const ELEM_HEAD: u32 = 1;
const ELEM_BODY: u32 = 2;
const ELEM_FOOD: u32 = 3;
const ELEM_SCORE: u32 = 4;
const ELEM_FX: u32 = 5;
const ELEM_OVERLAY: u32 = 6;
const ELEM_PAUSE: u32 = 7;
// Title-screen elements (the screens never coexist: clear() drops them all).
const ELEM_T_TITLE: u32 = 10;
const ELEM_T_INFO: u32 = 11;
const ELEM_T_PRESS: u32 = 12;
const ELEM_T_APPLE: u32 = 13;
const ELEM_T_HEAD: u32 = 14;

// --- z layers -----------------------------------------------------------------

const Z_FIELD: i8 = 2;
const Z_HEAD: i8 = 3;
const Z_FX: i8 = 5;
const Z_PAUSE: i8 = 8;
const Z_OVERLAY: i8 = 10;

// --- layout -------------------------------------------------------------------

/// Header strip (score line) height inside the canvas body, in pixels.
const HEADER_H: i16 = 12;
/// One-pixel playfield border on each side.
const BORDER: i16 = 1;

/// Pixel geometry of the playfield, derived from the canvas body size.
#[derive(Copy, Clone)]
pub struct Layout {
    pub grid_w: u8,
    pub grid_h: u8,
    field_x: i16,
    field_y: i16,
}

impl Layout {
    /// Fit the largest cell grid into the body below the header, centered.
    pub fn compute(body_w: u16, body_h: u16) -> Layout {
        let avail_w = body_w as i16 - 2 * BORDER;
        let avail_h = body_h as i16 - HEADER_H - 2 * BORDER;
        let grid_w = (avail_w / sprites::CELL).clamp(0, u8::MAX as i16) as u8;
        let grid_h = (avail_h / sprites::CELL).clamp(0, u8::MAX as i16) as u8;
        let used_w = grid_w as i16 * sprites::CELL;
        let used_h = grid_h as i16 * sprites::CELL;
        Layout {
            grid_w,
            grid_h,
            field_x: (body_w as i16 - used_w) / 2,
            field_y: HEADER_H + BORDER + (avail_h - used_h) / 2,
        }
    }

    /// Top-left pixel of a grid cell.
    fn cell_px(&self, cell: (u8, u8)) -> (i16, i16) {
        (
            self.field_x + cell.0 as i16 * sprites::CELL,
            self.field_y + cell.1 as i16 * sprites::CELL,
        )
    }

    /// A cell's pixel position as an offset from the field origin - the
    /// coordinate space the head/food elements are tweened/offset in.
    fn cell_offset(&self, cell: (u8, u8)) -> (i16, i16) {
        (
            cell.0 as i16 * sprites::CELL,
            cell.1 as i16 * sprites::CELL,
        )
    }
}

// --- sprite handles -------------------------------------------------------------

// Created once on plugin enter and kept alive across screen rebuilds via
// `canvas::clear_ex(CLEAR_KEEP_SPRITES)` - a plain `clear()` would wipe the
// sprite store with the display list. Playback stops on every clear, so each
// screen restarts its `play()` loops. 0 = not available.
static HEAD_SPRITE: PluginCell<u32> = PluginCell::new(0);
static BODY_SPRITE: PluginCell<u32> = PluginCell::new(0);
static FOOD_SPRITE: PluginCell<u32> = PluginCell::new(0);
static BOOM_SPRITE: PluginCell<u32> = PluginCell::new(0);

/// Create the sprite sheets once (plugin enter).
pub fn create_sprites() {
    if let Ok(s) = sprite::Sprite::from_sheet(8, 8, sprites::HEAD_FRAME_COUNT, &sprites::HEAD_SHEET)
    {
        // Opaque: the eye's cleared pixels must paint white over anything.
        s.set_flags(sprite::FLAG_OPAQUE).ok();
        s.set_frame_durations(&sprites::HEAD_FRAME_MS).ok();
        HEAD_SPRITE.set(s.handle());
    } else {
        HEAD_SPRITE.set(0);
    }
    match sprite::Sprite::from_sheet(8, 8, sprites::BODY_FRAME_COUNT, &sprites::BODY_SHEET) {
        Ok(s) => BODY_SPRITE.set(s.handle()),
        Err(_) => BODY_SPRITE.set(0),
    }
    match sprite::Sprite::from_sheet(8, 8, sprites::FOOD_FRAME_COUNT, &sprites::FOOD_SHEET) {
        Ok(s) => FOOD_SPRITE.set(s.handle()),
        Err(_) => FOOD_SPRITE.set(0),
    }
    match sprite::Sprite::from_sheet(16, 16, sprites::BOOM_FRAME_COUNT, &sprites::BOOM_SHEET) {
        Ok(s) => BOOM_SPRITE.set(s.handle()),
        Err(_) => BOOM_SPRITE.set(0),
    }
}

/// Baseline offset of a GFX font: these fonts draw from the BASELINE, so
/// every `draw_text*` call needs `top + ascent` (same trick as name_badge:
/// "A" has no descender, its bbox height is exactly the cap-height ascent).
fn font_ascent(font: u8) -> i16 {
    let Ok(s) = surface::Surface::create(8, 8) else {
        return FALLBACK_ASCENT;
    };
    if s.set_font(font).is_err() {
        return FALLBACK_ASCENT;
    }
    match s.measure_text("A") {
        Ok((_, h)) => h as i16,
        Err(_) => FALLBACK_ASCENT,
    }
}

/// Used when the ascent cannot be measured (surface unavailable).
const FALLBACK_ASCENT: i16 = 24;

/// Reset the draw state that recorded commands capture (font, ink, shade).
fn reset_draw_state() {
    canvas::set_font(canvas::FONT_BUILTIN).ok();
    canvas::set_text_size(1);
    canvas::set_text_inverted(false);
    canvas::set_shade(canvas::SHADE_SOLID);
    canvas::set_ink_white(false).ok();
}

// --- title screen -----------------------------------------------------------------

/// Full title screen: animated logo, decorative sprites, a static controls
/// line, speed + high score info and a blinking start hint. One full refresh
/// at the end.
pub fn show_title(body_w: u16, body_h: u16, level: u8, highscore: u32) {
    let (w, h) = (body_w as i16, body_h as i16);
    canvas::clear_ex(canvas::CLEAR_KEEP_SPRITES).ok();
    reset_draw_state();

    // Vertical layout, all relative to the measured logo height: the GFX
    // fonts are baseline-anchored, so the logo draws at top + ascent. The
    // wobble tween moves it 2 px up, hence the 4 px top margin.
    let logo_top: i16 = 4;
    let ascent = font_ascent(canvas::FONT_BOLD_24PT);
    let info_y = logo_top + ascent + 10;
    TITLE_INFO_Y.set(info_y);

    // Wobbling logo: the whole word drifts 2 px up/down forever.
    canvas::elem_begin(ELEM_T_TITLE).ok();
    canvas::set_font(canvas::FONT_BOLD_24PT).ok();
    canvas::draw_text_aligned(0, logo_top + ascent, w, "SNAKE", canvas::ALIGN_CENTER);
    canvas::elem_end();
    canvas::set_font(canvas::FONT_BUILTIN).ok();
    anim::Anim::element(ELEM_T_TITLE)
        .from(0, -2)
        .to(0, 2)
        .duration_ms(1600)
        .ease(anim::Ease::QuadInOut)
        .yoyo()
        .repeat(anim::REPEAT_FOREVER)
        .start()
        .ok();

    // Decorative sprites flanking the logo, centered on its band.
    let deco_y = logo_top + (ascent - sprites::CELL) / 2;
    let apple = FOOD_SPRITE.get();
    if apple != 0 {
        canvas::elem_begin(ELEM_T_APPLE).ok();
        canvas::draw_sprite(w / 2 - 78, deco_y, apple).ok();
        canvas::elem_end();
        sprite::Sprite::from_handle(apple)
            .play(sprite::Mode::PingPong, 260, sprite::REPEAT_FOREVER, 0)
            .ok();
    }
    let head = HEAD_SPRITE.get();
    if head != 0 {
        canvas::elem_begin(ELEM_T_HEAD).ok();
        canvas::draw_sprite(w / 2 + 70, deco_y, head).ok();
        canvas::elem_end();
        sprite::Sprite::from_handle(head)
            .play(sprite::Mode::Loop, 700, sprite::REPEAT_FOREVER, 0)
            .ok();
    }

    record_title_info(w, info_y, level, highscore);

    // Blinking start hint, toggled entirely by the host.
    canvas::elem_begin(ELEM_T_PRESS).ok();
    canvas::draw_text_aligned(0, h - 26, w, "press Y to play", canvas::ALIGN_CENTER);
    canvas::elem_end();
    anim::blink(ELEM_T_PRESS, 900, anim::REPEAT_FOREVER, 0).ok();

    // Static how-to line: a scrolling marquee smears into an unreadable blur
    // on the e-paper's ~5 fps refresh, so keep the controls fixed and terse.
    canvas::draw_text_aligned(0, h - 12, w, "2/4/6/8 turn   5 pause", canvas::ALIGN_CENTER);

    canvas::commit(true);
}

// Body-local y of the title info line, set by show_title's layout pass so
// update_title_info re-records at the same spot.
static TITLE_INFO_Y: PluginCell<i16> = PluginCell::new(40);

/// Speed + high score block on the title screen; re-recorded on key 1-5.
fn record_title_info(w: i16, y: i16, level: u8, highscore: u32) {
    canvas::elem_begin(ELEM_T_INFO).ok();
    // Speed gauge: five boxes, the active ones filled.
    let gauge_w = 5 * 12;
    let gx = (w - gauge_w) / 2;
    canvas::draw_text_aligned(0, y, w / 2 - gauge_w / 2 - 4, "speed", canvas::ALIGN_RIGHT);
    for i in 0..5i16 {
        canvas::draw_rect(gx + i * 12, y, 10, 8, i < level as i16);
    }
    let best = alloc::format!("best {}", highscore);
    canvas::draw_text(gx + gauge_w + 6, y, &best);
    canvas::elem_end();
}

/// Update the title info block after a speed change (partial refresh).
pub fn update_title_info(body_w: u16, level: u8, highscore: u32) {
    canvas::elem_clear(ELEM_T_INFO).ok();
    record_title_info(body_w as i16, TITLE_INFO_Y.get(), level, highscore);
    canvas::commit(false);
}

// --- game screen -------------------------------------------------------------------

/// Head sprite orientation per movement direction. The sheet faces east;
/// combos mirror canvas_demo's 0/90/180/270 rotation table.
fn head_flags(dir: Direction) -> u8 {
    sprite::FLAG_OPAQUE
        | match dir {
            Direction::East => 0,
            Direction::South => sprite::FLAG_ROT_90,
            Direction::West => sprite::FLAG_FLIP_H | sprite::FLAG_FLIP_V,
            Direction::North => {
                sprite::FLAG_ROT_90 | sprite::FLAG_FLIP_H | sprite::FLAG_FLIP_V
            }
        }
}

/// Record the body element's content: one sprite stamp per cell behind the
/// head. Called inside an open `elem_begin(ELEM_BODY)` recording.
fn record_body(layout: &Layout, game: &Game) {
    let body_sprite = BODY_SPRITE.get();
    for cell in game.body().skip(1) {
        let (x, y) = layout.cell_px(cell);
        if body_sprite != 0 {
            canvas::draw_sprite(x, y, body_sprite).ok();
        } else {
            canvas::draw_rect(x + 1, y + 1, sprites::CELL - 2, sprites::CELL - 2, true);
        }
    }
}

/// Score line in the header; re-recorded whenever the score changes.
fn record_score(w: i16, score: u32, best: u32) {
    canvas::elem_begin(ELEM_SCORE).ok();
    let line = alloc::format!("score {}", score);
    canvas::draw_text(2, 2, &line);
    let best_line = alloc::format!("best {}", best);
    canvas::draw_text_aligned(w / 2, 2, w / 2 - 2, &best_line, canvas::ALIGN_RIGHT);
    canvas::elem_end();
}

/// Build the whole game screen for a fresh round: static frame + header,
/// then one element each for head, body, food, score and the crash effect.
pub fn start_round(body_w: u16, _body_h: u16, layout: &Layout, game: &Game, best: u32) {
    let w = body_w as i16;
    canvas::clear_ex(canvas::CLEAR_KEEP_SPRITES).ok();
    reset_draw_state();

    // Static background: header separator + playfield border.
    canvas::hline(0, HEADER_H - 2, w);
    let field_w = layout.grid_w as i16 * sprites::CELL;
    let field_h = layout.grid_h as i16 * sprites::CELL;
    canvas::draw_rect(
        layout.field_x - BORDER,
        layout.field_y - BORDER,
        field_w + 2 * BORDER,
        field_h + 2 * BORDER,
        false,
    );

    record_score(w, game.score(), best);

    // Body: absolute cell stamps, re-recorded in place every step.
    canvas::elem_begin(ELEM_BODY).ok();
    record_body(layout, game);
    canvas::elem_end();
    canvas::elem_set_z(ELEM_BODY, Z_FIELD).ok();

    // Food: recorded at the field origin, positioned via its offset so a
    // respawn is a single elem_set_offset call.
    let food_sprite = FOOD_SPRITE.get();
    if food_sprite != 0 {
        canvas::elem_begin(ELEM_FOOD).ok();
        let (fx, fy) = layout.cell_px((0, 0));
        canvas::draw_sprite(fx, fy, food_sprite).ok();
        canvas::elem_end();
        canvas::elem_set_z(ELEM_FOOD, Z_FIELD).ok();
        move_food(layout, game);
        sprite::Sprite::from_handle(food_sprite)
            .play(sprite::Mode::PingPong, 260, sprite::REPEAT_FOREVER, 0)
            .ok();
    }

    // Head: same origin trick; every step tweens the offset one cell over.
    let head_sprite = HEAD_SPRITE.get();
    if head_sprite != 0 {
        sprite::Sprite::from_handle(head_sprite)
            .set_flags(head_flags(game.direction()))
            .ok();
        canvas::elem_begin(ELEM_HEAD).ok();
        let (hx, hy) = layout.cell_px((0, 0));
        canvas::draw_sprite(hx, hy, head_sprite).ok();
        canvas::elem_end();
        canvas::elem_set_z(ELEM_HEAD, Z_HEAD).ok();
        let (ox, oy) = layout.cell_offset(game.head());
        canvas::elem_set_offset(ELEM_HEAD, ox, oy).ok();
    }

    // Crash explosion: pre-recorded and hidden until the snake dies.
    let boom_sprite = BOOM_SPRITE.get();
    if boom_sprite != 0 {
        canvas::elem_begin(ELEM_FX).ok();
        let (bx, by) = layout.cell_px((0, 0));
        canvas::draw_sprite(
            bx - (sprites::BOOM_SIZE - sprites::CELL) / 2,
            by - (sprites::BOOM_SIZE - sprites::CELL) / 2,
            boom_sprite,
        )
        .ok();
        canvas::elem_end();
        canvas::elem_set_z(ELEM_FX, Z_FX).ok();
        canvas::elem_show(ELEM_FX, false).ok();
    }

    canvas::commit(true);
}

/// Place the food element's offset on the game's current food cell.
fn move_food(layout: &Layout, game: &Game) {
    match game.food() {
        Some(cell) => {
            let (ox, oy) = layout.cell_offset(cell);
            canvas::elem_set_offset(ELEM_FOOD, ox, oy).ok();
            canvas::elem_show(ELEM_FOOD, true).ok();
        }
        // Grid full - nothing left to eat.
        None => {
            canvas::elem_show(ELEM_FOOD, false).ok();
        }
    }
}

/// Render one game step: tween the head into its new cell, re-record the
/// body, and touch food/score only when something was eaten.
pub fn render_step(
    body_w: u16,
    layout: &Layout,
    game: &Game,
    old_head: (u8, u8),
    ate: bool,
    best: u32,
    step_ms: u32,
) {
    let head_sprite = HEAD_SPRITE.get();
    if head_sprite != 0 {
        sprite::Sprite::from_handle(head_sprite)
            .set_flags(head_flags(game.direction()))
            .ok();
    }
    let (fx, fy) = layout.cell_offset(old_head);
    let (tx, ty) = layout.cell_offset(game.head());
    anim::Anim::element(ELEM_HEAD)
        .from(fx, fy)
        .to(tx, ty)
        .duration_ms(step_ms.min(u16::MAX as u32) as u16)
        .ease(anim::Ease::Linear)
        .start()
        .ok();

    canvas::elem_clear(ELEM_BODY).ok();
    canvas::elem_begin(ELEM_BODY).ok();
    record_body(layout, game);
    canvas::elem_end();

    if ate {
        canvas::elem_clear(ELEM_SCORE).ok();
        record_score(body_w as i16, game.score(), best);
        move_food(layout, game);
    }

    canvas::commit(false);
}

// --- pause / crash / game over --------------------------------------------------

/// Show or hide the blinking PAUSE badge over the field.
pub fn show_pause(body_w: u16, body_h: u16, paused: bool) {
    if !paused {
        canvas::elem_remove(ELEM_PAUSE).ok();
        canvas::commit(false);
        return;
    }
    let (w, h) = (body_w as i16, body_h as i16);
    let (pw, ph) = (72, 22);
    let (px, py) = ((w - pw) / 2, (h - ph) / 2);
    canvas::elem_begin(ELEM_PAUSE).ok();
    // White cut-out first so the badge really covers the field below it.
    canvas::set_ink_white(true).ok();
    canvas::draw_round_rect(px, py, pw, ph, 4, true);
    canvas::set_ink_white(false).ok();
    canvas::draw_round_rect(px, py, pw, ph, 4, false);
    canvas::draw_text_aligned(px, py + 7, pw, "PAUSE", canvas::ALIGN_CENTER);
    canvas::elem_end();
    canvas::elem_set_z(ELEM_PAUSE, Z_PAUSE).ok();
    anim::blink(ELEM_PAUSE, 900, anim::REPEAT_FOREVER, 0).ok();
    canvas::commit(false);
}

/// Fire the crash explosion on the cell the snake died in. The sprite's
/// completion action (`ACT_BOOM_DONE`) then brings up the game-over panel.
pub fn play_crash_fx(layout: &Layout, cell: (u8, u8)) {
    let boom_sprite = BOOM_SPRITE.get();
    if boom_sprite == 0 {
        return;
    }
    let (ox, oy) = layout.cell_offset(cell);
    canvas::elem_set_offset(ELEM_FX, ox, oy).ok();
    canvas::elem_show(ELEM_FX, true).ok();
    let boom = sprite::Sprite::from_handle(boom_sprite);
    boom.set_frame(0).ok();
    boom.play(sprite::Mode::Once, sprites::BOOM_FRAME_MS, 1, ACT_BOOM_DONE)
        .ok();
    canvas::commit(false);
}

/// Drop the game-over panel in from the top with a bounce.
pub fn show_game_over(body_w: u16, body_h: u16, score: u32, best: u32, new_best: bool) {
    let (w, h) = (body_w as i16, body_h as i16);
    let (pw, ph) = (150, 64);
    let (px, py) = ((w - pw) / 2, (h - ph) / 2);

    canvas::elem_remove(ELEM_OVERLAY).ok();
    canvas::elem_begin(ELEM_OVERLAY).ok();
    canvas::set_ink_white(true).ok();
    canvas::draw_round_rect(px, py, pw, ph, 5, true);
    canvas::set_ink_white(false).ok();
    canvas::draw_round_rect(px, py, pw, ph, 5, false);
    canvas::draw_rect(px, py, pw, 16, true);
    canvas::set_text_inverted(true);
    canvas::draw_text_aligned(px, py + 4, pw, "GAME OVER", canvas::ALIGN_CENTER);
    canvas::set_text_inverted(false);
    let line = if new_best {
        alloc::format!("NEW BEST {}", score)
    } else {
        alloc::format!("score {}   best {}", score, best)
    };
    canvas::draw_text_aligned(px, py + 24, pw, &line, canvas::ALIGN_CENTER);
    canvas::draw_text_aligned(px, py + 44, pw, "Y again   N title", canvas::ALIGN_CENTER);
    canvas::elem_end();
    canvas::elem_set_z(ELEM_OVERLAY, Z_OVERLAY).ok();

    // Slide in from above the body; bounce settles it on the field.
    canvas::elem_show(ELEM_OVERLAY, false).ok();
    anim::Anim::element(ELEM_OVERLAY)
        .from(0, -(py + ph))
        .to(0, 0)
        .duration_ms(900)
        .ease(anim::Ease::Bounce)
        .show_on_start()
        .start()
        .ok();
    canvas::commit(false);
}
