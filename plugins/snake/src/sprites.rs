//! \file
//! \brief 1-bpp sprite sheets for the snake, hand-drawn as const arrays.
//!
//! All sheets are packed rows, MSB-first (bit 7 = leftmost pixel), frames
//! stacked vertically - the layout `sprite::Sprite::from_sheet` expects.
//! Set bits paint black; with `FLAG_OPAQUE` the cleared bits paint white
//! (used by the head so its eye is really white on any background).

/// Side length of one grid cell and of the cell-sized sprites, in pixels.
pub const CELL: i16 = 8;

// --- head (8x8, 2 frames: eyes open / blink) --------------------------------
//
// Drawn facing EAST; the render layer rotates/flips it per direction.
// Frame 0: open eye + mouth notch.   Frame 1: closed eye (blink).
pub const HEAD_FRAME_COUNT: u16 = 2;
pub const HEAD_SHEET: [u8; 16] = [
    // frame 0: eye (2x2 white) at the right, mouth notch bottom-right
    0b0111_1110,
    0b1111_1111,
    0b1111_1001,
    0b1111_1001,
    0b1111_1111,
    0b1111_1100,
    0b1111_1111,
    0b0111_1110,
    // frame 1: blink - a one-pixel lid line instead of the open eye
    0b0111_1110,
    0b1111_1111,
    0b1111_1111,
    0b1111_1001,
    0b1111_1111,
    0b1111_1100,
    0b1111_1111,
    0b0111_1110,
];
/// Eyes open for a while, blink briefly.
pub const HEAD_FRAME_MS: [u16; 2] = [1400, 160];

// --- body segment (8x8, 1 frame) ---------------------------------------------
//
// Rounded pill with a checkered "scales" fill; corners stay transparent.
pub const BODY_FRAME_COUNT: u16 = 1;
pub const BODY_SHEET: [u8; 8] = [
    0b0111_1110,
    0b1101_0101,
    0b1010_1011,
    0b1101_0101,
    0b1010_1011,
    0b1101_0101,
    0b1010_1011,
    0b0111_1110,
];

// --- food apple (8x8, 3 frames: small / mid / big pulse) ----------------------
//
// Played ping-pong so the apple breathes: 0 -> 1 -> 2 -> 1 -> 0 ...
pub const FOOD_FRAME_COUNT: u16 = 3;
pub const FOOD_SHEET: [u8; 24] = [
    // frame 0: small berry
    0b0000_0000,
    0b0000_1000,
    0b0000_0000,
    0b0011_1100,
    0b0011_1100,
    0b0011_1100,
    0b0001_1000,
    0b0000_0000,
    // frame 1: mid apple with stem
    0b0000_1000,
    0b0000_1000,
    0b0011_1100,
    0b0111_1110,
    0b0111_1110,
    0b0111_1110,
    0b0011_1100,
    0b0001_1000,
    // frame 2: big apple with leaf
    0b0000_1000,
    0b0001_1100,
    0b0111_1110,
    0b1111_1111,
    0b1111_1111,
    0b1111_1111,
    0b0111_1110,
    0b0011_1100,
];

// --- crash explosion (16x16, 4 frames, played once) ---------------------------
//
// A burst that grows from a spark to a ring and falls apart into debris.
pub const BOOM_FRAME_COUNT: u16 = 4;
pub const BOOM_SHEET: [u8; 128] = [
    // frame 0: spark
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x01, 0x80, 0x03, 0xC0, 0x03, 0xC0, 0x01, 0x80,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    // frame 1: star burst
    0x00, 0x00, 0x01, 0x80, 0x01, 0x80, 0x00, 0x00,
    0x04, 0x20, 0x02, 0x40, 0x0F, 0xF0, 0x1F, 0xF8,
    0x1F, 0xF8, 0x0F, 0xF0, 0x02, 0x40, 0x04, 0x20,
    0x00, 0x00, 0x01, 0x80, 0x01, 0x80, 0x00, 0x00,
    // frame 2: ring with spokes
    0x01, 0x80, 0x01, 0x80, 0x0C, 0x30, 0x1E, 0x78,
    0x30, 0x0C, 0x60, 0x06, 0xC1, 0x83, 0xC1, 0x83,
    0xC1, 0x83, 0xC1, 0x83, 0x60, 0x06, 0x30, 0x0C,
    0x1E, 0x78, 0x0C, 0x30, 0x01, 0x80, 0x01, 0x80,
    // frame 3: falling debris
    0x80, 0x01, 0x20, 0x08, 0x00, 0x00, 0x08, 0x20,
    0x00, 0x00, 0x40, 0x02, 0x00, 0x00, 0x02, 0x00,
    0x00, 0x40, 0x00, 0x00, 0x10, 0x04, 0x00, 0x00,
    0x04, 0x10, 0x00, 0x00, 0x40, 0x02, 0x80, 0x01,
];
/// Explosion frame stepping (ms per frame while playing once).
pub const BOOM_FRAME_MS: u16 = 220;
/// Explosion sprite side length in pixels.
pub const BOOM_SIZE: i16 = 16;
