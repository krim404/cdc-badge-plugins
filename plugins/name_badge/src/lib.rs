//! \file
//! \brief Animated one-line name badge.
//!
//! Shows a single line of text (the wearer's name, T9-entered, NVS-stored)
//! and rotates through eight e-paper animation effects that mix the host
//! animation framework (element tweens, sprite playback, marquee) with
//! hand-rolled per-tick effects (Bayer dissolves, glitch bursts, orbits).
//!
//! Pure helper modules (`rng`, `dither`, `textmath`, `gfx`) compile on the
//! host for `cargo test -p name_badge`; everything touching the SDK is
//! wasm32-gated.

#![cfg_attr(target_arch = "wasm32", no_std)]

extern crate alloc;

pub mod dither;
pub mod gfx;
pub mod rng;
pub mod textmath;

#[cfg(target_arch = "wasm32")]
mod cell;
#[cfg(target_arch = "wasm32")]
mod engine;
#[cfg(target_arch = "wasm32")]
mod fx;
#[cfg(target_arch = "wasm32")]
mod namefit;
#[cfg(target_arch = "wasm32")]
mod plugin;
