//! \file
//! \brief Pure text helpers: UTF-8-safe prefixes for the typewriter and the
//!        marquee pacing maths.

/// Number of characters (not bytes) in `s`.
pub fn char_count(s: &str) -> usize {
    s.chars().count()
}

/// Byte length of the first `chars` characters, always on a char boundary.
pub fn prefix_len_bytes(s: &str, chars: usize) -> usize {
    s.char_indices()
        .nth(chars)
        .map(|(idx, _)| idx)
        .unwrap_or(s.len())
}

/// Marquee step interval so the text scrolls `step_px` per frame at a pace
/// the panel can follow; clamped to the e-paper floor.
pub const MARQUEE_STEP_PX: u16 = 8;
pub const MARQUEE_FRAME_MS: u16 = 300;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefix_is_char_safe_for_umlauts() {
        let s = "J\u{f6}rg M\u{fc}\u{df}ig"; // "Jörg Müßig"
        assert_eq!(char_count(s), 10);
        for n in 0..=char_count(s) {
            let len = prefix_len_bytes(s, n);
            assert!(s.is_char_boundary(len), "prefix {} not on boundary", n);
        }
        assert_eq!(&s[..prefix_len_bytes(s, 1)], "J");
        assert_eq!(&s[..prefix_len_bytes(s, 2)], "J\u{f6}");
        assert_eq!(prefix_len_bytes(s, 99), s.len());
    }
}
