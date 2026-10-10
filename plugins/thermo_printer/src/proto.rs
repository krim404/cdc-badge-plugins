//! Cat-printer BLE protocol: frame format, CRC8, row encoding, status
//! notifications and the command sequence for one print job.
//!
//! Clean-room implementation. The validated command subset follows
//! `printable` (cicloid/printable, docs/PROTOCOL.md "X6" section) and
//! NaitLee/Cat-Printer served as the UUID reference. Frame format:
//! `51 78 <cmd> 00 <len_lo> <len_hi> <payload> <crc8(payload)> ff`.
//! Printer rows are LSB-first within each byte (bit 0 = leftmost pixel), the
//! opposite of the badge raster layout, so row encoding reverses each byte.

#![allow(dead_code)]

use alloc::vec::Vec;

/// GATT service UUID of the cat printer family (16-bit 0xAE30).
pub const SERVICE_UUID: &str = "0000ae30-0000-1000-8000-00805f9b34fb";
/// TX characteristic (badge -> printer, write without response).
pub const TX_UUID: &str = "0000ae01-0000-1000-8000-00805f9b34fb";
/// RX characteristic (printer -> badge, notify).
pub const RX_UUID: &str = "0000ae02-0000-1000-8000-00805f9b34fb";

/// Service UUID in the 16-byte little-endian layout the BLE host API uses.
pub const SERVICE_UUID_LE: [u8; 16] = [
    0xfb, 0x34, 0x9b, 0x5f, 0x80, 0x00, 0x00, 0x80, 0x00, 0x10, 0x00, 0x00, 0x30, 0xae, 0x00, 0x00,
];
/// TX characteristic UUID (little-endian).
pub const TX_UUID_LE: [u8; 16] = [
    0xfb, 0x34, 0x9b, 0x5f, 0x80, 0x00, 0x00, 0x80, 0x00, 0x10, 0x00, 0x00, 0x01, 0xae, 0x00, 0x00,
];
/// RX characteristic UUID (little-endian).
pub const RX_UUID_LE: [u8; 16] = [
    0xfb, 0x34, 0x9b, 0x5f, 0x80, 0x00, 0x00, 0x80, 0x00, 0x10, 0x00, 0x00, 0x02, 0xae, 0x00, 0x00,
];

/// Scan-name prefixes of the supported printer models (cat family). The
/// `LX` family speaks a different protocol and is deliberately absent.
pub const MODELS: [&str; 13] = [
    "GB01", "GB02", "GB03", "GT01", "YT01", "MX05", "MX06", "MX08", "MX10", "MXTP", "X6", "x6",
    "SC05",
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

pub const CMD_FEED_PAPER: u8 = 0xA1;
pub const CMD_DRAW_BITMAP: u8 = 0xA2;
pub const CMD_DEVICE_STATUS: u8 = 0xAE;
pub const CMD_SET_ENERGY: u8 = 0xAF;
pub const CMD_SET_SPEED: u8 = 0xBD;
pub const CMD_APPLY_ENERGY: u8 = 0xBE;
pub const CMD_DRAW_BITMAP_RLE: u8 = 0xBF;

/// Default thermal energy (kitty-printer "medium" preset; tunable in Settings).
pub const DEFAULT_ENERGY: u16 = 24000;
/// Lowest / highest energy validated by the reference implementation.
pub const MIN_ENERGY: u16 = 12000;
pub const MAX_ENERGY: u16 = 48000;
/// Default feed-speed divisor (smaller is faster and lighter).
pub const DEFAULT_SPEED: u8 = 16;
/// Fastest / slowest speed divisor validated by the reference implementation.
pub const MIN_SPEED: u8 = 8;
pub const MAX_SPEED: u8 = 32;
/// Default feed after the image, in printer steps.
pub const DEFAULT_FEED_STEPS: u16 = 0x0070;

/// Printer flow-control status carried by a 0xAE notification.
#[derive(Debug, PartialEq, Eq)]
pub enum Status {
    /// Receive buffer full: stop sending rows until `Ready`.
    BufferFull,
    /// Sending may resume.
    Ready,
}

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

/// Decode a printer status notification (`51 78 AE 01 01 00 <st> <crc> FF`).
/// Anything else (battery, device info, prefixed variants) yields `None`.
pub fn parse_status(data: &[u8]) -> Option<Status> {
    if data.len() != 9
        || data[0] != 0x51
        || data[1] != 0x78
        || data[2] != CMD_DEVICE_STATUS
        || data[3] != 0x01
        || data[4] != 0x01
        || data[5] != 0x00
        || data[8] != 0xFF
    {
        return None;
    }
    if crc8(&data[6..7]) != data[7] {
        return None;
    }
    match data[6] {
        0x10 => Some(Status::BufferFull),
        0x00 => Some(Status::Ready),
        _ => None,
    }
}

/// Reverse the bit order of one byte (badge MSB-first -> printer LSB-first).
pub fn reverse_bits(mut b: u8) -> u8 {
    b = (b & 0xF0) >> 4 | (b & 0x0F) << 4;
    b = (b & 0xCC) >> 2 | (b & 0x33) << 2;
    b = (b & 0xAA) >> 1 | (b & 0x55) << 1;
    b
}

/// Encode a badge raster row (MSB-first) as an uncompressed 0xA2 frame.
/// Rows narrower than [`ROW_BYTES`] are padded with white on the right.
pub fn frame_row_plain(row: &[u8]) -> Vec<u8> {
    let mut printer_row = Vec::with_capacity(ROW_BYTES);
    for &b in row.iter().take(ROW_BYTES) {
        printer_row.push(reverse_bits(b));
    }
    printer_row.resize(ROW_BYTES, 0);
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

/// Encode a raster row: plain 0xA2, or the smaller of RLE (0xBF) and plain
/// when `rle` is enabled. RLE rows are only emitted for full-width rows.
pub fn frame_row(row: &[u8], rle: bool) -> Vec<u8> {
    if rle && row.len() == ROW_BYTES {
        if let Some(runs) = rle_encode_row(row) {
            return frame(CMD_DRAW_BITMAP_RLE, &runs);
        }
    }
    frame_row_plain(row)
}

/// Frames that open a job: speed, energy, apply, one blank lead row.
pub fn job_setup(energy: u16, speed: u8) -> Vec<u8> {
    let mut out = Vec::with_capacity(ROW_BYTES + 48);
    out.extend_from_slice(&frame(CMD_SET_SPEED, &[speed]));
    out.extend_from_slice(&frame(CMD_SET_ENERGY, &energy.to_le_bytes()));
    out.extend_from_slice(&frame(CMD_APPLY_ENERGY, &[0x01]));
    out.extend_from_slice(&frame_row_plain(&[0u8; ROW_BYTES]));
    out
}

/// Frames for rows `from..to` of a packed raster (`stride` bytes per row,
/// MSB-first, set bit = black); only the first ROW_BYTES of a row are printed.
pub fn job_rows(bits: &[u8], stride: usize, from: usize, to: usize, rle: bool) -> Vec<u8> {
    let mut out = Vec::with_capacity(to.saturating_sub(from) * (ROW_BYTES + 8));
    for y in from..to {
        let row = &bits[y * stride..y * stride + ROW_BYTES.min(stride)];
        out.extend_from_slice(&frame_row(row, rle));
    }
    out
}

/// Frames that close a job: the paper feed (nothing when `feed_steps` is 0).
pub fn job_finish(feed_steps: u16) -> Vec<u8> {
    if feed_steps > 0 {
        frame(CMD_FEED_PAPER, &feed_steps.to_le_bytes())
    } else {
        Vec::new()
    }
}

/// Build the full byte stream for one print job from a packed raster. The
/// sequence mirrors the reference implementation: speed, energy, apply, one
/// blank lead row, the rows, then the paper feed.
pub fn build_job(
    bits: &[u8],
    stride: usize,
    height: usize,
    energy: u16,
    speed: u8,
    feed_steps: u16,
    rle: bool,
) -> Vec<u8> {
    let mut out = job_setup(energy, speed);
    out.extend_from_slice(&job_rows(bits, stride, 0, height, rle));
    out.extend_from_slice(&job_finish(feed_steps));
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
    fn setup_frames_match_reference_vectors() {
        // Vectors from printable's packets.rs tests.
        assert_eq!(
            frame(CMD_SET_SPEED, &[16]),
            [0x51, 0x78, 0xBD, 0x00, 0x01, 0x00, 0x10, 0x70, 0xFF]
        );
        assert_eq!(
            frame(CMD_SET_ENERGY, &12000u16.to_le_bytes()),
            [0x51, 0x78, 0xAF, 0x00, 0x02, 0x00, 0xE0, 0x2E, 0x89, 0xFF]
        );
        assert_eq!(
            frame(CMD_APPLY_ENERGY, &[0x01]),
            [0x51, 0x78, 0xBE, 0x00, 0x01, 0x00, 0x01, 0x07, 0xFF]
        );
    }

    #[test]
    fn status_notifications() {
        assert_eq!(
            parse_status(&[0x51, 0x78, 0xAE, 0x01, 0x01, 0x00, 0x10, 0x70, 0xFF]),
            Some(Status::BufferFull)
        );
        assert_eq!(
            parse_status(&[0x51, 0x78, 0xAE, 0x01, 0x01, 0x00, 0x00, 0x00, 0xFF]),
            Some(Status::Ready)
        );
        // Wrong direction byte, bad CRC, unknown payload, wrong length.
        assert_eq!(
            parse_status(&[0x51, 0x78, 0xAE, 0x00, 0x01, 0x00, 0x10, 0x70, 0xFF]),
            None
        );
        assert_eq!(
            parse_status(&[0x51, 0x78, 0xAE, 0x01, 0x01, 0x00, 0x10, 0x00, 0xFF]),
            None
        );
        assert_eq!(
            parse_status(&[0x51, 0x78, 0xAE, 0x01, 0x01, 0x00, 0x20, 0xE0, 0xFF]),
            None
        );
        assert_eq!(parse_status(&[0x51, 0x78, 0xAE, 0x01]), None);
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
    fn narrow_rows_are_padded_to_full_width() {
        let f = frame_row_plain(&[0xFF; 13]);
        let len = f[4] as usize | ((f[5] as usize) << 8);
        assert_eq!(len, ROW_BYTES);
        assert_eq!(f.len(), 8 + ROW_BYTES);
        assert_eq!(&f[6..19], &[0xFF; 13]);
        assert_eq!(&f[19..6 + ROW_BYTES], &[0u8; ROW_BYTES - 13]);
        // Narrow rows never go out as RLE even when enabled.
        assert_eq!(frame_row(&[0u8; 13], true)[2], CMD_DRAW_BITMAP);
    }

    #[test]
    fn rle_all_white_row() {
        let row = [0u8; ROW_BYTES];
        let rle = rle_encode_row(&row).expect("all-white must compress");
        // 384 white pixels = 127 + 127 + 127 + 3.
        assert_eq!(rle, alloc::vec![127, 127, 127, 3]);
        assert_eq!(frame_row(&row, true)[2], CMD_DRAW_BITMAP_RLE);
        assert_eq!(frame_row(&row, false)[2], CMD_DRAW_BITMAP);
    }

    #[test]
    fn rle_alternating_bytes_fall_back() {
        let row = [0xAAu8; ROW_BYTES];
        assert!(
            rle_encode_row(&row).is_none(),
            "1-px runs must not compress"
        );
        let f = frame_row(&row, true);
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

    /// Split a job stream into its command bytes, checking the framing.
    fn commands(job: &[u8]) -> Vec<u8> {
        let mut cmds = Vec::new();
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
        cmds
    }

    #[test]
    fn job_stream_sequence() {
        let bits = [0u8; ROW_BYTES * 2];
        let job = build_job(
            &bits,
            ROW_BYTES,
            2,
            DEFAULT_ENERGY,
            DEFAULT_SPEED,
            0x0070,
            false,
        );
        assert_eq!(
            commands(&job),
            [
                CMD_SET_SPEED,
                CMD_SET_ENERGY,
                CMD_APPLY_ENERGY,
                CMD_DRAW_BITMAP, // blank lead row
                CMD_DRAW_BITMAP,
                CMD_DRAW_BITMAP,
                CMD_FEED_PAPER,
            ]
        );
        // Speed and energy payloads land where the printer expects them.
        assert_eq!(job[6], DEFAULT_SPEED);
        assert_eq!(&job[9 + 6..9 + 8], &DEFAULT_ENERGY.to_le_bytes());
    }

    #[test]
    fn job_stream_rle_and_zero_feed() {
        let bits = [0u8; ROW_BYTES];
        let job = build_job(&bits, ROW_BYTES, 1, DEFAULT_ENERGY, DEFAULT_SPEED, 0, true);
        assert_eq!(
            commands(&job),
            [
                CMD_SET_SPEED,
                CMD_SET_ENERGY,
                CMD_APPLY_ENERGY,
                CMD_DRAW_BITMAP,
                CMD_DRAW_BITMAP_RLE,
            ]
        );
    }

    #[test]
    fn model_matching() {
        assert!(is_known_model("GB01_1234"));
        assert!(is_known_model("MXTP"));
        assert!(is_known_model("X6h-A1B2"));
        assert!(is_known_model("x6h-A1B2"));
        assert!(is_known_model("SC05-77"));
        assert!(!is_known_model("LX-D02"));
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
