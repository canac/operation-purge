//! Per-sound voice pools with the original's exact steal/drop semantics
//! (`cOperationPurge.cpp:1908-2005`, `Sound.cpp:180-200`).
//!
//! PARITY:
//! - Non-3D `GetFreeBuffer` (`Sound.cpp:180-200`): first idle voice, else
//!   steal a **random** busy one (`m_apDSBuffer[rand() % n]`). Never drops.
//! - 3D `Play3DSound` (`cOperationPurge.cpp:1908-2005`): first idle voice
//!   wins immediately (`break`); otherwise find the farthest-from-listener
//!   *playing* voice, seeding the comparison distance with the **new
//!   sound's own distance** (`fDistSqrFarthest = Distance3DSqr(listener,
//!   origin)`) — steal only if a playing voice is **strictly** farther;
//!   if the new sound is farthest, it is silently **dropped** (never
//!   played). This is the exact "drop-if-new-sound-is-farthest" rule.

/// One voice slot's *selection-relevant* state: whether it is currently
/// playing, and (for 3D voices) the distance from the listener's current
/// position to where its sound was emitted (`GetPosition` then
/// `Distance3DSqr`, evaluated at decision time).
#[derive(Copy, Clone, Debug)]
pub struct Voice {
    pub playing: bool,
    /// Only meaningful for 3D voices while `playing`.
    pub distance_from_listener: f32,
}

impl Default for Voice {
    fn default() -> Self {
        Self {
            playing: false,
            distance_from_listener: 0.0,
        }
    }
}

/// A pool of `nSimultaneousInstances` voices for one sound clip.
pub struct VoicePool {
    pub voices: Vec<Voice>,
}

impl VoicePool {
    /// A pool of `n` idle voices.
    pub fn new(n: usize) -> Self {
        Self {
            voices: vec![Voice::default(); n],
        }
    }

    /// Non-3D selection (`cSound::GetFreeBuffer`, `Sound.cpp:180-200`):
    /// return the first idle voice; if every voice is playing, steal a
    /// random busy one. `pick_random(n)` supplies `rand() % n` for the
    /// all-busy case (kept as a caller-provided closure so the choice is
    /// deterministic and testable). Returns `None` only for an empty pool
    /// (the original's `m_apDSBuffer == NULL` guard).
    pub fn get_free_buffer(&self, pick_random: impl FnOnce(usize) -> usize) -> Option<usize> {
        let n = self.voices.len();
        if n == 0 {
            return None;
        }
        if let Some(idx) = self.voices.iter().position(|v| !v.playing) {
            // First idle voice (the `if (i != m_dwNumBuffers)` branch).
            return Some(idx);
        }
        // All playing => random steal (`m_apDSBuffer[rand() % n]`).
        Some(pick_random(n).min(n - 1))
    }

    /// 3D selection (`cOperationPurge::Play3DSound`,
    /// `cOperationPurge.cpp:1908-2005`). `new_distance` is the new sound's
    /// own distance from the listener (`Distance3D`), used to seed the
    /// farthest comparison.
    ///
    /// - First idle voice wins immediately (`break`) — returns `Some(idx)`.
    /// - Otherwise, among the *playing* voices, find the one strictly
    ///   farther from the listener than every prior candidate, seeded with
    ///   the new sound's distance. Steal it (`Some(idx)`).
    /// - If no playing voice is farther than the new sound (the new sound
    ///   is the farthest), it is **dropped** — returns `None`. Do NOT
    ///   "helpfully" evict a nearer voice instead (PARITY).
    ///
    /// The comparison is on squared distances, matching `Distance3DSqr`.
    pub fn play_3d(&self, new_distance: f32) -> Option<usize> {
        if self.voices.is_empty() {
            return None;
        }
        // fDistSqrFarthest = Distance3DSqr(listener, origin) — the new sound.
        let mut farthest_sqr = new_distance * new_distance;
        let mut chosen: Option<usize> = None;
        for (i, v) in self.voices.iter().enumerate() {
            if !v.playing {
                // The buffer is not playing so choose it and break.
                return Some(i);
            }
            let d_sqr = v.distance_from_listener * v.distance_from_listener;
            // Strictly farther => this playing voice becomes the steal target.
            if d_sqr > farthest_sqr {
                farthest_sqr = d_sqr;
                chosen = Some(i);
            }
        }
        // chosen == None => pDSBufferFarthest == NULL => new sound dropped.
        chosen
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pool(states: &[(bool, f32)]) -> VoicePool {
        VoicePool {
            voices: states
                .iter()
                .map(|&(playing, distance_from_listener)| Voice {
                    playing,
                    distance_from_listener,
                })
                .collect(),
        }
    }

    // ---------------- non-3D GetFreeBuffer ----------------

    #[test]
    fn non_3d_returns_first_idle_voice() {
        let p = pool(&[(true, 0.0), (false, 0.0), (false, 0.0)]);
        // pick_random must NOT be consulted when an idle voice exists.
        assert_eq!(p.get_free_buffer(|_| panic!("should not steal")), Some(1));
    }

    #[test]
    fn non_3d_steals_random_busy_when_all_playing() {
        let p = pool(&[(true, 0.0), (true, 0.0), (true, 0.0), (true, 0.0)]);
        // > N shots => all voices busy => random steal at the supplied index.
        assert_eq!(p.get_free_buffer(|n| 2 % n), Some(2));
        assert_eq!(p.get_free_buffer(|_| 0), Some(0));
        // Out-of-range random index is clamped into the pool.
        assert_eq!(p.get_free_buffer(|n| n + 5), Some(3));
    }

    #[test]
    fn non_3d_empty_pool_is_none() {
        let p = VoicePool::new(0);
        assert_eq!(p.get_free_buffer(|_| 0), None);
    }

    // ---------------- 3D Play3DSound ----------------

    #[test]
    fn threed_first_idle_wins_immediately() {
        // Idle at index 1; the new sound is very close. First idle wins.
        let p = pool(&[(true, 5.0), (false, 0.0), (true, 200.0)]);
        assert_eq!(p.play_3d(1.0), Some(1));
    }

    #[test]
    fn threed_steals_farthest_playing_voice_when_farther_than_new() {
        // All playing at 20/80/40; new sound at 10. Farthest playing = index 1 (80).
        let p = pool(&[(true, 20.0), (true, 80.0), (true, 40.0)]);
        assert_eq!(p.play_3d(10.0), Some(1));
    }

    #[test]
    fn threed_drops_when_new_sound_is_the_farthest() {
        let p = pool(&[(true, 20.0), (true, 30.0), (true, 40.0)]);
        assert_eq!(p.play_3d(100.0), None);
    }

    #[test]
    fn threed_drop_at_exact_tie_is_strict() {
        // A playing voice exactly at the new sound's distance is NOT strictly
        // farther => no steal => drop (the compare is `> fDistSqrFarthest`).
        let p = pool(&[(true, 50.0), (true, 50.0)]);
        assert_eq!(p.play_3d(50.0), None);
    }

    #[test]
    fn threed_prefers_idle_over_stealing_even_if_a_playing_voice_is_farther() {
        let p = pool(&[(true, 999.0), (false, 0.0)]);
        assert_eq!(p.play_3d(1.0), Some(1));
    }

    #[test]
    fn threed_empty_pool_is_none() {
        let p = VoicePool::new(0);
        assert_eq!(p.play_3d(1.0), None);
    }
}
