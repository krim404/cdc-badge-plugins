//! Print-job wire format v1 — the transport framing used by both the network
//! listener and the serial `PLUGIN CMD` channel. The payload type selects how
//! the badge renders it; the badge always does the rendering/scaling.
//!
//! ```text
//! "PJ" u16 LE magic 0x4A50 | u8 type | u8 flags | u32 LE len | payload[len]
//! type 0 = text/plain UTF-8   (badge renders with fonts)
//! type 1 = image PNG/JPEG     (badge scales+dithers to 384px)
//! type 2 = raster (TP job body, already 384px 1bpp)
//! ```

#![allow(dead_code)]

/// Magic "PJ" as a little-endian u16 ('P'=0x50, 'J'=0x4A).
pub const MAGIC: u16 = 0x4A50;
/// Fixed frame header size: magic + type + flags + payload length.
pub const PJ_HEADER_LEN: usize = 8;

pub const TYPE_TEXT: u8 = 0;
pub const TYPE_IMAGE: u8 = 1;
pub const TYPE_RASTER: u8 = 2;

/// A parsed print job referencing the payload bytes.
pub struct Job<'a> {
    pub kind: u8,
    pub flags: u8,
    pub payload: &'a [u8],
}

/// Parse a PJ frame. Returns `None` on any malformation or truncation.
pub fn parse(buf: &[u8]) -> Option<Job<'_>> {
    if buf.len() < PJ_HEADER_LEN {
        return None;
    }
    let magic = u16::from_le_bytes([buf[0], buf[1]]);
    if magic != MAGIC {
        return None;
    }
    let kind = buf[2];
    let flags = buf[3];
    let len = u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]) as usize;
    // checked_add: an attacker-controlled len near u32::MAX would overflow
    // usize on wasm32 (and panic in builds with overflow checks).
    let end = PJ_HEADER_LEN.checked_add(len)?;
    let payload = buf.get(PJ_HEADER_LEN..end)?;
    Some(Job {
        kind,
        flags,
        payload,
    })
}

/// Build a PJ frame around a payload (used by tests and, symmetrically, the
/// Python client).
#[cfg(test)]
pub fn build(kind: u8, payload: &[u8]) -> alloc::vec::Vec<u8> {
    let mut out = alloc::vec::Vec::with_capacity(PJ_HEADER_LEN + payload.len());
    out.extend_from_slice(&MAGIC.to_le_bytes());
    out.push(kind);
    out.push(0);
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(payload);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_text() {
        let f = build(TYPE_TEXT, b"hello");
        let j = parse(&f).unwrap();
        assert_eq!(j.kind, TYPE_TEXT);
        assert_eq!(j.payload, b"hello");
    }

    #[test]
    fn magic_bytes_are_pj() {
        let f = build(TYPE_IMAGE, &[1, 2, 3]);
        assert_eq!(&f[0..2], b"PJ");
        assert_eq!(parse(&f).unwrap().kind, TYPE_IMAGE);
    }

    #[test]
    fn rejects_bad_magic() {
        let mut f = build(TYPE_TEXT, b"x");
        f[0] = 0;
        assert!(parse(&f).is_none());
    }

    #[test]
    fn rejects_truncated() {
        let f = build(TYPE_RASTER, &[0u8; 10]);
        assert!(parse(&f[..f.len() - 1]).is_none());
        assert!(parse(&f[..4]).is_none());
    }

    #[test]
    fn empty_payload_ok() {
        let f = build(TYPE_TEXT, b"");
        assert_eq!(parse(&f).unwrap().payload.len(), 0);
    }

    #[test]
    fn rejects_overflowing_len() {
        // len = u32::MAX must not overflow the header+len bounds check.
        let mut f = build(TYPE_TEXT, b"x");
        f[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(parse(&f).is_none());
    }
}
