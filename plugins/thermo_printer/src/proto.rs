//! Cat-printer BLE protocol: frame format, CRC8, row encoding and the
//! command sequence for one print job.
//!
//! Clean-room implementation; https://github.com/NaitLee/Cat-Printer served
//! as the behaviour and UUID reference only. Frame format:
//! `51 78 <cmd> 00 <len_lo> <len_hi> <payload> <crc8(payload)> ff`.
//! Printer rows are LSB-first within each byte (bit 0 = leftmost pixel), the
//! opposite of the badge raster layout, so row encoding reverses each byte.

#![allow(dead_code)]

use alloc::vec::Vec;

/// GATT service UUID of the known cat printer models.
pub const SERVICE_UUID: &str = "0000ae00-0000-1000-8000-00805f9b34fb";
/// TX characteristic (badge -> printer, write).
pub const TX_UUID: &str = "0000ae01-0000-1000-8000-00805f9b34fb";
/// RX characteristic (printer -> badge, notify).
pub const RX_UUID: &str = "0000ae02-0000-1000-8000-00805f9b34fb";

/// Service UUID in the 16-byte little-endian layout the BLE host API uses.
pub const SERVICE_UUID_LE: [u8; 16] = [
    0xfb, 0x34, 0x9b, 0x5f, 0x80, 0x00, 0x00, 0x80, 0x00, 0x10, 0x00, 0x00, 0x00, 0xae, 0x00, 0x00,
];
/// TX characteristic UUID (little-endian).
pub const TX_UUID_LE: [u8; 16] = [
    0xfb, 0x34, 0x9b, 0x5f, 0x80, 0x00, 0x00, 0x80, 0x00, 0x10, 0x00, 0x00, 0x01, 0xae, 0x00, 0x00,
];
/// RX characteristic UUID (little-endian).
pub const RX_UUID_LE: [u8; 16] = [
    0xfb, 0x34, 0x9b, 0x5f, 0x80, 0x00, 0x00, 0x80, 0x00, 0x10, 0x00, 0x00, 0x02, 0xae, 0x00, 0x00,
];

/// Scan-name prefixes of the supported printer models.
pub const MODELS: [&str; 10] = [
    "GB01", "GB02", "GB03", "GT01", "YT01", "MX05", "MX06", "MX08", "MX10", "MXTP",
];

/// Print width of every supported model in pixels.
pub const WIDTH_PX: u16 = 384;
/// Packed bytes per printer row.
pub const ROW_BYTES: usize = (WIDTH_PX as usize) / 8;

/// QR pixel scale that fills the printer width: `modules` plus a 2-module
/// quiet zone on each side must fit into [`WIDTH_PX`], clamped to 1..12.
pub fn qr_scale(modules: u16) -> u8 {
    ((WIDTH_PX as u32) / (modules as u32 + 4)).clamp(1, 12) as u8
}

pub const CMD_RETRACT_PAPER: u8 = 0xA0;
pub const CMD_FEED_PAPER: u8 = 0xA1;
pub const CMD_DRAW_BITMAP: u8 = 0xA2;
pub const CMD_GET_DEV_STATE: u8 = 0xA3;
pub const CMD_SET_QUALITY: u8 = 0xA4;
pub const CMD_CONTROL_LATTICE: u8 = 0xA6;
pub const CMD_SET_ENERGY: u8 = 0xAF;
pub const CMD_SET_SPEED: u8 = 0xBD;
pub const CMD_DRAWING_MODE: u8 = 0xBE;
pub const CMD_DRAW_BITMAP_RLE: u8 = 0xBF;

/// Lattice magic that opens a print (from the protocol behaviour reference).
pub const LATTICE_START: [u8; 11] = [
    0xAA, 0x55, 0x17, 0x38, 0x44, 0x5F, 0x5F, 0x5F, 0x44, 0x38, 0x2C,
];
/// Lattice magic that closes a print.
pub const LATTICE_END: [u8; 11] = [
    0xAA, 0x55, 0x17, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x17,
];

/// Default thermal energy (model-dependent; tunable in Settings).
pub const DEFAULT_ENERGY: u16 = 0x2EE0;
/// Default quality byte.
pub const DEFAULT_QUALITY: u8 = 0x33;
/// Default feed after the image, in printer steps.
pub const DEFAULT_FEED_STEPS: u16 = 0x0070;

/// CRC8, polynomial 0x07, init 0x00 (the frame checksum over the payload).
pub fn crc8(data: &[u8]) -> u8 {
    let mut crc: u8 = 0;
    for &b in data {
        crc ^= b;
        for _ in 0..8 {
            crc = if crc & 0x80 != 0 {
                (crc << 1) ^ 0x07
            } else {
                crc << 1
            };
        }
    }
    crc
}

/// Encode one protocol frame for `cmd` with `payload`.
pub fn frame(cmd: u8, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(payload.len() + 8);
    out.push(0x51);
    out.push(0x78);
    out.push(cmd);
    out.push(0x00);
    out.push((payload.len() & 0xFF) as u8);
    out.push((payload.len() >> 8) as u8);
    out.extend_from_slice(payload);
    out.push(crc8(payload));
    out.push(0xFF);
    out
}

/// Reverse the bit order of one byte (badge MSB-first -> printer LSB-first).
pub fn reverse_bits(mut b: u8) -> u8 {
    b = (b & 0xF0) >> 4 | (b & 0x0F) << 4;
    b = (b & 0xCC) >> 2 | (b & 0x33) << 2;
    b = (b & 0xAA) >> 1 | (b & 0x55) << 1;
    b
}

/// Encode a badge raster row (MSB-first) as an uncompressed 0xA2 frame.
pub fn frame_row_plain(row: &[u8]) -> Vec<u8> {
    let mut printer_row = Vec::with_capacity(row.len());
    for &b in row {
        printer_row.push(reverse_bits(b));
    }
    frame(CMD_DRAW_BITMAP, &printer_row)
}

/// RLE-encode a badge raster row into 0xBF run bytes: bit 7 = pixel value,
/// bits 0..6 = run length (max 127 per byte). Returns `None` when the plain
/// encoding is not longer, so the caller falls back to 0xA2.
pub fn rle_encode_row(row: &[u8]) -> Option<Vec<u8>> {
    let width = row.len() * 8;
    let mut runs = Vec::with_capacity(row.len());
    let mut run_val = false;
    let mut run_len: u32 = 0;
    for x in 0..width {
        let black = (row[x >> 3] & (0x80u8 >> (x & 7))) != 0;
        if black == run_val {
            run_len += 1;
        } else {
            push_run(&mut runs, run_val, run_len);
            run_val = black;
            run_len = 1;
        }
    }
    push_run(&mut runs, run_val, run_len);
    if runs.len() < row.len() {
        Some(runs)
    } else {
        None
    }
}

fn push_run(out: &mut Vec<u8>, val: bool, mut len: u32) {
    while len > 0 {
        let chunk = len.min(127) as u8;
        out.push(if val { 0x80 | chunk } else { chunk });
        len -= chunk as u32;
    }
}

/// Encode a raster row as the smaller of RLE (0xBF) and plain (0xA2).
pub fn frame_row(row: &[u8]) -> Vec<u8> {
    match rle_encode_row(row) {
        Some(rle) => frame(CMD_DRAW_BITMAP_RLE, &rle),
        None => frame_row_plain(row),
    }
}

/// Build the full byte stream for one print job from a packed raster.
///
/// `bits` holds `height` rows of `stride` bytes (MSB-first, set bit = black);
/// only the first ROW_BYTES of each row are printed (raster must be 384 px).
pub fn build_job(
    bits: &[u8],
    stride: usize,
    height: usize,
    energy: u16,
    quality: u8,
    feed_steps: u16,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(height * (ROW_BYTES + 8) + 64);
    out.extend_from_slice(&frame(CMD_SET_QUALITY, &[quality]));
    out.extend_from_slice(&frame(CMD_CONTROL_LATTICE, &LATTICE_START));
    out.extend_from_slice(&frame(CMD_SET_ENERGY, &energy.to_le_bytes()));
    out.extend_from_slice(&frame(CMD_DRAWING_MODE, &[0]));
    for y in 0..height {
        let row = &bits[y * stride..y * stride + ROW_BYTES.min(stride)];
        out.extend_from_slice(&frame_row(row));
    }
    out.extend_from_slice(&frame(CMD_FEED_PAPER, &feed_steps.to_le_bytes()));
    out.extend_from_slice(&frame(CMD_CONTROL_LATTICE, &LATTICE_END));
    out.extend_from_slice(&frame(CMD_GET_DEV_STATE, &[0]));
    out
}

/// True when a scan-result name matches a supported printer model.
pub fn is_known_model(name: &str) -> bool {
    MODELS.iter().any(|m| name.starts_with(m))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc8_known_vectors() {
        assert_eq!(crc8(&[]), 0x00);
        assert_eq!(crc8(&[0x00]), 0x00);
        assert_eq!(crc8(&[0x01]), 0x07);
        // CRC8/ATM ("123456789") check value.
        assert_eq!(crc8(b"123456789"), 0xF4);
    }

    #[test]
    fn frame_golden_bytes() {
        let f = frame(CMD_FEED_PAPER, &[0x70, 0x00]);
        assert_eq!(f[..6], [0x51, 0x78, 0xA1, 0x00, 0x02, 0x00]);
        assert_eq!(f[6..8], [0x70, 0x00]);
        assert_eq!(f[8], crc8(&[0x70, 0x00]));
        assert_eq!(f[9], 0xFF);
        assert_eq!(f.len(), 10);
    }

    #[test]
    fn bit_reversal() {
        assert_eq!(reverse_bits(0x80), 0x01);
        assert_eq!(reverse_bits(0x01), 0x80);
        assert_eq!(reverse_bits(0xF0), 0x0F);
        assert_eq!(reverse_bits(0xAA), 0x55);
        assert_eq!(reverse_bits(0x00), 0x00);
        assert_eq!(reverse_bits(0xFF), 0xFF);
    }

    #[test]
    fn rle_all_white_row() {
        let row = [0u8; ROW_BYTES];
        let rle = rle_encode_row(&row).expect("all-white must compress");
        // 384 white pixels = 127 + 127 + 127 + 3.
        assert_eq!(rle, alloc::vec![127, 127, 127, 3]);
    }

    #[test]
    fn rle_alternating_bytes_fall_back() {
        let row = [0xAAu8; ROW_BYTES];
        assert!(
            rle_encode_row(&row).is_none(),
            "1-px runs must not compress"
        );
        let f = frame_row(&row);
        assert_eq!(f[2], CMD_DRAW_BITMAP);
    }

    #[test]
    fn rle_run_split_and_values() {
        let mut row = [0u8; ROW_BYTES];
        row[0] = 0xFF; // 8 black pixels, then white.
        let rle = rle_encode_row(&row).unwrap();
        assert_eq!(rle[0], 0x80 | 8);
        assert_eq!(&rle[1..], &[127, 127, 122][..]); // 376 white
    }

    #[test]
    fn job_stream_shape() {
        let bits = [0u8; ROW_BYTES * 2];
        let job = build_job(&bits, ROW_BYTES, 2, DEFAULT_ENERGY, DEFAULT_QUALITY, 0x0070);
        assert_eq!(job[0], 0x51);
        assert_eq!(job[2], CMD_SET_QUALITY);
        // Every frame starts 0x51 0x78 and ends 0xFF; count command bytes.
        let mut cmds = alloc::vec::Vec::new();
        let mut i = 0;
        while i + 8 <= job.len() {
            assert_eq!(job[i], 0x51);
            assert_eq!(job[i + 1], 0x78);
            let len = job[i + 4] as usize | ((job[i + 5] as usize) << 8);
            cmds.push(job[i + 2]);
            assert_eq!(job[i + 6 + len + 1], 0xFF);
            i += 6 + len + 2;
        }
        assert_eq!(i, job.len());
        assert_eq!(cmds.first(), Some(&CMD_SET_QUALITY));
        assert_eq!(cmds.last(), Some(&CMD_GET_DEV_STATE));
        assert_eq!(
            cmds.iter()
                .filter(|&&c| c == CMD_DRAW_BITMAP_RLE || c == CMD_DRAW_BITMAP)
                .count(),
            2
        );
    }

    #[test]
    fn model_matching() {
        assert!(is_known_model("GB01_1234"));
        assert!(is_known_model("MXTP"));
        assert!(!is_known_model("Fridge"));
        assert!(!is_known_model(""));
    }

    #[test]
    fn qr_scale_fills_print_width() {
        // 25 modules + 4 quiet-zone modules at scale 12 = 348 px <= 384.
        assert_eq!(qr_scale(25), 12);
        // 57 modules: 384 / 61 = 6; scaled QR stays within the raster width.
        assert_eq!(qr_scale(57), 6);
        assert!((qr_scale(57) as u32) * (57 + 4) <= WIDTH_PX as u32);
        // Huge codes never scale below 1.
        assert_eq!(qr_scale(1000), 1);
    }
}
