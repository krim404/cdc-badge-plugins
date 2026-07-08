//! \file
//! \brief Hand-drawn 1-bpp sprite art and fixed-point geometry tables.
//!
//! Rows are u16 (16 px wide art) or u8 (8 px wide art), MSB = leftmost pixel.
//! `pack16` converts to the byte layout `draw_bitmap`/`Sprite::from_sheet`
//! expect.

use alloc::vec::Vec;

/// Pack 16-px-wide rows into two bytes per row, MSB first.
pub fn pack16(rows: &[u16]) -> Vec<u8> {
    let mut out = Vec::with_capacity(rows.len() * 2);
    for &r in rows {
        out.push((r >> 8) as u8);
        out.push((r & 0xFF) as u8);
    }
    out
}

// --- twinkle star: 8x8, 4 frames (dot, plus, star, plus) --------------------

pub const STAR_W: u16 = 8;
pub const STAR_H: u16 = 8;
pub const STAR_FRAMES: u16 = 4;

const STAR_DOT: [u8; 8] = [
    0b0000_0000,
    0b0000_0000,
    0b0000_0000,
    0b0001_1000,
    0b0001_1000,
    0b0000_0000,
    0b0000_0000,
    0b0000_0000,
];

const STAR_PLUS: [u8; 8] = [
    0b0000_0000,
    0b0001_1000,
    0b0001_1000,
    0b0111_1110,
    0b0111_1110,
    0b0001_1000,
    0b0001_1000,
    0b0000_0000,
];

const STAR_BURST: [u8; 8] = [
    0b1001_1001,
    0b0101_1010,
    0b0011_1100,
    0b1111_1111,
    0b1111_1111,
    0b0011_1100,
    0b0101_1010,
    0b1001_1001,
];

/// Full twinkle sheet: dot -> plus -> burst -> plus.
pub fn star_sheet() -> Vec<u8> {
    let mut out = Vec::with_capacity(4 * 8);
    out.extend_from_slice(&STAR_DOT);
    out.extend_from_slice(&STAR_PLUS);
    out.extend_from_slice(&STAR_BURST);
    out.extend_from_slice(&STAR_PLUS);
    out
}

// --- ghost: 16x16, 2 flap frames + full-silhouette mask ---------------------

pub const GHOST_W: u16 = 16;
pub const GHOST_H: u16 = 16;
pub const GHOST_FRAMES: u16 = 2;

/// Data plane: eyes left unset so the mask paints them white.
const GHOST_FRAME_A: [u16; 16] = [
    0b0000011111100000,
    0b0001111111111000,
    0b0011111111111100,
    0b0011100110011100,
    0b0111100110011110,
    0b0111111111111110,
    0b0111111111111110,
    0b0111111111111110,
    0b0111111111111110,
    0b0111111111111110,
    0b0111111111111110,
    0b0111111111111110,
    0b0111111111111110,
    0b0111111111111110,
    0b0110110110110110,
    0b0100100110010010,
];

/// Second flap frame: skirt swings the other way.
const GHOST_FRAME_B: [u16; 16] = [
    0b0000011111100000,
    0b0001111111111000,
    0b0011111111111100,
    0b0011100110011100,
    0b0111100110011110,
    0b0111111111111110,
    0b0111111111111110,
    0b0111111111111110,
    0b0111111111111110,
    0b0111111111111110,
    0b0111111111111110,
    0b0111111111111110,
    0b0111111111111110,
    0b0111111111111110,
    0b0110110110110110,
    0b0010010011001001,
];

const GHOST_SILHOUETTE: [u16; 16] = [
    0b0000011111100000,
    0b0001111111111000,
    0b0011111111111100,
    0b0011111111111100,
    0b0111111111111110,
    0b0111111111111110,
    0b0111111111111110,
    0b0111111111111110,
    0b0111111111111110,
    0b0111111111111110,
    0b0111111111111110,
    0b0111111111111110,
    0b0111111111111110,
    0b0111111111111110,
    0b0110110110110110,
    0b0100100110010010,
];

pub fn ghost_sheet() -> Vec<u8> {
    let mut out = pack16(&GHOST_FRAME_A);
    out.extend_from_slice(&pack16(&GHOST_FRAME_B));
    out
}

/// Per-frame mask sheet (same silhouette for both flap frames).
pub fn ghost_mask() -> Vec<u8> {
    let mut out = pack16(&GHOST_SILHOUETTE);
    out.extend_from_slice(&pack16(&GHOST_SILHOUETTE));
    out
}

// --- landing puff: 16x8, 3 frames (grows and fades outward) -----------------

pub const PUFF_W: u16 = 16;
pub const PUFF_H: u16 = 8;
pub const PUFF_FRAMES: u16 = 3;

const PUFF_1: [u16; 8] = [
    0b0000000000000000,
    0b0000000000000000,
    0b0000001111000000,
    0b0000011111100000,
    0b0000011111100000,
    0b0000001111000000,
    0b0000000000000000,
    0b0000000000000000,
];

const PUFF_2: [u16; 8] = [
    0b0000000000000000,
    0b0000110000110000,
    0b0001100110011000,
    0b0011000110001100,
    0b0011000110001100,
    0b0001100110011000,
    0b0000110000110000,
    0b0000000000000000,
];

const PUFF_3: [u16; 8] = [
    0b0100000110000010,
    0b1000000000000001,
    0b0000010000100000,
    0b0100000000000010,
    0b0100000000000010,
    0b0000010000100000,
    0b1000000000000001,
    0b0100000110000010,
];

pub fn puff_sheet() -> Vec<u8> {
    let mut out = pack16(&PUFF_1);
    out.extend_from_slice(&pack16(&PUFF_2));
    out.extend_from_slice(&pack16(&PUFF_3));
    out
}

// --- small diamond ornament: 8x8 single frame -------------------------------

pub const DIAMOND8: [u8; 8] = [
    0b0001_1000,
    0b0011_1100,
    0b0111_1110,
    0b1111_1111,
    0b1111_1111,
    0b0111_1110,
    0b0011_1100,
    0b0001_1000,
];

// --- orbit table: 12 positions, fixed-point 1.7 (cos, sin) ------------------

/// One revolution in 12 steps of 30 degrees; values are cos/sin * 128.
/// Scale at use site: `offset = (table * radius) >> 7`.
pub const ORBIT_12: [(i16, i16); 12] = [
    (128, 0),
    (111, 64),
    (64, 111),
    (0, 128),
    (-64, 111),
    (-111, 64),
    (-128, 0),
    (-111, -64),
    (-64, -111),
    (0, -128),
    (64, -111),
    (111, -64),
];

/// Scale one orbit entry to pixel offsets for radii (rx, ry).
pub fn orbit_offset(slot: usize, rx: i16, ry: i16) -> (i16, i16) {
    let (c, s) = ORBIT_12[slot % ORBIT_12.len()];
    (
        ((c as i32 * rx as i32) >> 7) as i16,
        ((s as i32 * ry as i32) >> 7) as i16,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack16_layout_msb_first() {
        assert_eq!(pack16(&[0b1000_0000_0000_0001]), alloc::vec![0x80, 0x01]);
    }

    #[test]
    fn sheets_have_expected_sizes() {
        assert_eq!(star_sheet().len(), (STAR_FRAMES * STAR_H) as usize);
        assert_eq!(ghost_sheet().len(), (GHOST_FRAMES * GHOST_H * 2) as usize);
        assert_eq!(ghost_mask().len(), ghost_sheet().len());
        assert_eq!(puff_sheet().len(), (PUFF_FRAMES * PUFF_H * 2) as usize);
    }

    #[test]
    fn orbit_is_point_symmetric() {
        for i in 0..6 {
            let (c, s) = ORBIT_12[i];
            let (c2, s2) = ORBIT_12[i + 6];
            assert_eq!((c, s), (-c2, -s2));
        }
        assert_eq!(orbit_offset(0, 100, 40), (100, 0));
        assert_eq!(orbit_offset(3, 100, 40), (0, 40));
        assert_eq!(orbit_offset(6, 100, 40), (-100, 0));
    }
}
