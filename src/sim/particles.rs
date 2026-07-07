//! Three-tier particle system: `cParticle` / `cParticleEmitter` /
//! `cParticleEffect` (source-map §3.6). Full body lands at M9.
//!
//! PARITY (design §1.3, preserved verbatim, bodies land at M9):
//! - Fixed 1024-slot arena, linear replace-oldest-by-life-fraction scan
//!   (`s_MaximumParticles`, source-map §7).
//! - Sub-frame spawn interpolation + pre-aging (required for
//!   distance-based projectile trails).
//! - Emitter shortest-arc quaternion ported verbatim, including
//!   degenerate cases (zero velocity, anti-parallel) — NOT glam's
//!   `from_rotation_arc`, which handles the anti-parallel case
//!   differently (source-map §3.6, §8.5).
//! - Static effects prune expired emitters; **dynamic effects never do**
//!   (owner must delete) — do not "fix" this into always-pruning.

use crate::ids::{EmitterTypeId, ParticleTypeId};
use glam::Vec3;

/// `s_MaximumParticles` (`cOperationPurge.cpp:18`).
pub const MAX_PARTICLES: usize = 1024;

/// `s_MaximumHoles` (`cOperationPurge.cpp:19`).
pub const MAX_HOLES: usize = 256;

pub struct Particle {
    pub particle_type: ParticleTypeId,
    pub position: Vec3,
    pub velocity: Vec3,
    pub age: f32,
    pub lifetime: f32,
}

/// Fixed-size arena with the original's linear replace-oldest-by-life-
/// fraction allocation scan (PARITY — not a ring buffer/free-list, which
/// would change which particle gets evicted under pressure).
pub struct ParticleArena {
    pub slots: Vec<Option<Particle>>,
}

impl ParticleArena {
    pub fn new() -> Self {
        Self {
            slots: (0..MAX_PARTICLES).map(|_| None).collect(),
        }
    }

    /// Finds a free slot, or the slot whose particle has the highest
    /// life-fraction (closest to death) to evict. Body lands at M9.
    pub fn allocate(&mut self, _particle: Particle) {
        todo!("ported at M9 — replace-oldest-by-life-fraction (source-map §3.6)")
    }
}

impl Default for ParticleArena {
    fn default() -> Self {
        Self::new()
    }
}

/// A bullet-hole decal.
pub struct Hole {
    pub position: Vec3,
    pub normal: Vec3,
}

/// 256-entry FIFO of wall decals (`s_MaximumHoles`).
pub struct HoleFifo {
    pub entries: std::collections::VecDeque<Hole>,
}

impl HoleFifo {
    pub fn new() -> Self {
        Self {
            entries: std::collections::VecDeque::with_capacity(MAX_HOLES),
        }
    }

    pub fn push(&mut self, hole: Hole) {
        if self.entries.len() == MAX_HOLES {
            self.entries.pop_front();
        }
        self.entries.push_back(hole);
    }
}

impl Default for HoleFifo {
    fn default() -> Self {
        Self::new()
    }
}

pub struct ParticleEmitter {
    pub emitter_type: EmitterTypeId,
    pub position: Vec3,
    pub age: f32,
}

pub struct ParticleEffect {
    pub emitters: Vec<ParticleEmitter>,
    /// Dynamic effects (spawned by an event, e.g. an explosion) never
    /// self-prune expired emitters — PARITY, the owner is responsible for
    /// deletion (source-map §3.6).
    pub is_dynamic: bool,
}
