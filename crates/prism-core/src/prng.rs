//! PCG32 (XSH-RR): small, fast, seedable, good enough for fuzzing and
//! jitter. Bit-exact with the reference `pcg32_random_r`.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pcg32 {
    state: u64,
    inc: u64,
}

const MULT: u64 = 6364136223846793005;

impl Default for Pcg32 {
    fn default() -> Self {
        Self::new(0x853c49e6748fea9b)
    }
}

impl Pcg32 {
    /// Seed with the reference default stream.
    pub fn new(seed: u64) -> Self {
        Self::with_stream(seed, 0xda3e39cb94b95bdb)
    }

    /// Seed and stream selector, matching `pcg32_srandom_r(seed, seq)`.
    pub fn with_stream(seed: u64, seq: u64) -> Self {
        let mut r = Self { state: 0, inc: (seq << 1) | 1 };
        r.step();
        r.state = r.state.wrapping_add(seed);
        r.step();
        r
    }

    #[inline]
    fn step(&mut self) {
        self.state = self.state.wrapping_mul(MULT).wrapping_add(self.inc);
    }

    #[inline]
    pub fn next_u32(&mut self) -> u32 {
        let old = self.state;
        self.step();
        let xorshifted = (((old >> 18) ^ old) >> 27) as u32;
        let rot = (old >> 59) as u32;
        xorshifted.rotate_right(rot)
    }

    #[inline]
    pub fn next_u64(&mut self) -> u64 {
        ((self.next_u32() as u64) << 32) | self.next_u32() as u64
    }

    /// Uniform in `[0, 1)` with 53 random bits.
    #[inline]
    pub fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
    }

    /// Uniform in `[0, n)`. Unbiased (rejection sampling). `n` must be `> 0`.
    pub fn below(&mut self, n: u32) -> u32 {
        debug_assert!(n > 0);
        let threshold = n.wrapping_neg() % n;
        loop {
            let r = self.next_u32();
            if r >= threshold {
                return r % n;
            }
        }
    }

    /// Uniform in `lo..hi` (exclusive). `hi` must be `> lo`.
    pub fn range(&mut self, lo: usize, hi: usize) -> usize {
        debug_assert!(hi > lo);
        let span = hi - lo;
        if span <= u32::MAX as usize {
            lo + self.below(span as u32) as usize
        } else {
            lo + (self.next_u64() % span as u64) as usize
        }
    }

    /// Uniform in `[lo, hi)`.
    #[inline]
    pub fn range_f64(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.next_f64()
    }

    /// `true` with probability `p`.
    #[inline]
    pub fn chance(&mut self, p: f64) -> bool {
        self.next_f64() < p
    }

    pub fn choose<'a, T>(&mut self, items: &'a [T]) -> Option<&'a T> {
        if items.is_empty() { None } else { Some(&items[self.range(0, items.len())]) }
    }

    /// Fisher–Yates.
    pub fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            let j = self.range(0, i + 1);
            items.swap(i, j);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_reference_output() {
        // `pcg32-demo` round 1 with `pcg32_srandom_r(&rng, 42u, 54u)`.
        let mut r = Pcg32::with_stream(42, 54);
        let expected = [0xa15c02b7u32, 0x7b47f409, 0xba1d3330, 0x83d2f293, 0xbfa4784b, 0xcbed606e];
        for e in expected {
            assert_eq!(r.next_u32(), e);
        }
    }

    #[test]
    fn deterministic_and_seed_sensitive() {
        let a: Vec<u32> = (0..8).map(|_| 0).scan(Pcg32::new(1), |r, _| Some(r.next_u32())).collect();
        let b: Vec<u32> = (0..8).map(|_| 0).scan(Pcg32::new(1), |r, _| Some(r.next_u32())).collect();
        let c: Vec<u32> = (0..8).map(|_| 0).scan(Pcg32::new(2), |r, _| Some(r.next_u32())).collect();
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn ranges_stay_in_bounds() {
        let mut r = Pcg32::new(7);
        for _ in 0..10_000 {
            let f = r.next_f64();
            assert!((0.0..1.0).contains(&f));
            assert!(r.below(3) < 3);
            let x = r.range(10, 13);
            assert!((10..13).contains(&x));
            let g = r.range_f64(-2.0, 2.0);
            assert!((-2.0..2.0).contains(&g));
        }
        assert_eq!(r.choose::<u8>(&[]), None);
        assert_eq!(r.choose(&[9]), Some(&9));
    }

    #[test]
    fn shuffle_is_a_permutation() {
        let mut r = Pcg32::new(3);
        let mut v: Vec<u32> = (0..50).collect();
        r.shuffle(&mut v);
        let mut sorted = v.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, (0..50).collect::<Vec<_>>());
        assert_ne!(v, sorted, "fifty elements should not come back in order");
    }

    #[test]
    fn roughly_uniform() {
        let mut r = Pcg32::new(11);
        let mut buckets = [0u32; 10];
        for _ in 0..100_000 {
            buckets[r.below(10) as usize] += 1;
        }
        for b in buckets {
            assert!((9_000..11_000).contains(&b), "{buckets:?}");
        }
    }
}
