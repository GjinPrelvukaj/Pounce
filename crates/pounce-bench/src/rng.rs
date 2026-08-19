//! A seeded SplitMix64 generator.
//!
//! Deliberately hand-rolled rather than pulling in `rand`: fixture
//! reproducibility must not depend on a third-party crate's internal
//! algorithm, which is free to change across major versions. A fixture site
//! that silently reshapes itself on a dependency bump would invalidate every
//! benchmark ever published.

/// Deterministic pseudo-random generator. Not cryptographically secure, and
/// must never be used for anything security-relevant.
#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform-ish value in `0..n`. Returns 0 when `n == 0`.
    ///
    /// Uses modulo, so values below `u64::MAX % n` are very slightly favoured.
    /// The bias is far too small to shift any fixture statistic and buys
    /// simplicity over a rejection loop.
    pub fn below(&mut self, n: u32) -> u32 {
        if n == 0 {
            return 0;
        }
        (self.next_u64() % u64::from(n)) as u32
    }

    /// Returns true with probability `percent / 100`.
    pub fn chance(&mut self, percent: u32) -> bool {
        self.below(100) < percent
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_yields_same_sequence() {
        let mut a = Rng::new(42);
        let mut b = Rng::new(42);
        let seq_a: Vec<u64> = (0..100).map(|_| a.next_u64()).collect();
        let seq_b: Vec<u64> = (0..100).map(|_| b.next_u64()).collect();
        assert_eq!(seq_a, seq_b);
    }

    #[test]
    fn different_seeds_diverge() {
        let mut a = Rng::new(1);
        let mut b = Rng::new(2);
        assert_ne!(a.next_u64(), b.next_u64());
    }

    #[test]
    fn below_stays_in_range() {
        let mut r = Rng::new(7);
        for _ in 0..1000 {
            let v = r.below(10);
            assert!(v < 10, "below(10) returned {v}");
        }
    }

    #[test]
    fn below_one_is_always_zero() {
        let mut r = Rng::new(9);
        assert_eq!(r.below(1), 0);
    }

    #[test]
    fn below_zero_does_not_panic() {
        let mut r = Rng::new(9);
        assert_eq!(r.below(0), 0);
    }

    #[test]
    fn known_vector_is_stable() {
        // Locks the algorithm. If this test ever fails, every previously
        // published benchmark became incomparable — treat as a breaking change.
        let mut r = Rng::new(0);
        assert_eq!(r.next_u64(), 16294208416658607535);
    }

    #[test]
    fn chance_bounds_are_absolute() {
        let mut r = Rng::new(3);
        for _ in 0..200 {
            assert!(!r.chance(0), "chance(0) must never fire");
        }
        let mut r = Rng::new(3);
        for _ in 0..200 {
            assert!(r.chance(100), "chance(100) must always fire");
        }
    }
}
