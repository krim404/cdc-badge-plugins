//! \file
//! \brief Snake for the CDC Badge - a showcase for the canvas animation
//! framework (elements, host-driven tweens, sprites, white ink).
//!
//! Screens: title (speed 1-5, Y start, N exit) -> playing (2/4/6/8 steer,
//! 5 pause, N back) -> crash explosion -> game over (Y again, N title).
//! The speed level and one high score per level persist in NVS.
//!
//! Pure game rules live in [`game`] and are host-tested; [`render`] owns
//! every canvas/sprite/anim call; this file is the lifecycle + input glue.

// no_std only on the badge; host-side `cargo test` builds against std.
#![cfg_attr(target_arch = "wasm32", no_std)]

extern crate alloc;

pub mod game;

#[cfg(target_arch = "wasm32")]
mod render;
#[cfg(target_arch = "wasm32")]
mod sprites;

#[cfg(target_arch = "wasm32")]
mod plugin {
    use core::cell::Cell;
    use core::ops::Deref;

    use cdc_badge_plugin::{canvas, event, log, nvs, plugin_main, random, time, ui};

    use crate::game::{
        clamp_speed_level, step_interval_ms, Direction, Game, StepOutcome, HIGHSCORE_KEYS,
    };
    use crate::render::{self, Layout};

    plugin_main!();

    const TAG: &str = "snake";

    /// Canvas key events (`user_data` = ASCII key code).
    pub(crate) const ACT_KEY: u32 = 9200;
    /// Completion of the crash explosion sprite.
    pub(crate) const ACT_BOOM_DONE: u32 = 9201;
    /// DISPLAY_REFRESH event subscription (`user_data` = 1 begin, 0 end).
    const ACT_REFRESH: u32 = 9202;

    /// NVS key for the persisted speed level.
    const NVS_SPEED: &str = "speed";
    const DEFAULT_SPEED: u8 = 3;

    const K_Y: u32 = b'Y' as u32;
    const K_N: u32 = b'N' as u32;
    const K_UP: u32 = b'2' as u32;
    const K_LEFT: u32 = b'4' as u32;
    const K_RIGHT: u32 = b'6' as u32;
    const K_DOWN: u32 = b'8' as u32;
    const K_PAUSE: u32 = b'5' as u32;

    /// Single-threaded WASM plugin: a Sync wrapper around Cell is enough.
    /// Mirrors the PluginCell pattern from canvas_demo/sci_calc.
    pub(crate) struct PluginCell<T>(Cell<T>);
    unsafe impl<T> Sync for PluginCell<T> {}
    impl<T: Copy> PluginCell<T> {
        pub(crate) const fn new(v: T) -> Self {
            Self(Cell::new(v))
        }
    }
    impl<T> Deref for PluginCell<T> {
        type Target = Cell<T>;
        fn deref(&self) -> &Cell<T> {
            &self.0
        }
    }

    /// The running game lives in a RefCell because it is not `Copy`.
    struct PluginRef<T>(core::cell::RefCell<T>);
    unsafe impl<T> Sync for PluginRef<T> {}

    #[derive(Copy, Clone, PartialEq, Eq)]
    enum Screen {
        Title,
        Playing,
        Paused,
        /// Crash explosion is running; keys are ignored until it finishes.
        Crashing,
        GameOver,
    }

    static SCREEN: PluginCell<Screen> = PluginCell::new(Screen::Title);
    static GAME: PluginRef<Option<Game>> = PluginRef(core::cell::RefCell::new(None));
    static LAYOUT: PluginCell<Option<Layout>> = PluginCell::new(None);
    static BODY_SIZE: PluginCell<(u16, u16)> = PluginCell::new((0, 0));
    static LEVEL: PluginCell<u8> = PluginCell::new(DEFAULT_SPEED);
    /// High score of the currently selected level, mirrored from NVS.
    static BEST: PluginCell<u32> = PluginCell::new(0);
    static LAST_STEP_MS: PluginCell<u64> = PluginCell::new(0);
    /// Whether the round that just crashed set a new high score.
    static NEW_BEST: PluginCell<bool> = PluginCell::new(false);
    /// Uptime of the crash, for the explosion-timeout safety net.
    static CRASH_MS: PluginCell<u64> = PluginCell::new(0);
    /// A FAST/FULL e-paper refresh is in progress: hold the game step so the
    /// snake does not move while the panel is unreadable.
    static REFRESH_BUSY: PluginCell<bool> = PluginCell::new(false);
    /// Uptime the current refresh pause began (watchdog reference).
    static REFRESH_SINCE: PluginCell<u64> = PluginCell::new(0);
    /// DISPLAY_REFRESH subscription handle, released on exit (0 = none).
    static REFRESH_SUB: PluginCell<u32> = PluginCell::new(0);

    /// Explosion runtime (4 frames x 220 ms) plus margin; after that the
    /// game-over panel is forced even without the completion action.
    const CRASH_FX_TIMEOUT_MS: u64 = 1500;

    /// Fail-safe: if a DISPLAY_REFRESH end is ever dropped, resume anyway once
    /// the pause exceeds this (> the 1800 ms FULL window) so we never hang.
    const REFRESH_WATCHDOG_MS: u64 = 2000;

    /// Load the persisted high score for a level (0 when unset).
    fn load_best(level: u8) -> u32 {
        nvs::get_u32(HIGHSCORE_KEYS[(level - 1) as usize]).unwrap_or(0)
    }

    /// Persist a new high score and mirror it; returns true when it beat
    /// the stored one.
    fn save_best_if_beaten(level: u8, score: u32) -> bool {
        if score <= BEST.get() {
            return false;
        }
        BEST.set(score);
        if let Err(e) = nvs::set_u32(HIGHSCORE_KEYS[(level - 1) as usize], score) {
            log::warn(TAG, &alloc::format!("high score not saved: {:?}", e));
        }
        true
    }

    /// Start a fresh round on the current layout and speed level.
    fn start_round() {
        let layout = match LAYOUT.get() {
            Some(l) => l,
            None => return,
        };
        let seed = random::u32().unwrap_or(time::uptime_ms() as u32);
        let game = match Game::new(layout.grid_w, layout.grid_h, seed) {
            Some(g) => g,
            None => {
                // Body too small for the minimum grid - bail out gracefully.
                ui::push_toast("screen too small", 0, 2000);
                ui::pop();
                return;
            }
        };
        let (w, h) = BODY_SIZE.get();
        render::start_round(w, h, &layout, &game, BEST.get());
        *GAME.0.borrow_mut() = Some(game);
        LAST_STEP_MS.set(time::uptime_ms());
        SCREEN.set(Screen::Playing);
    }

    /// Leave the round (from any in-game screen) back to the title.
    fn back_to_title() {
        *GAME.0.borrow_mut() = None;
        let (w, h) = BODY_SIZE.get();
        render::show_title(w, h, LEVEL.get(), BEST.get());
        SCREEN.set(Screen::Title);
    }

    /// Advance the game by one step and render the delta.
    fn step_game() {
        let layout = match LAYOUT.get() {
            Some(l) => l,
            None => return,
        };
        let mut slot = GAME.0.borrow_mut();
        let game = match slot.as_mut() {
            Some(g) => g,
            None => return,
        };

        let old_head = game.head();
        match game.step() {
            StepOutcome::Crashed => {
                let level = LEVEL.get();
                let score = game.score();
                let new_best = save_best_if_beaten(level, score);
                NEW_BEST.set(new_best);
                if new_best {
                    log::info(TAG, &alloc::format!("new best {} on level {}", score, level));
                }
                SCREEN.set(Screen::Crashing);
                CRASH_MS.set(time::uptime_ms());
                render::play_crash_fx(&layout, old_head);
            }
            outcome => {
                let (w, _h) = BODY_SIZE.get();
                render::render_step(
                    w,
                    &layout,
                    game,
                    old_head,
                    outcome == StepOutcome::Ate,
                    BEST.get(),
                    step_interval_ms(LEVEL.get()),
                );
            }
        }
    }

    fn handle_title_key(key: u32) {
        match key {
            K_Y => start_round(),
            K_N => ui::pop(),
            k if (b'1' as u32..=b'5' as u32).contains(&k) => {
                let level = clamp_speed_level(k - b'0' as u32);
                LEVEL.set(level);
                BEST.set(load_best(level));
                if let Err(e) = nvs::set_u32(NVS_SPEED, level as u32) {
                    log::warn(TAG, &alloc::format!("speed not saved: {:?}", e));
                }
                let (w, _h) = BODY_SIZE.get();
                render::update_title_info(w, LEVEL.get(), BEST.get());
            }
            _ => {}
        }
    }

    fn handle_playing_key(key: u32) {
        let dir = match key {
            K_UP => Some(Direction::North),
            K_DOWN => Some(Direction::South),
            K_LEFT => Some(Direction::West),
            K_RIGHT => Some(Direction::East),
            _ => None,
        };
        if let Some(dir) = dir {
            if let Some(game) = GAME.0.borrow_mut().as_mut() {
                game.steer(dir);
            }
            return;
        }
        match key {
            K_PAUSE => {
                let (w, h) = BODY_SIZE.get();
                render::show_pause(w, h, true);
                SCREEN.set(Screen::Paused);
            }
            K_N => back_to_title(),
            _ => {}
        }
    }

    fn handle_paused_key(key: u32) {
        match key {
            K_PAUSE | K_Y => {
                let (w, h) = BODY_SIZE.get();
                render::show_pause(w, h, false);
                // Restart the step clock so the pause does not burst-step.
                LAST_STEP_MS.set(time::uptime_ms());
                SCREEN.set(Screen::Playing);
            }
            K_N => back_to_title(),
            _ => {}
        }
    }

    fn handle_game_over_key(key: u32) {
        match key {
            K_Y => start_round(),
            _ => back_to_title(),
        }
    }

    /// Bring up the game-over panel (from the explosion's completion action,
    /// its timeout fallback, or a key pressed during the explosion).
    fn show_game_over_panel() {
        let score = GAME.0.borrow().as_ref().map(Game::score).unwrap_or(0);
        let (w, h) = BODY_SIZE.get();
        render::show_game_over(w, h, score, BEST.get(), NEW_BEST.get());
        SCREEN.set(Screen::GameOver);
    }

    // --- lifecycle exports ----------------------------------------------------

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
        // Empty title: no title bar, the body gets the whole panel height.
        canvas::push("", ACT_KEY, 0);
        // Max animation rate: the fastest speed level steps every 450 ms.
        canvas::set_anim_policy(canvas::ANIM_REFRESH_AUTO, 5).ok();
        // Pause stepping while the panel does a FAST/FULL refresh (unreadable).
        if let Ok(id) = event::subscribe(event::DISPLAY_REFRESH, ACT_REFRESH) {
            REFRESH_SUB.set(id);
        }

        let (w, h) = canvas::body_size();
        BODY_SIZE.set((w, h));
        LAYOUT.set(Some(Layout::compute(w, h)));

        let level = clamp_speed_level(nvs::get_u32(NVS_SPEED).unwrap_or(DEFAULT_SPEED as u32));
        LEVEL.set(level);
        BEST.set(load_best(level));

        render::create_sprites();
        render::show_title(w, h, level, BEST.get());
        SCREEN.set(Screen::Title);
        0
    }

    #[no_mangle]
    pub extern "C" fn plugin_on_exit() -> i32 {
        let sub = REFRESH_SUB.get();
        if sub != 0 {
            event::unsubscribe(sub);
            REFRESH_SUB.set(0);
        }
        REFRESH_BUSY.set(false);
        *GAME.0.borrow_mut() = None;
        0
    }

    #[no_mangle]
    pub extern "C" fn plugin_on_tick(uptime_ms: u64) -> i32 {
        match SCREEN.get() {
            Screen::Playing => {
                // Hold the step during a FAST/FULL refresh so the snake does
                // not move unseen; do NOT bump LAST_STEP_MS, so it resumes the
                // instant the refresh ends. Watchdog guards a dropped end.
                if REFRESH_BUSY.get() {
                    if uptime_ms.saturating_sub(REFRESH_SINCE.get()) < REFRESH_WATCHDOG_MS {
                        return 0;
                    }
                    REFRESH_BUSY.set(false);
                }
                let interval = step_interval_ms(LEVEL.get()) as u64;
                if uptime_ms.saturating_sub(LAST_STEP_MS.get()) >= interval {
                    LAST_STEP_MS.set(uptime_ms);
                    step_game();
                }
            }
            // Safety net: if the explosion's completion action never
            // arrives, force the game-over panel instead of hanging.
            Screen::Crashing => {
                if uptime_ms.saturating_sub(CRASH_MS.get()) >= CRASH_FX_TIMEOUT_MS {
                    show_game_over_panel();
                }
            }
            _ => {}
        }
        0
    }

    #[no_mangle]
    pub extern "C" fn plugin_on_action(action_id: u32, _idx: u32, user_data: u32) -> i32 {
        match action_id {
            ACT_REFRESH => {
                let busy = user_data != 0;
                REFRESH_BUSY.set(busy);
                if busy {
                    REFRESH_SINCE.set(time::uptime_ms());
                }
                0
            }
            ACT_BOOM_DONE => {
                if SCREEN.get() == Screen::Crashing {
                    show_game_over_panel();
                }
                0
            }
            ACT_KEY => {
                match SCREEN.get() {
                    Screen::Title => handle_title_key(user_data),
                    Screen::Playing => handle_playing_key(user_data),
                    Screen::Paused => handle_paused_key(user_data),
                    // Impatient key during the explosion: skip ahead.
                    Screen::Crashing => match user_data {
                        K_N => back_to_title(),
                        _ => show_game_over_panel(),
                    },
                    Screen::GameOver => handle_game_over_key(user_data),
                }
                0
            }
            _ => 0,
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub(crate) use plugin::{PluginCell, ACT_BOOM_DONE};
