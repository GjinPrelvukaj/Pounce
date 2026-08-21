//! FNV-1a, hand-rolled and pinned by known-answer tests.
//!
//! Hand-rolled for the same reason `pounce-bench` hand-rolls its PRNG: this
//! value is **persisted** in `.pounce` files, so it must produce the same
//! number in every build forever. `DefaultHasher` is SipHash with no
//! cross-release stability guarantee, which would silently make an old file's
//! `body_hash` incomparable with a new one — and duplicate-content detection
//! that quietly stops matching is worse than none, because it reports success.

const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const PRIME: u64 = 0x0000_0100_0000_01b3;

/// Incremental FNV-1a, so text can be hashed as it streams past without ever
/// being accumulated.
#[derive(Debug, Clone)]
pub struct Fnv1a(u64);

impl Fnv1a {
    pub fn new() -> Self {
        Self(OFFSET)
    }

    pub fn write(&mut self, s: &str) {
        for byte in s.as_bytes() {
            self.0 ^= u64::from(*byte);
            self.0 = self.0.wrapping_mul(PRIME);
        }
    }

    pub fn finish(&self) -> u64 {
        self.0
    }
}

impl Default for Fnv1a {
    fn default() -> Self {
        Self::new()
    }
}

pub fn fnv1a(s: &str) -> u64 {
    let mut h = Fnv1a::new();
    h.write(s);
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_the_published_fnv1a_vectors() {
        // From the FNV reference, verified independently rather than copied.
        // If this fails, treat it as a breaking change to the `.pounce` format,
        // not as an expectation to update — the whole point of the hash is that
        // its value never moves.
        assert_eq!(fnv1a(""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a("a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fnv1a("foobar"), 0x8594_4171_f739_67e8);
    }

    #[test]
    fn streaming_matches_one_shot() {
        // The extractor feeds text in chunks; it must land on the same number
        // as hashing the whole string, or a page's hash would depend on where
        // the parser happened to split it.
        let mut h = Fnv1a::new();
        h.write("foo");
        h.write("bar");
        assert_eq!(h.finish(), fnv1a("foobar"));
    }
}
