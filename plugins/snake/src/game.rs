//! \file
//! \brief Pure snake game logic - no SDK imports, fully host-testable.
//!
//! The playfield is a cell grid (head-first snake body, one food cell).
//! `Game::step` advances one move: it applies the buffered direction,
//! detects wall/self collisions and handles growth and food respawn.
//! Randomness comes from an injected xorshift seed so every test run is
//! deterministic; the badge glue seeds it from `random::u32()`.

use alloc::collections::VecDeque;

/// Points awarded per food eaten.
pub const POINTS_PER_FOOD: u32 = 10;
/// Snake length at the start of a round.
pub const START_LENGTH: usize = 3;
/// Number of selectable speed levels (keys 1..=5 on the title screen).
pub const SPEED_LEVELS: u8 = 5;
/// Step interval per speed level, slowest first. The e-paper partial refresh
/// takes ~250 ms (~5 fps ceiling); the 450 ms floor keeps every step well
/// clear of a refresh so no two steps merge into one visible redraw, and the
/// wide spacing makes each level feel distinctly slower than the next.
pub const STEP_INTERVAL_MS: [u32; SPEED_LEVELS as usize] = [1600, 1200, 900, 650, 450];
/// Buffered steering inputs applied one per step (rolling window: a later
/// input evicts the oldest queued turn).
const MAX_PENDING: usize = 2;
/// NVS keys for the per-level high scores (index = level - 1).
pub const HIGHSCORE_KEYS: [&str; SPEED_LEVELS as usize] = ["hs_1", "hs_2", "hs_3", "hs_4", "hs_5"];
/// Smallest playable grid: room for the start snake plus a food cell.
pub const MIN_GRID_W: u8 = 8;
pub const MIN_GRID_H: u8 = 4;

/// Movement direction on the grid.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Direction {
    North,
    East,
    South,
    West,
}

impl Direction {
    /// The 180-degree reverse of this direction.
    pub fn opposite(self) -> Direction {
        match self {
            Direction::North => Direction::South,
            Direction::East => Direction::West,
            Direction::South => Direction::North,
            Direction::West => Direction::East,
        }
    }

    /// Grid delta (dx, dy) for one step; y grows downwards.
    pub fn delta(self) -> (i16, i16) {
        match self {
            Direction::North => (0, -1),
            Direction::East => (1, 0),
            Direction::South => (0, 1),
            Direction::West => (-1, 0),
        }
    }
}

/// Result of advancing the game by one step.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum StepOutcome {
    /// Normal move, nothing special happened.
    Moved,
    /// Food eaten: snake grew, score increased, food respawned.
    Ate,
    /// Wall or self collision: the game is over.
    Crashed,
}

/// Deterministic xorshift32 PRNG - tiny, dependency-free, seedable.
pub struct XorShift32 {
    state: u32,
}

impl XorShift32 {
    /// A zero seed would lock xorshift at zero forever, so it is remapped.
    pub fn new(seed: u32) -> XorShift32 {
        XorShift32 {
            state: if seed == 0 { 0x9E37_79B9 } else { seed },
        }
    }

    pub fn next(&mut self) -> u32 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.state = x;
        x
    }

    /// Uniform-ish value in `0..bound` (bound > 0).
    pub fn below(&mut self, bound: u32) -> u32 {
        self.next() % bound
    }
}

/// One running snake round on a `w x h` cell grid.
pub struct Game {
    w: u8,
    h: u8,
    /// Body cells, head at the front.
    snake: VecDeque<(u8, u8)>,
    dir: Direction,
    /// Buffered steering inputs, applied one per step (FIFO, capacity
    /// `MAX_PENDING`).
    pending: VecDeque<Direction>,
    /// `None` only when the snake has filled the whole grid.
    food: Option<(u8, u8)>,
    score: u32,
    alive: bool,
    rng: XorShift32,
}

impl Game {
    /// Start a round: snake of `START_LENGTH` centered, heading east,
    /// food on a free cell. `None` when the grid is too small to play.
    pub fn new(w: u8, h: u8, seed: u32) -> Option<Game> {
        if w < MIN_GRID_W || h < MIN_GRID_H {
            return None;
        }

        let (cx, cy) = (w / 2, h / 2);
        let snake: VecDeque<(u8, u8)> = (0..START_LENGTH)
            .map(|i| (cx - i as u8, cy))
            .collect();

        let mut game = Game {
            w,
            h,
            snake,
            dir: Direction::East,
            pending: VecDeque::new(),
            food: None,
            score: 0,
            alive: true,
            rng: XorShift32::new(seed),
        };
        game.spawn_food();
        Some(game)
    }

    pub fn width(&self) -> u8 {
        self.w
    }

    pub fn height(&self) -> u8 {
        self.h
    }

    pub fn head(&self) -> (u8, u8) {
        self.snake.front().copied().unwrap_or((0, 0))
    }

    /// Body cells head-first, including the head.
    pub fn body(&self) -> impl Iterator<Item = (u8, u8)> + '_ {
        self.snake.iter().copied()
    }

    pub fn len(&self) -> usize {
        self.snake.len()
    }

    pub fn is_empty(&self) -> bool {
        self.snake.is_empty()
    }

    pub fn food(&self) -> Option<(u8, u8)> {
        self.food
    }

    pub fn score(&self) -> u32 {
        self.score
    }

    pub fn is_alive(&self) -> bool {
        self.alive
    }

    pub fn direction(&self) -> Direction {
        self.dir
    }

    /// Buffer a steering input, applied on a later step. Turns queue up to
    /// `MAX_PENDING` deep so a quick two-turn hook is not swallowed; a further
    /// input evicts the oldest queued turn (a rolling window, newest wins).
    /// No-ops and reversals of the last queued heading are dropped.
    pub fn steer(&mut self, dir: Direction) {
        if !self.alive {
            return;
        }
        // Validate against the heading that will be active after any already
        // queued turns resolve, so a second perpendicular turn is accepted.
        let effective = self.pending.back().copied().unwrap_or(self.dir);
        if dir == effective || dir == effective.opposite() {
            return;
        }
        self.pending.push_back(dir);
        if self.pending.len() > MAX_PENDING {
            self.pending.pop_front();
        }
    }

    /// Advance one step. On `Crashed` the game stays frozen; further calls
    /// keep returning `Crashed` without changing state.
    pub fn step(&mut self) -> StepOutcome {
        if !self.alive {
            return StepOutcome::Crashed;
        }

        // Apply the next buffered turn that is legal from the current heading.
        // Eviction can leave a stale front (parallel/reverse to the real
        // movement); skip such entries instead of turning into the neck.
        while let Some(dir) = self.pending.pop_front() {
            if dir == self.dir || dir == self.dir.opposite() {
                continue;
            }
            self.dir = dir;
            break;
        }

        let (dx, dy) = self.dir.delta();
        let (hx, hy) = self.head();
        let (nx, ny) = (hx as i16 + dx, hy as i16 + dy);
        if nx < 0 || ny < 0 || nx >= self.w as i16 || ny >= self.h as i16 {
            self.alive = false;
            return StepOutcome::Crashed;
        }

        let new_head = (nx as u8, ny as u8);
        let eats = self.food == Some(new_head);
        // The tail cell is legal to enter unless the snake grows this step -
        // it vacates its cell in the same move.
        let tail = self.snake.back().copied();
        let hits_body = self
            .snake
            .iter()
            .any(|&c| c == new_head && !(Some(c) == tail && !eats));
        if hits_body {
            self.alive = false;
            return StepOutcome::Crashed;
        }

        self.snake.push_front(new_head);
        if eats {
            self.score += POINTS_PER_FOOD;
            self.spawn_food();
            return StepOutcome::Ate;
        }
        self.snake.pop_back();
        StepOutcome::Moved
    }

    /// Place the food on a uniformly chosen free cell; `None` when the
    /// snake fills the whole grid (the player has effectively won).
    fn spawn_food(&mut self) {
        let total = self.w as u32 * self.h as u32;
        let free = total - self.snake.len() as u32;
        if free == 0 {
            self.food = None;
            return;
        }

        let mut target = self.rng.below(free);
        for y in 0..self.h {
            for x in 0..self.w {
                if self.snake.iter().any(|&c| c == (x, y)) {
                    continue;
                }
                if target == 0 {
                    self.food = Some((x, y));
                    return;
                }
                target -= 1;
            }
        }
        // Unreachable: `free` counted exactly the cells iterated above.
        self.food = None;
    }

    /// Test-only constructor with an exact body layout (head first).
    #[cfg(test)]
    pub(crate) fn for_test(
        w: u8,
        h: u8,
        body: &[(u8, u8)],
        dir: Direction,
        food: Option<(u8, u8)>,
    ) -> Game {
        Game {
            w,
            h,
            snake: body.iter().copied().collect(),
            dir,
            pending: VecDeque::new(),
            food,
            score: 0,
            alive: true,
            rng: XorShift32 { state: 1 },
        }
    }
}

/// Clamp a stored speed level into the valid `1..=SPEED_LEVELS` range.
pub fn clamp_speed_level(level: u32) -> u8 {
    level.clamp(1, SPEED_LEVELS as u32) as u8
}

/// Step interval in ms for a speed level (clamped defensively).
pub fn step_interval_ms(level: u8) -> u32 {
    STEP_INTERVAL_MS[(clamp_speed_level(level as u32) - 1) as usize]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Grid used by most tests: big enough that the start snake has room.
    const W: u8 = 12;
    const H: u8 = 6;

    fn game() -> Game {
        Game::new(W, H, 42).expect("test grid must be playable")
    }

    // --- construction -------------------------------------------------------

    #[test]
    fn new_game_starts_centered_east_with_start_length() {
        let g = game();
        assert!(g.is_alive());
        assert_eq!(g.len(), START_LENGTH);
        assert_eq!(g.direction(), Direction::East);
        assert_eq!(g.score(), 0);
        let (hx, hy) = g.head();
        assert_eq!((hx, hy), (W / 2, H / 2));
        // Body extends west of the head, contiguous.
        let body: alloc::vec::Vec<_> = g.body().collect();
        assert_eq!(body[0], (hx, hy));
        assert_eq!(body[1], (hx - 1, hy));
        assert_eq!(body[2], (hx - 2, hy));
    }

    #[test]
    fn new_game_spawns_food_off_the_snake() {
        let g = game();
        let food = g.food().expect("fresh game must have food");
        assert!(g.body().all(|c| c != food));
        assert!(food.0 < W && food.1 < H);
    }

    #[test]
    fn too_small_grid_is_rejected() {
        assert!(Game::new(MIN_GRID_W - 1, H, 1).is_none());
        assert!(Game::new(W, MIN_GRID_H - 1, 1).is_none());
        assert!(Game::new(MIN_GRID_W, MIN_GRID_H, 1).is_some());
    }

    // --- movement -----------------------------------------------------------

    #[test]
    fn step_moves_head_one_cell_and_keeps_length() {
        let mut g = game();
        let (hx, hy) = g.head();
        assert_eq!(g.step(), StepOutcome::Moved);
        assert_eq!(g.head(), (hx + 1, hy));
        assert_eq!(g.len(), START_LENGTH);
    }

    #[test]
    fn steer_applies_on_next_step() {
        let mut g = game();
        let (hx, hy) = g.head();
        g.steer(Direction::South);
        g.step();
        assert_eq!(g.head(), (hx, hy + 1));
        assert_eq!(g.direction(), Direction::South);
    }

    #[test]
    fn reversing_into_the_neck_is_ignored() {
        let mut g = game();
        let (hx, hy) = g.head();
        g.steer(Direction::West); // 180 degrees from East - must be ignored
        g.step();
        assert_eq!(g.head(), (hx + 1, hy));
    }

    #[test]
    fn buffered_turns_apply_one_per_step_in_order() {
        // Two quick turns queue up and fire on successive steps (a hook), so
        // the second is not swallowed by the first.
        let mut g = game();
        let (hx, hy) = g.head();
        g.steer(Direction::South);
        g.steer(Direction::East); // legal follow-up after South
        g.step();
        assert_eq!(g.head(), (hx, hy + 1));
        assert_eq!(g.direction(), Direction::South);
        g.step();
        assert_eq!(g.head(), (hx + 1, hy + 1));
        assert_eq!(g.direction(), Direction::East);
    }

    #[test]
    fn a_third_input_evicts_the_oldest_queued_turn() {
        // Rolling window of depth MAX_PENDING: a third turn drops the oldest
        // (South) and keeps the newest intent. The game must not crash, and
        // the first applied step honours a still-legal buffered turn.
        let mut g = game();
        let (hx, hy) = g.head();
        g.steer(Direction::North); // queued
        g.steer(Direction::West); // queued (legal after North)
        g.steer(Direction::South); // evicts North; queue is now [West, South]
        let outcome = g.step();
        assert_ne!(outcome, StepOutcome::Crashed);
        // West would reverse the actual East movement and is skipped, so the
        // newest legal intent (South) takes effect.
        assert_eq!(g.head(), (hx, hy + 1));
        assert_eq!(g.direction(), Direction::South);
    }

    #[test]
    fn reversal_of_actual_movement_is_blocked_on_the_first_turn() {
        // Moving East with North already buffered: West is a legal *follow-up*
        // to North, so it is queued - but the first step still applies North,
        // and West can never reverse the actual movement (step self-heals).
        let mut g = game();
        let (hx, hy) = g.head();
        g.steer(Direction::North);
        g.steer(Direction::West);
        g.step();
        assert_eq!(g.head(), (hx, hy - 1));
    }

    // --- collisions ----------------------------------------------------------

    #[test]
    fn wall_collision_crashes_and_freezes() {
        let mut g = game();
        let mut outcome = StepOutcome::Moved;
        // Head starts at W/2 and runs east; it must crash at the wall.
        for _ in 0..W {
            outcome = g.step();
            if outcome == StepOutcome::Crashed {
                break;
            }
        }
        assert_eq!(outcome, StepOutcome::Crashed);
        assert!(!g.is_alive());
        let head = g.head();
        assert_eq!(g.step(), StepOutcome::Crashed);
        assert_eq!(g.head(), head, "crashed game must not keep moving");
    }

    #[test]
    fn self_collision_crashes() {
        // Hook shape: steering south from the head runs into a body cell
        // that is NOT the tail, so it must crash.
        //   (2,2)h (3,2) (4,2)
        //   (2,3)  (3,3) (4,3)   tail = (1,3)
        let body = [
            (2, 2),
            (3, 2),
            (4, 2),
            (4, 3),
            (3, 3),
            (2, 3),
            (1, 3),
        ];
        let mut g = Game::for_test(W, H, &body, Direction::West, Some((9, 1)));
        g.steer(Direction::South);
        assert_eq!(g.step(), StepOutcome::Crashed);
        assert!(!g.is_alive());
    }

    #[test]
    fn moving_into_the_leaving_tail_cell_is_not_a_crash() {
        // Classic rule: the tail vacates its cell in the same step, so a
        // length-4 snake can circle a 2x2 block forever.
        //   (1,1)h (2,1)
        //   (1,2)t (2,2)
        let body = [(1, 1), (2, 1), (2, 2), (1, 2)];
        let mut g = Game::for_test(W, H, &body, Direction::West, Some((9, 1)));
        for dir in [
            Direction::South,
            Direction::East,
            Direction::North,
            Direction::West,
            Direction::South,
            Direction::East,
        ] {
            g.steer(dir);
            let outcome = g.step();
            assert_ne!(outcome, StepOutcome::Crashed, "died turning {:?}", dir);
            assert_eq!(g.len(), 4);
        }
    }

    // --- food + growth -------------------------------------------------------

    #[test]
    fn eating_grows_scores_and_respawns_food() {
        let mut g = game();
        let len_before = g.len();
        let outcome = eat_once(&mut g);
        assert_eq!(outcome, StepOutcome::Ate);
        assert_eq!(g.len(), len_before + 1);
        assert_eq!(g.score(), POINTS_PER_FOOD);
        let food = g.food().expect("food must respawn");
        assert!(g.body().all(|c| c != food));
    }

    #[test]
    fn food_respawn_avoids_the_snake_even_when_the_grid_is_nearly_full() {
        // Serpentine body covering rows 0..3 of the minimal 8x4 grid (24 of
        // 32 cells), head at (7,2), food right below. After eating, only the
        // 7 remaining cells of row 3 are free - the food must land there,
        // whatever the seed.
        let mut body = alloc::vec::Vec::new();
        for x in 0..MIN_GRID_W {
            body.push((x, 0));
        }
        for x in (0..MIN_GRID_W).rev() {
            body.push((x, 1));
        }
        for x in 0..MIN_GRID_W {
            body.push((x, 2));
        }
        body.reverse(); // head-first: head = (7,2)
        for seed in [1u32, 2, 3, 42, 0xDEAD_BEEF] {
            let mut g = Game::for_test(MIN_GRID_W, MIN_GRID_H, &body, Direction::East, Some((7, 3)));
            g.rng = XorShift32::new(seed);
            g.steer(Direction::South);
            assert_eq!(g.step(), StepOutcome::Ate);
            let food = g.food().expect("7 cells are still free");
            assert_eq!(food.1, 3, "only row 3 has free cells (seed {seed})");
            assert!(g.body().all(|c| c != food), "food on snake (seed {seed})");
        }
    }

    // --- speed + highscore helpers -------------------------------------------

    #[test]
    fn speed_levels_map_to_descending_intervals() {
        assert_eq!(step_interval_ms(1), STEP_INTERVAL_MS[0]);
        assert_eq!(step_interval_ms(SPEED_LEVELS), STEP_INTERVAL_MS[4]);
        for w in STEP_INTERVAL_MS.windows(2) {
            assert!(w[0] > w[1], "intervals must strictly decrease");
        }
    }

    #[test]
    fn stored_speed_level_is_clamped() {
        assert_eq!(clamp_speed_level(0), 1);
        assert_eq!(clamp_speed_level(3), 3);
        assert_eq!(clamp_speed_level(99), SPEED_LEVELS);
    }

    #[test]
    fn highscore_keys_match_levels() {
        assert_eq!(HIGHSCORE_KEYS.len(), SPEED_LEVELS as usize);
        assert_eq!(HIGHSCORE_KEYS[0], "hs_1");
        assert_eq!(HIGHSCORE_KEYS[4], "hs_5");
    }

    #[test]
    fn xorshift_is_deterministic_and_survives_zero_seed() {
        let mut a = XorShift32::new(1234);
        let mut b = XorShift32::new(1234);
        assert_eq!(a.next(), b.next());
        let mut z = XorShift32::new(0);
        assert_ne!(z.next(), 0, "zero seed must not lock the generator");
        let mut r = XorShift32::new(99);
        for _ in 0..100 {
            assert!(r.below(7) < 7);
        }
    }

    // --- helpers --------------------------------------------------------------

    /// Steer the snake onto the food along a vertical-first taxi path and
    /// eat it. When the greedy direction would be a blocked 180-degree turn,
    /// side-step perpendicular first. Panics if the game dies on the way.
    fn eat_once(g: &mut Game) -> StepOutcome {
        for _ in 0..(4 * W as usize * H as usize) {
            let food = match g.food() {
                Some(f) => f,
                None => panic!("no food on the grid"),
            };
            let (hx, hy) = g.head();
            let want = if food.1 < hy {
                Direction::North
            } else if food.1 > hy {
                Direction::South
            } else if food.0 < hx {
                Direction::West
            } else {
                Direction::East
            };
            let dir = if want == g.direction().opposite() {
                // Side-step towards the grid middle so the detour stays in bounds.
                match want {
                    Direction::East | Direction::West => {
                        if hy > g.height() / 2 {
                            Direction::North
                        } else {
                            Direction::South
                        }
                    }
                    Direction::North | Direction::South => {
                        if hx > g.width() / 2 {
                            Direction::West
                        } else {
                            Direction::East
                        }
                    }
                }
            } else {
                want
            };
            g.steer(dir);
            let outcome = g.step();
            assert_ne!(outcome, StepOutcome::Crashed, "died walking to food");
            if outcome == StepOutcome::Ate {
                return outcome;
            }
        }
        panic!("never reached the food");
    }
}
