//! Shared seeded RNG facade.
//!
//! PARITY: the original seeds a single `srand(GetTickCount())` and calls
//! `rand()` everywhere — maze pick, scatter, damage variance, spawn timing.
//! `RAND_MAX == 32767` on the MSVCRT the game
//! shipped against, so `fRand() = rand() / (RAND_MAX + 1)` has **15-bit**
//! granularity — not the full precision of a modern PRNG's float output.
//! The port keeps one shared seeded `SmallRng` behind `frand()` and
//! reproduces that same 15-bit granularity (no bit-compatibility with msvcrt `rand()` is claimed, only the same
//! statistical coarseness).

use rand::rngs::SmallRng;
use rand::{RngExt, SeedableRng};

pub struct GameRng {
    inner: SmallRng,
}

impl GameRng {
    pub fn from_seed(seed: u64) -> Self {
        Self {
            inner: SmallRng::seed_from_u64(seed),
        }
    }

    /// PARITY: `fRand() = rand() / (RAND_MAX + 1)` — a value in `[0, 1)`
    /// quantized to 1/32768 (15-bit) steps, matching `RAND_MAX == 32767`.
    /// Downstream scatter/AI-timing math depends on this
    /// coarseness, not on `SmallRng`'s native float precision.
    pub fn frand(&mut self) -> f32 {
        const GRANULARITY: u32 = 1 << 15; // RAND_MAX + 1
        let bucket: u32 = self.inner.random_range(0..GRANULARITY);
        bucket as f32 / GRANULARITY as f32
    }

    /// `fRandVariance` (`Functions.cpp:96-99`): uniform in
    /// `median ± variance`. The original divides by `RAND_MAX` (inclusive
    /// upper bound); this reuses [`frand`](Self::frand) and stays half-open.
    pub fn rand_variance(&mut self, median: f32, variance: f32) -> f32 {
        median + self.frand() * variance * 2.0 - variance
    }

    /// `fRandMinMax` (`Functions.cpp:102-105`): uniform in `[min, max)`,
    /// with the same divisor deviation as [`rand_variance`](Self::rand_variance).
    pub fn rand_min_max(&mut self, min: f32, max: f32) -> f32 {
        min + self.frand() * (max - min)
    }

    /// Escape hatch for callers that need a full-precision range/shuffle
    /// (e.g. the maze wildcard matcher's random pick) rather
    /// than the quantized `frand()` facade.
    pub fn inner_mut(&mut self) -> &mut SmallRng {
        &mut self.inner
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frand_is_in_unit_range_and_seeded_reproducibly() {
        let mut a = GameRng::from_seed(42);
        let mut b = GameRng::from_seed(42);
        for _ in 0..64 {
            let (va, vb) = (a.frand(), b.frand());
            assert!((0.0..1.0).contains(&va));
            assert_eq!(va, vb, "same seed must reproduce the same sequence");
        }
    }

    #[test]
    fn frand_has_15_bit_granularity() {
        let mut rng = GameRng::from_seed(7);
        for _ in 0..256 {
            let v = rng.frand();
            let bucket = (v * 32768.0).round();
            assert!((v * 32768.0 - bucket).abs() < 1e-3);
        }
    }
}
