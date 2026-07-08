//! \file
//! \brief The eight effects. Each module exposes the same shape:
//!        `enter(ctx, now)` records elements and starts host animations,
//!        `tick(ctx, now)` runs hand-rolled per-step logic (self-throttled),
//!        `on_action(ctx, id)` handles routed completion actions.
//!
//! Effect-local element ids live in 10..25; ids 1/2 are the shared hero name
//! and banner (see `namefit`). `canvas::clear()` on every switch guarantees
//! nothing leaks between effects.

pub mod bounce;
pub mod entrance;
pub mod ghosts;
pub mod glitch;
pub mod orbit;
pub mod sparkle;
pub mod typewriter;
pub mod wipe;
