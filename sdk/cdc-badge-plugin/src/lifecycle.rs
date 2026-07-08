//! \file
//! \brief Plugin lifecycle: opt-in background residency.
//!
//! The `background` and `autoload` capabilities only grant permission to stay
//! resident. A plugin must call [`set_resident(true)`](set_resident) to actually
//! remain loaded in the background — typically in `plugin_init` for an autoload
//! service, or while running for a background one. Without it, the plugin is
//! torn down when the user leaves it (and an autoload plugin is unloaded right
//! after boot init). Pass `false` to opt back out.

use crate::{check, Result};
use core::ffi::c_int;

/// \brief Request (`true`) or drop (`false`) background residency.
///
/// Needs the `background` or `autoload` capability to have any effect.
pub fn set_resident(resident: bool) -> Result<()> {
    check(unsafe { host_set_resident(resident as c_int) })
}

#[link(wasm_import_module = "cdc")]
extern "C" {
    fn host_set_resident(resident: c_int) -> c_int;
}
