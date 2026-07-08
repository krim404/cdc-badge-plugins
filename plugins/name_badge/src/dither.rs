//! \file
//! \brief Procedural 1-bpp helpers: Bayer reveal masks, raster inversion and
//!        glitch noise bands.
//!
//! Rasters use the canvas bitmap layout: byte-padded rows, MSB first, set
//! bit = black. All functions are pure so they run in host tests.

use crate::rng::XorShift32;
use alloc::vec::Vec;

/// 4x4 ordered-dither matrix; thresholds 0..15.
const BAYER_4X4: [[u8; 4]; 4] = [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]];

/// Reveal levels run 0 (nothing) ..= LEVEL_MAX (full raster).
pub const LEVEL_MAX: u8 = 16;

/// Keep only the pixels whose Bayer threshold is below `level`, producing the
/// classic dissolve: level 0 = all white, LEVEL_MAX = the input raster.
pub fn apply_bayer(raster: &[u8], stride: usize, level: u8) -> Vec<u8> {
    let mut out = Vec::with_capacity(raster.len());
    out.extend_from_slice(raster);
    if level >= LEVEL_MAX {
        return out;
    }
    let rows = raster.len().checked_div(stride).unwrap_or(0);
    for y in 0..rows {
        for x in 0..stride * 8 {
            if BAYER_4X4[y % 4][x % 4] < level {
                continue; // pixel survives
            }
            out[y * stride + x / 8] &= !(0x80 >> (x % 8));
        }
    }
    out
}

/// Flip every pixel (name knocked out of a solid band and back).
pub fn invert(raster: &[u8]) -> Vec<u8> {
    raster.iter().map(|b| !b).collect()
}

/// One horizontal static/noise band of `w` px by `rows` px for the glitch
/// effect; roughly half the pixels set, re-rolled per call.
pub fn noise_band(rng: &mut XorShift32, w: u16, rows: u16) -> Vec<u8> {
    let stride = (w as usize).div_ceil(8);
    let mut out = Vec::with_capacity(stride * rows as usize);
    for _ in 0..stride * rows as usize {
        out.push((rng.next_u32() & 0xFF) as u8);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ones(len: usize) -> Vec<u8> {
        alloc::vec![0xFF; len]
    }

    #[test]
    fn level_zero_is_empty_and_max_is_identity() {
        let src = ones(4 * 8); // 32 px wide, 8 rows
        assert!(apply_bayer(&src, 4, 0).iter().all(|&b| b == 0));
        assert_eq!(apply_bayer(&src, 4, LEVEL_MAX), src);
    }

    #[test]
    fn reveal_is_monotonic() {
        let src = ones(4 * 8);
        let mut prev = 0u32;
        for level in 0..=LEVEL_MAX {
            let set: u32 = apply_bayer(&src, 4, level)
                .iter()
                .map(|b| b.count_ones())
                .sum();
            assert!(set >= prev, "level {} lost pixels", level);
            prev = set;
        }
        assert_eq!(prev, 4 * 8 * 8);
    }

    #[test]
    fn invert_roundtrips() {
        let src = alloc::vec![0xA5, 0x00, 0xFF, 0x3C];
        assert_eq!(invert(&invert(&src)), src);
    }

    #[test]
    fn noise_band_sizes_and_varies() {
        let mut rng = XorShift32::new(7);
        let a = noise_band(&mut rng, 296, 6);
        assert_eq!(a.len(), 37 * 6);
        let b = noise_band(&mut rng, 296, 6);
        assert_ne!(a, b);
    }
}
