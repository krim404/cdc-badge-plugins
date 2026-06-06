//! \file
//! \brief Routes `getrandom` (used by vodozemac) to the host hardware RNG.
//!
//! `wasm32-unknown-unknown` has no default entropy source, so getrandom's
//! `custom` backend is registered here and forwarded to `random::fill`. Only
//! compiled for wasm; native test builds use getrandom's OS backend.

use core::num::NonZeroU32;
use getrandom::{register_custom_getrandom, Error};

const HOST_RNG_FAIL: u32 = Error::CUSTOM_START + 1;

fn host_getrandom(buf: &mut [u8]) -> Result<(), Error> {
    cdc_badge_plugin::random::fill(buf)
        .map_err(|_| Error::from(NonZeroU32::new(HOST_RNG_FAIL).unwrap()))
}

register_custom_getrandom!(host_getrandom);
