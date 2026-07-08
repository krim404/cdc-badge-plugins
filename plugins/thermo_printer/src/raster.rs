//! thermo_print job format v1: the raw-raster payload other plugins hand to
//! this provider via `use_ext_feature("thermo_print", payload)`.
//!
//! ```text
//! offset 0: u16 LE magic 0x5450 ("TP")   2: u8 version = 1   3: u8 flags = 0
//!        4: u16 LE width_px (<= 384)     6: u16 LE height_px
//!        8: u16 LE stride_bytes         10: u16 LE reserved = 0
//!       12: rows[height][stride], MSB-first, set bit = black
//! ```

#![allow(dead_code)]

use alloc::vec::Vec;

/// Job magic ("TP" as little-endian u16).
pub const MAGIC: u16 = 0x5450;
/// Current job format version.
pub const VERSION: u8 = 1;
/// Header size in bytes.
pub const HEADER_LEN: usize = 12;
/// Maximum printable width.
pub const MAX_WIDTH_PX: u16 = 384;

/// A parsed thermo_print job referencing the payload's row data.
pub struct Job<'a> {
    pub width_px: u16,
    pub height_px: u16,
    pub stride_bytes: u16,
    pub rows: &'a [u8],
}

/// Build a job payload from a packed raster.
pub fn build(width_px: u16, height_px: u16, stride_bytes: u16, rows: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(HEADER_LEN + rows.len());
    out.extend_from_slice(&MAGIC.to_le_bytes());
    out.push(VERSION);
    out.push(0);
    out.extend_from_slice(&width_px.to_le_bytes());
    out.extend_from_slice(&height_px.to_le_bytes());
    out.extend_from_slice(&stride_bytes.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(rows);
    out
}

/// Parse and validate a job payload. Returns `None` on any malformation.
pub fn parse(payload: &[u8]) -> Option<Job<'_>> {
    if payload.len() < HEADER_LEN {
        return None;
    }
    let magic = u16::from_le_bytes([payload[0], payload[1]]);
    if magic != MAGIC || payload[2] != VERSION {
        return None;
    }
    let width_px = u16::from_le_bytes([payload[4], payload[5]]);
    let height_px = u16::from_le_bytes([payload[6], payload[7]]);
    let stride_bytes = u16::from_le_bytes([payload[8], payload[9]]);
    if width_px == 0 || width_px > MAX_WIDTH_PX || height_px == 0 {
        return None;
    }
    if (stride_bytes as usize) < (width_px as usize).div_ceil(8) {
        return None;
    }
    let need = stride_bytes as usize * height_px as usize;
    let rows = payload.get(HEADER_LEN..HEADER_LEN + need)?;
    Some(Job {
        width_px,
        height_px,
        stride_bytes,
        rows,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let rows = [0xF0u8; 48 * 3];
        let payload = build(384, 3, 48, &rows);
        let job = parse(&payload).expect("valid payload");
        assert_eq!(job.width_px, 384);
        assert_eq!(job.height_px, 3);
        assert_eq!(job.stride_bytes, 48);
        assert_eq!(job.rows, &rows);
    }

    #[test]
    fn rejects_bad_magic_and_version() {
        let mut p = build(384, 1, 48, &[0u8; 48]);
        p[0] = 0x00;
        assert!(parse(&p).is_none());
        let mut p = build(384, 1, 48, &[0u8; 48]);
        p[2] = 9;
        assert!(parse(&p).is_none());
    }

    #[test]
    fn rejects_bad_geometry() {
        assert!(parse(&build(0, 1, 48, &[0u8; 48])).is_none());
        assert!(parse(&build(400, 1, 50, &[0u8; 50])).is_none());
        assert!(parse(&build(384, 0, 48, &[])).is_none());
        // stride smaller than the packed width
        assert!(parse(&build(384, 1, 40, &[0u8; 40])).is_none());
    }

    #[test]
    fn rejects_truncated_rows() {
        let mut p = build(384, 2, 48, &[0u8; 96]);
        p.truncate(p.len() - 1);
        assert!(parse(&p).is_none());
    }

    #[test]
    fn narrow_raster_ok() {
        let p = build(100, 2, 13, &[0u8; 26]);
        let job = parse(&p).expect("narrow raster valid");
        assert_eq!(job.width_px, 100);
        assert_eq!(job.stride_bytes, 13);
    }
}
