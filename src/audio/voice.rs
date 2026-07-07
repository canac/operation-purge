//! Per-sound voice pools with the original's exact steal/drop semantics
//! (design §5.4, verified `cOperationPurge.cpp:1908-2005`). Bodies land
//! at M10.
//!
//! PARITY:
//! - Non-3D `GetFreeBuffer`: first idle voice, else steal a **random**
//!   busy one.
//! - 3D `Play3DSound`: first idle voice wins immediately; otherwise find
//!   the farthest-from-listener *playing* voice, seeding the comparison
//!   distance with the **new sound's own distance** — steal only if a
//!   playing voice is strictly farther; if the new sound is farthest, it
//!   is silently **dropped** (never played). This is the exact
//!   "drop-if-new-sound-is-farthest" rule from design §5.4.

pub struct Voice {
    pub playing: bool,
    /// Only meaningful for 3D voices.
    pub distance_from_listener: f32,
}

/// A pool of `nSimultaneousInstances` voices for one sound clip.
pub struct VoicePool {
    pub voices: Vec<Voice>,
}

impl VoicePool {
    /// Non-3D selection: first idle, else random steal. Body lands at M10.
    pub fn get_free_buffer(&mut self) -> Option<usize> {
        todo!("ported at M10 — GetFreeBuffer (design §5.4)")
    }

    /// 3D selection: first idle wins; else steal the farthest-playing
    /// voice only if it is strictly farther than `new_sound_distance`;
    /// otherwise the new sound is dropped (`None`) — PARITY, do not
    /// "helpfully" always play by evicting the nearest voice instead.
    pub fn play_3d(&mut self, _new_sound_distance: f32) -> Option<usize> {
        todo!("ported at M10 — Play3DSound steal/drop rule (design §5.4)")
    }
}
