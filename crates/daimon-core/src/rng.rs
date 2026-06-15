//! A tiny deterministic PRNG (SplitMix64).
//!
//! An autonomous mind is hard enough to debug without non-determinism. Every
//! stochastic choice a Daimon makes — which way to wander, whether to greet a
//! stranger — is driven by one of these, seeded from the agent's identity. Two
//! runs with the same seed produce the same life, byte for byte. That is what
//! makes emergent behaviour *testable* instead of merely anecdotal.

/// SplitMix64 — small, fast, good enough for behavioural jitter (not crypto).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Rng {
    state: u64,
}

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    #[inline]
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform float in `[0, 1)`.
    #[inline]
    pub fn next_f32(&mut self) -> f32 {
        // 24 bits of mantissa precision is plenty for behaviour weighting.
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }

    /// Uniform integer in `[0, n)`. Returns 0 if `n == 0`.
    #[inline]
    pub fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            return 0;
        }
        (self.next_u64() % n as u64) as usize
    }

    /// Returns `true` with probability `p`.
    #[inline]
    pub fn chance(&mut self, p: f32) -> bool {
        self.next_f32() < p
    }

    /// Pick a random element index, weighted by `weights`. Returns `None` if the
    /// slice is empty or all weights are non-positive.
    pub fn weighted(&mut self, weights: &[f32]) -> Option<usize> {
        let total: f32 = weights.iter().filter(|w| **w > 0.0).sum();
        if total <= 0.0 {
            return None;
        }
        let mut pick = self.next_f32() * total;
        for (i, &w) in weights.iter().enumerate() {
            if w <= 0.0 {
                continue;
            }
            pick -= w;
            if pick <= 0.0 {
                return Some(i);
            }
        }
        weights.iter().rposition(|w| *w > 0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_for_seed() {
        let mut a = Rng::new(42);
        let mut b = Rng::new(42);
        for _ in 0..1000 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn floats_in_unit_interval() {
        let mut r = Rng::new(7);
        for _ in 0..10_000 {
            let f = r.next_f32();
            assert!((0.0..1.0).contains(&f));
        }
    }

    #[test]
    fn weighted_respects_zero_weights() {
        let mut r = Rng::new(1);
        // only index 2 has weight; it must always be chosen.
        for _ in 0..100 {
            assert_eq!(r.weighted(&[0.0, 0.0, 1.0, 0.0]), Some(2));
        }
        assert_eq!(r.weighted(&[0.0, 0.0]), None);
        assert_eq!(r.weighted(&[]), None);
    }
}
