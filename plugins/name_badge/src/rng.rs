//! \file
//! \brief Tiny deterministic PRNG for effect placement.
//!
//! XorShift32 is plenty for visual jitter and needs no_std nothing. The
//! plugin seeds it once from the host RNG; the pure core stays host-testable.

pub struct XorShift32 {
    state: u32,
}

impl XorShift32 {
    /// Create a generator; a zero seed is remapped (XorShift is stuck at 0).
    pub const fn new(seed: u32) -> Self {
        Self {
            state: if seed == 0 { 0x9E37_79B9 } else { seed },
        }
    }

    pub fn next_u32(&mut self) -> u32 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.state = x;
        x
    }

    /// Uniform-ish value in `0..bound` (bound 0 yields 0).
    pub fn below(&mut self, bound: u32) -> u32 {
        if bound == 0 {
            return 0;
        }
        self.next_u32() % bound
    }

    /// Value in `min..=max` (inclusive); swapped bounds are tolerated.
    pub fn range_i16(&mut self, min: i16, max: i16) -> i16 {
        let (lo, hi) = if min <= max { (min, max) } else { (max, min) };
        let span = (hi as i32 - lo as i32 + 1) as u32;
        lo.wrapping_add(self.below(span) as i16)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_seed_is_remapped_and_advances() {
        let mut r = XorShift32::new(0);
        let a = r.next_u32();
        let b = r.next_u32();
        assert_ne!(a, 0);
        assert_ne!(a, b);
    }

    #[test]
    fn deterministic_for_same_seed() {
        let mut a = XorShift32::new(1234);
        let mut b = XorShift32::new(1234);
        for _ in 0..16 {
            assert_eq!(a.next_u32(), b.next_u32());
        }
    }

    #[test]
    fn range_respects_bounds() {
        let mut r = XorShift32::new(42);
        for _ in 0..200 {
            let v = r.range_i16(-5, 9);
            assert!((-5..=9).contains(&v));
        }
        assert_eq!(r.below(0), 0);
        // Swapped bounds must not panic and stay inside the same interval.
        let v = r.range_i16(9, -5);
        assert!((-5..=9).contains(&v));
    }
}
