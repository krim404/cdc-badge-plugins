//! \file
//! \brief Single-threaded static state wrappers.
//!
//! The WASM plugin runtime is single-threaded, so a `Sync` wrapper around
//! `Cell`/`RefCell` is sound. Mirrors the PluginCell pattern from
//! `examples/canvas_demo`.

use core::cell::{Cell, RefCell};
use core::ops::Deref;

pub struct PluginCell<T>(Cell<T>);
unsafe impl<T> Sync for PluginCell<T> {}
impl<T: Copy> PluginCell<T> {
    pub const fn new(v: T) -> Self {
        Self(Cell::new(v))
    }
}
impl<T> Deref for PluginCell<T> {
    type Target = Cell<T>;
    fn deref(&self) -> &Cell<T> {
        &self.0
    }
}

/// RefCell variant for non-Copy state (String, Surface, rasters).
pub struct PluginRef<T>(RefCell<T>);
unsafe impl<T> Sync for PluginRef<T> {}
impl<T> PluginRef<T> {
    pub const fn new(v: T) -> Self {
        Self(RefCell::new(v))
    }
}
impl<T> Deref for PluginRef<T> {
    type Target = RefCell<T>;
    fn deref(&self) -> &RefCell<T> {
        &self.0
    }
}
