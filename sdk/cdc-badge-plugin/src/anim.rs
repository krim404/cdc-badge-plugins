//! \file
//! \brief Host-driven tweens on canvas elements.
//!
//! A tween animates an element's replay offset over time with an easing
//! curve; the host advances it on its own clock, commits the canvas and fires
//! `plugin_on_action(done_action_id, handle, elem_id)` on completion - no
//! per-frame plugin code needed. Sequencing: `delay_ms` staggers parallel
//! tweens, [`Anim::after`] chains them, `repeat` plus [`Anim::yoyo`] loops.
//!
//! The e-paper panel caps animation at ~5 fps (see
//! [`crate::canvas::set_anim_policy`]), so think in durations of 500 ms and
//! up.
//!
//! ```ignore
//! let h = anim::Anim::element(ELEM_TITLE)
//!     .to(80, 0)
//!     .duration_ms(800)
//!     .ease(anim::Ease::CubicOut)
//!     .delay_ms(100)
//!     .on_done(ACT_SLIDE_DONE)
//!     .start()?;
//! anim::Anim::element(ELEM_SUB).to(80, 20).duration_ms(800).after(h).start()?;
//! ```

use crate::{check, ffi, Error, Result};

/// `repeat` value meaning "repeat forever".
pub const REPEAT_FOREVER: u16 = ffi::HOST_ANIM_REPEAT_FOREVER as u16;

/// Easing curves (fixed-point Penner subset).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Ease {
    Linear,
    QuadIn,
    QuadOut,
    QuadInOut,
    CubicIn,
    CubicOut,
    CubicInOut,
    /// Back-out: overshoots the target, then settles.
    Overshoot,
    /// Bounce-out: ball-drop rebound.
    Bounce,
    /// Holds the start value, jumps at the end.
    Step,
    /// Elastic-out: springy overshoot wobble.
    Elastic,
}

impl Ease {
    fn raw(self) -> u8 {
        (match self {
            Ease::Linear => ffi::HOST_EASE_LINEAR,
            Ease::QuadIn => ffi::HOST_EASE_QUAD_IN,
            Ease::QuadOut => ffi::HOST_EASE_QUAD_OUT,
            Ease::QuadInOut => ffi::HOST_EASE_QUAD_IN_OUT,
            Ease::CubicIn => ffi::HOST_EASE_CUBIC_IN,
            Ease::CubicOut => ffi::HOST_EASE_CUBIC_OUT,
            Ease::CubicInOut => ffi::HOST_EASE_CUBIC_IN_OUT,
            Ease::Overshoot => ffi::HOST_EASE_OVERSHOOT,
            Ease::Bounce => ffi::HOST_EASE_BOUNCE,
            Ease::Step => ffi::HOST_EASE_STEP,
            Ease::Elastic => ffi::HOST_EASE_ELASTIC,
        }) as u8
    }
}

/// Tween state reported by [`state`].
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// Waiting for its delay or chain predecessor.
    Delayed,
    Running,
    Paused,
    /// Completed or cancelled (the handle is gone).
    Finished,
}

/// Builder for one element-offset tween. Zero-cost wrapper around
/// `host_anim_t`; [`Anim::start`] is the single FFI call.
pub struct Anim {
    cfg: ffi::HostAnim,
}

impl Anim {
    /// \brief Tween the element with id `elem_id`.
    pub fn element(elem_id: u32) -> Anim {
        Anim {
            cfg: ffi::HostAnim {
                elem_id,
                from_x: 0,
                from_y: 0,
                to_x: 0,
                to_y: 0,
                duration_ms: 500,
                delay_ms: 0,
                repeat: 0,
                easing: ffi::HOST_EASE_LINEAR as u8,
                flags: ffi::HOST_ANIM_FLAG_FROM_CURRENT as u8,
                done_action_id: 0,
                start_after: 0,
            },
        }
    }

    /// \brief Explicit start offset (default: the element's live offset).
    pub fn from(mut self, x: i16, y: i16) -> Anim {
        self.cfg.from_x = x;
        self.cfg.from_y = y;
        self.cfg.flags &= !(ffi::HOST_ANIM_FLAG_FROM_CURRENT as u8);
        self
    }

    /// \brief Target offset.
    pub fn to(mut self, x: i16, y: i16) -> Anim {
        self.cfg.to_x = x;
        self.cfg.to_y = y;
        self
    }

    /// \brief Active time per run in ms (default 500).
    pub fn duration_ms(mut self, ms: u16) -> Anim {
        self.cfg.duration_ms = ms;
        self
    }

    /// \brief Wait before the first run.
    pub fn delay_ms(mut self, ms: u16) -> Anim {
        self.cfg.delay_ms = ms;
        self
    }

    /// \brief Extra runs after the first ([`REPEAT_FOREVER`] = endless).
    pub fn repeat(mut self, count: u16) -> Anim {
        self.cfg.repeat = count;
        self
    }

    /// \brief Alternate direction on each repeat run.
    pub fn yoyo(mut self) -> Anim {
        self.cfg.flags |= ffi::HOST_ANIM_FLAG_YOYO as u8;
        self
    }

    /// \brief Hide the element when the tween completes.
    pub fn hide_when_done(mut self) -> Anim {
        self.cfg.flags |= ffi::HOST_ANIM_FLAG_HIDE_DONE as u8;
        self
    }

    /// \brief Show the element when the delay elapses (entrance effects).
    pub fn show_on_start(mut self) -> Anim {
        self.cfg.flags |= ffi::HOST_ANIM_FLAG_SHOW_START as u8;
        self
    }

    /// \brief Easing curve (default linear).
    pub fn ease(mut self, ease: Ease) -> Anim {
        self.cfg.easing = ease.raw();
        self
    }

    /// \brief Fire `plugin_on_action(action_id, handle, elem_id)` when a
    ///        finite tween completes.
    pub fn on_done(mut self, action_id: u32) -> Anim {
        self.cfg.done_action_id = action_id;
        self
    }

    /// \brief Start only after the tween with `handle` completes. Cancelling
    ///        the predecessor cancels this tween too.
    pub fn after(mut self, handle: u32) -> Anim {
        self.cfg.start_after = handle;
        self
    }

    /// \brief Start the tween; returns its handle.
    pub fn start(self) -> Result<u32> {
        let rc = unsafe { ffi::host_anim_start(&self.cfg) };
        if rc <= 0 {
            return Err(Error::from_code(rc));
        }
        Ok(rc as u32)
    }
}

/// \brief Cancel a tween and its chained successors (no completion actions
///        fire); `handle` 0 cancels all of the canvas's tweens.
pub fn cancel(handle: u32) -> Result<()> {
    check(unsafe { ffi::host_anim_cancel(handle) })
}

/// \brief Pause or resume a tween; delay time freezes too.
pub fn pause(handle: u32, paused: bool) -> Result<()> {
    check(unsafe { ffi::host_anim_pause(handle, paused) })
}

/// \brief Query a tween's state.
pub fn state(handle: u32) -> State {
    match unsafe { ffi::host_anim_state(handle) } {
        x if x == ffi::HOST_ANIM_STATE_DELAYED => State::Delayed,
        x if x == ffi::HOST_ANIM_STATE_RUNNING => State::Running,
        x if x == ffi::HOST_ANIM_STATE_PAUSED => State::Paused,
        _ => State::Finished,
    }
}

/// \brief Number of live tweens plus playing sprites.
pub fn active_count() -> u32 {
    let rc = unsafe { ffi::host_anim_active_count() };
    if rc < 0 {
        0
    } else {
        rc as u32
    }
}

/// \brief Blink an element: `count` on/off cycles of `period_ms` per phase
///        ([`REPEAT_FOREVER`] = endless), ending visible. Returns a handle
///        usable with [`cancel`] / [`pause`].
pub fn blink(elem_id: u32, period_ms: u16, count: u16, done_action_id: u32) -> Result<u32> {
    let rc = unsafe { ffi::host_anim_blink(elem_id, period_ms, count, done_action_id) };
    if rc <= 0 {
        return Err(Error::from_code(rc));
    }
    Ok(rc as u32)
}

/// \brief Slide a group of elements by the same delta - the building block
///        for page transitions (wipe a scene out, slide the next one in).
///
/// Starts one tween per element from its current offset; returns the handle
/// of the last one (chain the next scene's entrance with [`Anim::after`] on
/// it).
pub fn slide_group(elem_ids: &[u32], dx: i16, dy: i16, duration_ms: u16,
                   ease: Ease, done_action_id: u32) -> Result<u32> {
    let mut last = Err(crate::Error::NotFound);
    for (i, &id) in elem_ids.iter().enumerate() {
        let mut a = Anim::element(id)
            .to(dx, dy)
            .duration_ms(duration_ms)
            .ease(ease);
        // FROM_CURRENT is the builder default; `to` is relative to the
        // recorded position, so groups recorded at offset 0 stay in step.
        if i == elem_ids.len() - 1 {
            a = a.on_done(done_action_id);
        }
        last = a.start();
    }
    last
}
