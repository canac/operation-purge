//! Three-tier particle system: `cParticle` / `cParticleEmitter` /
//! `cParticleEffect`, ported from `cParticle.cpp`, `cParticleEmitter.cpp`,
//! `cParticleEffect.cpp`, plus the particle/hole pools `cOperationPurge`
//! owns (`AddParticle`/`AddHole`/`AddParticleEffect`,
//! `cOperationPurge.cpp:1421-1513`). `sim::world` drives them in
//! `MoveObjects` order (`cOperationPurge.cpp:683-772`).
//!
//! PARITY (preserved verbatim):
//! - 1024-particle cap; when full the particle with the greatest life
//!   fraction is replaced (`cOperationPurge.cpp:1459-1476`).
//! - `fEmitRate` is a **delay** between emissions, not a frequency
//!   (`cParticleEmitter.cpp:57-73`); sub-frame spawn interpolation +
//!   pre-aging (`pNewParticle->Move(dt*(1-factor))`) keep trails gap-free.
//! - Emitter shortest-arc quaternion ported verbatim, including degenerate
//!   cases (NaN on zero velocity, `(0,0,1,0)` fallback on anti-parallel) —
//!   NOT glam's `from_rotation_arc` (see `math::d3dx`).
//! - Static effects prune expired emitters; **dynamic effects never do**
//!   (owner must delete) — do not "fix" this into always-pruning.

use crate::data::{GameData, ParticleType};
use crate::ids::{EffectDefId, EmitterTypeId, ParticleTypeId};
use crate::math::intersect::{point_box, Box3};
use crate::math::{sphere_coord, Rgba};
use crate::math::d3dx;
use crate::rng::GameRng;
use glam::Vec3;

/// `s_MaximumParticles` (`cOperationPurge.cpp:18`).
pub const MAX_PARTICLES: usize = 1024;

/// `s_MaximumHoles` (`cOperationPurge.cpp:19`).
pub const MAX_HOLES: usize = 256;

/// PARITY: `fRandVariance` (`Functions.cpp:96-99`).
fn rand_variance(rng: &mut GameRng, median: f32, variance: f32) -> f32 {
    median + rng.frand() * variance * 2.0 - variance
}

/// PARITY: `fRandMinMax` (`Functions.cpp:102-105`).
fn rand_min_max(rng: &mut GameRng, min: f32, max: f32) -> f32 {
    min + rng.frand() * (max - min)
}

// ============================ cParticle ============================

/// A live particle (`cParticle`, `cParticle.cpp`). Kinematics:
/// `p += v*t + ½a*t²`, `v += a*t`, with `a = (0,0,fRiseAcceleration)`.
pub struct Particle {
    pub particle_type: ParticleTypeId,
    pub position: Vec3,
    pub velocity: Vec3,
    /// `m_vec3Acceleration = (0, 0, fRiseAcceleration)`.
    pub acceleration: Vec3,
    /// `m_fLifetimeTotal = fRandVariance(fLifetime, fLifetimeVariance)`.
    pub lifetime_total: f32,
    /// `m_fLifetime`, elapsed seconds.
    pub lifetime: f32,
    /// `m_angInitialRotation = fRandVariance(0, angInitialRotationVariance)`.
    pub initial_rotation: f32,
    /// `m_angRotationRate = fRandVariance(0, angRotationRateVariance)`.
    pub rotation_rate: f32,
}

impl Particle {
    /// PARITY: `cParticle::cParticle` (`cParticle.cpp:10-27`). Constructor
    /// initializer order fixes the RNG draw sequence: lifetime-total,
    /// initial-rotation, rotation-rate. `parent` is the emitter's parent
    /// velocity; `velocity` is the already-computed emit velocity.
    pub fn new(
        game_data: &GameData,
        particle_type: ParticleTypeId,
        position: Vec3,
        velocity: Vec3,
        parent: Vec3,
        rng: &mut GameRng,
    ) -> Self {
        let opts = &game_data.particle_types[particle_type.get() as usize];
        let lifetime_total = rand_variance(rng, opts.lifetime, opts.lifetime_variance);
        let initial_rotation = rand_variance(rng, 0.0, opts.initial_rotation_variance);
        let rotation_rate = rand_variance(rng, 0.0, opts.rotation_rate_variance);
        Self {
            particle_type,
            position,
            // m_vec3Velocity = parent*fInheritedVelocityFactor + emit + (0,0,fRiseSpeed)
            velocity: parent * opts.inherited_velocity_factor
                + velocity
                + Vec3::new(0.0, 0.0, opts.rise_speed),
            acceleration: Vec3::new(0.0, 0.0, opts.rise_acceleration),
            lifetime_total,
            lifetime: 0.0,
            initial_rotation,
            rotation_rate,
        }
    }

    /// PARITY: `cParticle::Move` (`cParticle.cpp:52-57`).
    pub fn move_by(&mut self, dt: f32) {
        self.lifetime += dt;
        self.position += self.velocity * dt + self.acceleration * 0.5 * dt * dt;
        self.velocity += self.acceleration * dt;
    }

    /// PARITY: `cParticle::IsLifeExpired` (`cParticle.cpp:60-63`).
    pub fn is_life_expired(&self) -> bool {
        self.lifetime >= self.lifetime_total
    }

    /// PARITY: `cParticle::GetLifetimePortion` (`cParticle.cpp:66-69`).
    /// `0 <= return < 1` for a live particle; a free arena slot is treated
    /// as `+inf` by the allocator so it evicts first.
    pub fn lifetime_portion(&self) -> f32 {
        self.lifetime / self.lifetime_total
    }

    /// PARITY: `cParticle::GetStage` (`cParticle.cpp:88-121`). Returns
    /// `(stage1, stage2, weight)` where the color/size is
    /// `lerp(stage1, stage2, 1 - weight)`... actually `stage1 +
    /// (stage2-stage1)*weight` per the C++ (weight applies to stage2's pull).
    fn get_stage(&self, opts: &ParticleType) -> (usize, usize, f32) {
        let time_fraction = self.lifetime / self.lifetime_total;
        let n = opts.stages.len();
        for i in 0..n {
            if time_fraction < opts.stages[i].time {
                if i == 0 {
                    return (0, 0, 1.0);
                }
                let prev = &opts.stages[i - 1];
                let cur = &opts.stages[i];
                let weight = (time_fraction - prev.time) / (cur.time - prev.time);
                return (i - 1, i, weight);
            }
        }
        (n - 1, n - 1, 1.0)
    }

    /// PARITY: `cParticle::GetSize` (`cParticle.cpp:124-135`).
    pub fn size(&self, game_data: &GameData) -> f32 {
        let opts = &game_data.particle_types[self.particle_type.get() as usize];
        let (s1, s2, w) = self.get_stage(opts);
        let size1 = opts.stages[s1].size;
        let size2 = opts.stages[s2].size;
        size1 + (size2 - size1) * w
    }

    /// PARITY: `cParticle::GetRotation` (`cParticle.cpp:138-141`).
    pub fn rotation(&self) -> f32 {
        self.initial_rotation + self.rotation_rate * self.lifetime
    }

    /// PARITY: `cParticle::GetColor` (`cParticle.cpp:144-155`) — full RGBA
    /// lerp (`Color1 + (Color2 - Color1) * weight`).
    pub fn color(&self, game_data: &GameData) -> Rgba {
        let opts = &game_data.particle_types[self.particle_type.get() as usize];
        let (s1, s2, w) = self.get_stage(opts);
        let c1 = opts.stages[s1].color;
        let c2 = opts.stages[s2].color;
        Rgba::new(
            c1.r + (c2.r - c1.r) * w,
            c1.g + (c2.g - c1.g) * w,
            c1.b + (c2.b - c1.b) * w,
            c1.a + (c2.a - c1.a) * w,
        )
    }
}

/// The `m_lpParticles` pool as a fixed 1024-slot arena.
///
/// PARITY deviation: the C++ list appends new particles at the back, so its
/// iteration (render) order and eviction tie-break follow insertion order;
/// here a new particle takes the lowest free slot and ties go to the lowest
/// index. Which particle is evicted otherwise matches.
pub struct ParticleArena {
    pub(crate) slots: Vec<Option<Particle>>,
    live: usize,
    /// Every slot below this index is occupied.
    free_hint: usize,
}

impl ParticleArena {
    pub fn new() -> Self {
        Self {
            slots: (0..MAX_PARTICLES).map(|_| None).collect(),
            live: 0,
            free_hint: 0,
        }
    }

    /// PARITY: `cOperationPurge::AddParticle` (`cOperationPurge.cpp:1454-1483`).
    /// Takes a free slot while below the cap; when full, replaces the first
    /// particle with the greatest life fraction. Returns the chosen index.
    pub fn allocate(&mut self, particle: Particle) -> usize {
        let idx = if self.live < MAX_PARTICLES {
            let idx = (self.free_hint..MAX_PARTICLES)
                .find(|&i| self.slots[i].is_none())
                .expect("live count below the cap implies a free slot");
            self.live += 1;
            self.free_hint = idx + 1;
            idx
        } else {
            let mut oldest = 0;
            let mut oldest_portion = self.portion(0);
            for i in 1..MAX_PARTICLES {
                let portion = self.portion(i);
                if portion > oldest_portion {
                    oldest = i;
                    oldest_portion = portion;
                }
            }
            oldest
        };
        self.slots[idx] = Some(particle);
        idx
    }

    fn portion(&self, i: usize) -> f32 {
        self.slots[i].as_ref().map_or(f32::INFINITY, Particle::lifetime_portion)
    }

    /// PARITY: the "Move particles" pass of `MoveObjects`
    /// (`cOperationPurge.cpp:683-697`): move every particle, then drop it if
    /// its life is over or it left `bounds` (the building box).
    pub fn move_all(&mut self, dt: f32, bounds: Box3) {
        for (i, slot) in self.slots.iter_mut().enumerate() {
            if let Some(p) = slot {
                p.move_by(dt);
                if p.is_life_expired() || !point_box(p.position, bounds) {
                    *slot = None;
                    self.live -= 1;
                    self.free_hint = self.free_hint.min(i);
                }
            }
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &Particle> {
        self.slots.iter().flatten()
    }

    pub fn live_count(&self) -> usize {
        self.live
    }
}

impl Default for ParticleArena {
    fn default() -> Self {
        Self::new()
    }
}

// ============================ bullet holes ============================

/// A bullet-hole decal (`tHole`, `cOperationPurge.cpp:1421-1436`).
pub struct Hole {
    pub position: Vec3,
    /// Outward wall normal (`OrientationAxis`).
    pub normal: Vec3,
    /// `fRand () * 2π`, drawn when the hole is added.
    pub rotation: f32,
    /// The projectile type's `hole_size`.
    pub size: f32,
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

// ========================= cParticleEmitter =========================

/// PARITY: the shortest-arc rotation that maps +Z onto `emitter_velocity`,
/// ported verbatim from `cParticleEmitter::GetEmitVelocity`
/// (`cParticleEmitter.cpp:96-127`). Returns the quaternion `(x,y,z,w)` —
/// either the normalized shortest-arc quaternion, or the `(0,0,1,0)`
/// fallback (`w <= 0`). Degenerate cases (design §1.3, source-map §7):
/// - **zero velocity** -> `D3DXVec3Normalize(0)` divides by zero -> `NaN`;
///   the quaternion stays `NaN` (neither `w < 0` nor `Equals(w,0)` fire).
/// - **anti-parallel** (velocity along -Z) -> `w` normalizes to `0`,
///   tripping the fallback, which is a 180° spin about Z that does NOT flip
///   the cone onto -Z (glam `from_rotation_arc` would).
pub fn emit_orientation_quaternion(emitter_velocity: Vec3) -> (f32, f32, f32, f32) {
    // D3DXVec3Normalize — raw division so a zero vector yields NaN (PARITY;
    // NOT glam's `normalize`, which debug-asserts finiteness).
    let n = emitter_velocity / emitter_velocity.length();
    let z_axis = Vec3::new(0.0, 0.0, 1.0);
    let cross = z_axis.cross(n);
    let (x, y, z, w) =
        d3dx::quaternion_normalize(cross.x, cross.y, cross.z, 1.0 + z_axis.dot(n));
    if w < 0.0 || crate::math::equals(w, 0.0) {
        (0.0, 0.0, 1.0, 0.0)
    } else {
        (x, y, z, w)
    }
}

/// A single emitter (`cParticleEmitter`). Spawns one particle type through
/// the shared arena on a `fEmitRate`-delay schedule.
pub struct ParticleEmitter {
    pub emitter_type: EmitterTypeId,
    pub particle_type: ParticleTypeId,
    /// `m_bStatic`: a static emitter never moves and never inherits its
    /// owner's motion; `emit_by_distance` is ignored (uses elapsed time).
    pub is_static: bool,
    pub lifetime_total: f32,
    pub lifetime: f32,
    pub position: Vec3,
    pub velocity: Vec3,
    pub acceleration: Vec3,
    /// `m_fNextEmitDelay`: seconds (or distance units) until the next spawn.
    pub next_emit_delay: f32,
}

impl ParticleEmitter {
    /// PARITY: `cParticleEmitter::cParticleEmitter` (`cParticleEmitter.cpp:10-24`).
    /// Initializer order fixes the RNG sequence: lifetime-total, then (in the
    /// body) the first emit delay.
    pub fn new(
        game_data: &GameData,
        emitter_type: EmitterTypeId,
        particle_type: ParticleTypeId,
        is_static: bool,
        position: Vec3,
        velocity: Vec3,
        rng: &mut GameRng,
    ) -> Self {
        let opts = &game_data.particle_emitter_types[emitter_type.get() as usize];
        let lifetime_total = rand_variance(rng, opts.lifetime, opts.lifetime_variance);
        let next_emit_delay = rand_variance(rng, opts.emit_rate, opts.emit_rate_variance);
        Self {
            emitter_type,
            particle_type,
            is_static,
            lifetime_total,
            lifetime: 0.0,
            position,
            velocity,
            acceleration: Vec3::ZERO,
            next_emit_delay,
        }
    }

    /// PARITY: `cParticleEmitter::IsLifeExpired` (`cParticleEmitter.cpp:79-82`).
    pub fn is_life_expired(&self) -> bool {
        self.lifetime >= self.lifetime_total
    }

    /// PARITY: `GetEmitDelay` (`cParticleEmitter.cpp:130-133`).
    fn emit_delay(&self, game_data: &GameData, rng: &mut GameRng) -> f32 {
        let opts = &game_data.particle_emitter_types[self.emitter_type.get() as usize];
        rand_variance(rng, opts.emit_rate, opts.emit_rate_variance)
    }

    /// PARITY: `cParticleEmitter::GetEmitVelocity(vec3Emitter)`
    /// (`cParticleEmitter.cpp:96-127`). RNG order: theta, phi, speed, then
    /// (if relative) the quaternion rotation onto the emitter velocity.
    fn emit_velocity(&self, parent: Vec3, game_data: &GameData, rng: &mut GameRng) -> Vec3 {
        let opts = &game_data.particle_emitter_types[self.emitter_type.get() as usize];
        let theta = rand_min_max(rng, opts.emit_theta_min, opts.emit_theta_max);
        let phi = rand_min_max(rng, opts.emit_phi_min, opts.emit_phi_max);
        let speed = rand_variance(rng, opts.emit_velocity, opts.emit_velocity_variance);
        let emit = sphere_coord(theta, phi, speed);
        if opts.emit_relative {
            let (x, y, z, w) = emit_orientation_quaternion(parent);
            let mat = d3dx::matrix_rotation_quaternion(x, y, z, w);
            // D3DXVec3TransformCoord: row-vector * matrix, then w-divide
            // (w stays 1 for an affine rotation).
            let out = mat.mul_row_vec4([emit.x, emit.y, emit.z, 1.0]);
            Vec3::new(out[0], out[1], out[2])
        } else {
            emit
        }
    }

    /// PARITY: `cParticleEmitter::Move(fTimeElapsed)`
    /// (`cParticleEmitter.cpp:32-40`) — advance the emitter under its own
    /// kinematics, then run the two-arg move.
    pub fn move_self(&mut self, dt: f32, game_data: &GameData, arena: &mut ParticleArena, rng: &mut GameRng) {
        let position_new = self.position + self.velocity * dt + 0.5 * self.acceleration * dt * dt;
        let velocity_new = self.velocity + self.acceleration * dt;
        self.move_with(dt, position_new, velocity_new, game_data, arena, rng);
    }

    /// PARITY: `cParticleEmitter::Move(fTimeElapsed, pt3Position, vec3Velocity)`
    /// (`cParticleEmitter.cpp:44-77`) — the sub-frame spawn loop. Newborn
    /// particles are lerped along the emitter's path and pre-aged by the
    /// frame remainder so trails stay gap-free at any game speed.
    pub fn move_with(
        &mut self,
        dt: f32,
        position: Vec3,
        velocity: Vec3,
        game_data: &GameData,
        arena: &mut ParticleArena,
        rng: &mut GameRng,
    ) {
        let position_old = self.position;
        let velocity_old = self.velocity;
        if !self.is_static {
            self.position = position;
            self.velocity = velocity;
        }
        let opts = &game_data.particle_emitter_types[self.emitter_type.get() as usize];
        let amount_delayed = if !self.is_static && opts.emit_by_distance {
            position_old.distance(self.position)
        } else {
            dt
        };

        // PARITY: fEmitRate is a delay, consumed by sub-frame interpolation.
        let mut interpolate_factor = self.next_emit_delay / amount_delayed;
        self.next_emit_delay -= amount_delayed;
        while self.next_emit_delay <= 0.0 {
            let particle_position =
                position_old + (self.position - position_old) * interpolate_factor;
            let parent_velocity =
                velocity_old + (self.velocity - velocity_old) * interpolate_factor;
            let particle_velocity = self.emit_velocity(parent_velocity, game_data, rng);
            let idx = arena.allocate(Particle::new(
                game_data,
                self.particle_type,
                particle_position,
                particle_velocity,
                parent_velocity,
                rng,
            ));
            // Pre-age the newborn by the remaining frame time.
            if let Some(p) = arena.slots[idx].as_mut() {
                p.move_by(dt * (1.0 - interpolate_factor));
            }

            let new_emit_delay = self.emit_delay(game_data, rng);
            self.next_emit_delay += new_emit_delay;
            interpolate_factor += new_emit_delay / amount_delayed;
        }
        self.lifetime += dt;
    }
}

// ========================== cParticleEffect ==========================

/// A particle effect (`cParticleEffect`): one emitter per component of its
/// definition. Static effects prune expired emitters; dynamic effects never
/// do (the owner deletes them) — PARITY, `cParticleEffect.cpp:73-97`.
pub struct ParticleEffect {
    pub emitters: Vec<ParticleEmitter>,
    /// Inverse of the C++ `m_bStatic`. Kept as `is_dynamic` (design §2
    /// naming): dynamic effects (`is_dynamic == true`) never self-prune.
    pub is_dynamic: bool,
}

impl ParticleEffect {
    /// PARITY: `cParticleEffect::cParticleEffect` (`cParticleEffect.cpp:6-27`).
    /// `is_dynamic == true` maps to C++ `bStatic == false`. Components whose
    /// emitter/particle type is `None` are skipped (GUARD: the C++ indexed
    /// with the raw id — the sample data always populates both).
    pub fn new(
        game_data: &GameData,
        effect: EffectDefId,
        is_dynamic: bool,
        position: Vec3,
        velocity: Vec3,
        rng: &mut GameRng,
    ) -> Self {
        let def = &game_data.particle_effect_definitions[effect.get() as usize];
        let mut emitters = Vec::with_capacity(def.components.len());
        // Snapshot the (emitter, particle) id pairs so the game_data borrow
        // is released before the per-emitter RNG draws.
        let pairs: Vec<(EmitterTypeId, ParticleTypeId)> = def
            .components
            .iter()
            .filter_map(|c| Some((c.particle_emitter_type?, c.particle_type?)))
            .collect();
        for (emitter_type, particle_type) in pairs {
            emitters.push(ParticleEmitter::new(
                game_data,
                emitter_type,
                particle_type,
                !is_dynamic,
                position,
                velocity,
                rng,
            ));
        }
        Self {
            emitters,
            is_dynamic,
        }
    }

    /// PARITY: `cParticleEffect::Move(fTimeElapsed)` (`cParticleEffect.cpp:40-49`).
    pub fn move_self(&mut self, dt: f32, game_data: &GameData, arena: &mut ParticleArena, rng: &mut GameRng) {
        for e in self.emitters.iter_mut() {
            e.move_self(dt, game_data, arena, rng);
        }
        self.remove_emitters();
    }

    /// PARITY: `cParticleEffect::Move(fTimeElapsed, pt3Position, vec3Velocity)`
    /// (`cParticleEffect.cpp:52-62`) — drive every emitter to the owner's
    /// new position/velocity (used by moving-object trails).
    pub fn move_with(
        &mut self,
        dt: f32,
        position: Vec3,
        velocity: Vec3,
        game_data: &GameData,
        arena: &mut ParticleArena,
        rng: &mut GameRng,
    ) {
        for e in self.emitters.iter_mut() {
            e.move_with(dt, position, velocity, game_data, arena, rng);
        }
        self.remove_emitters();
    }

    /// PARITY: `cParticleEffect::IsFinished` (`cParticleEffect.cpp:65-68`).
    pub fn is_finished(&self) -> bool {
        self.emitters.is_empty()
    }

    /// PARITY: `cParticleEffect::RemoveEmitters` (`cParticleEffect.cpp:71-97`).
    /// Dynamic effects never remove emitters (the owner deletes the whole
    /// effect); static effects drop expired ones.
    fn remove_emitters(&mut self) {
        if self.is_dynamic {
            return;
        }
        self.emitters.retain(|e| !e.is_life_expired());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::gamedata;
    use std::path::Path;

    fn load_data() -> GameData {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/GameData.toml");
        gamedata::load(&path).expect("GameData.toml")
    }

    // ---- emitter degenerate-case quaternions (design §6) ----

    #[test]
    fn emit_orientation_zero_velocity_is_nan() {
        // PARITY: D3DXVec3Normalize(0) -> NaN, no fallback fires.
        let (x, y, z, w) = emit_orientation_quaternion(Vec3::ZERO);
        assert!(x.is_nan() && y.is_nan() && z.is_nan() && w.is_nan());
    }

    #[test]
    fn emit_orientation_antiparallel_uses_z_spin_fallback() {
        // PARITY: velocity along -Z -> w normalizes to 0 -> (0,0,1,0)
        // fallback (a 180° spin about Z that does NOT flip the cone).
        let q = emit_orientation_quaternion(Vec3::new(0.0, 0.0, -1.0));
        assert_eq!(q, (0.0, 0.0, 1.0, 0.0));
    }

    #[test]
    fn emit_orientation_parallel_is_identity() {
        // Velocity already along +Z -> shortest arc is identity (0,0,0,1).
        let (x, y, z, w) = emit_orientation_quaternion(Vec3::new(0.0, 0.0, 3.0));
        assert!(x.abs() < 1e-6 && y.abs() < 1e-6 && z.abs() < 1e-6);
        assert!((w - 1.0).abs() < 1e-6);
    }

    #[test]
    fn emit_velocity_is_nan_when_relative_and_emitter_stationary() {
        // A relative emitter with zero parent velocity sprays NaN particles
        // (the preserved zero-velocity quirk). Uses the "Bullet fire"
        // emitter (index 0, emit_relative = true).
        let gd = load_data();
        let mut rng = GameRng::from_seed(1);
        let emitter = ParticleEmitter::new(
            &gd,
            EmitterTypeId::new(0),
            ParticleTypeId::new(0),
            true,
            Vec3::ZERO,
            Vec3::ZERO,
            &mut rng,
        );
        let v = emitter.emit_velocity(Vec3::ZERO, &gd, &mut rng);
        assert!(v.x.is_nan() || v.y.is_nan() || v.z.is_nan());
    }

    // ---- arena replace-oldest-by-life-fraction (design §6) ----

    fn test_particle(portion_num: f32, portion_den: f32) -> Particle {
        // Construct a particle with a known life fraction directly.
        Particle {
            particle_type: ParticleTypeId::new(0),
            position: Vec3::ZERO,
            velocity: Vec3::ZERO,
            acceleration: Vec3::ZERO,
            lifetime_total: portion_den,
            lifetime: portion_num,
            initial_rotation: 0.0,
            rotation_rate: 0.0,
        }
    }

    #[test]
    fn arena_fills_free_slots_lowest_index_first() {
        let mut arena = ParticleArena::new();
        // Both slots free -> lowest index (0) wins, then 1, then 2 ...
        assert_eq!(arena.allocate(test_particle(0.1, 1.0)), 0);
        assert_eq!(arena.allocate(test_particle(0.9, 1.0)), 1);
        assert_eq!(arena.allocate(test_particle(0.5, 1.0)), 2);
    }

    #[test]
    fn arena_evicts_particle_nearest_death_when_full() {
        let mut arena = ParticleArena::new();
        for i in 0..MAX_PARTICLES {
            let frac = if i == 7 { 0.99 } else { 0.10 };
            arena.allocate(test_particle(frac, 1.0));
        }
        let idx = arena.allocate(test_particle(0.0, 1.0));
        assert_eq!(idx, 7);
        assert_eq!(arena.slots[7].as_ref().unwrap().lifetime, 0.0);
        assert_eq!(arena.live_count(), MAX_PARTICLES);
    }

    #[test]
    fn arena_refills_the_lowest_freed_slot() {
        let mut arena = ParticleArena::new();
        for _ in 0..4 {
            arena.allocate(test_particle(0.5, 1.0));
        }
        arena.slots[2].as_mut().unwrap().lifetime = 1.0;
        arena.slots[1].as_mut().unwrap().lifetime = 1.0;
        arena.move_all(0.0, big_box());
        assert_eq!(arena.live_count(), 2);
        assert_eq!(arena.allocate(test_particle(0.0, 1.0)), 1);
        assert_eq!(arena.allocate(test_particle(0.0, 1.0)), 2);
        assert_eq!(arena.allocate(test_particle(0.0, 1.0)), 4);
    }

    #[test]
    fn particles_outside_the_building_are_culled() {
        let mut arena = ParticleArena::new();
        let mut inside = test_particle(0.0, 1.0);
        inside.position = Vec3::splat(1.0);
        let mut outside = test_particle(0.0, 1.0);
        outside.position = Vec3::new(1.0, 1.0, -1.0);
        arena.allocate(inside);
        arena.allocate(outside);
        arena.move_all(0.0, Box3 { center: Vec3::splat(5.0), size: Vec3::splat(10.0) });
        assert_eq!(arena.live_count(), 1);
        assert!(arena.slots[0].is_some());
    }

    fn big_box() -> Box3 {
        Box3 { center: Vec3::ZERO, size: Vec3::splat(1e6) }
    }

    // ---- fEmitRate delay / accumulation (design §6) ----

    fn make_emitter(
        gd: &GameData,
        emitter_type: u32,
        is_static: bool,
        rng: &mut GameRng,
    ) -> ParticleEmitter {
        ParticleEmitter::new(
            gd,
            EmitterTypeId::new(emitter_type),
            ParticleTypeId::new(0),
            is_static,
            Vec3::ZERO,
            Vec3::ZERO,
            rng,
        )
    }

    #[test]
    fn emit_rate_is_a_delay_not_a_frequency() {
        // A time-based static emitter with emit_rate r spawns ~ dt/r
        // particles per Move(dt). "Bullet fire" (index 0): emit_rate 0.005,
        // variance 0.003 -> per-emit delay in [0.002, 0.008]. Over dt = 0.1s
        // the count is bounded by dt/min_delay and dt/max_delay.
        let gd = load_data();
        let mut rng = GameRng::from_seed(42);
        let mut emitter = make_emitter(&gd, 0, true, &mut rng);
        let mut arena = ParticleArena::new();
        emitter.move_with(0.1, Vec3::ZERO, Vec3::ZERO, &gd, &mut arena, &mut rng);
        let n = arena.live_count();
        // dt/max_delay = 0.1/0.008 = 12.5, dt/min_delay = 0.1/0.002 = 50.
        assert!((12..=51).contains(&n), "spawned {n}");
    }

    #[test]
    fn emit_delay_accumulates_across_frames() {
        // With emit_rate variance 0 the delay is exact; verify the residual
        // next_emit_delay carries across two Move calls (no reset).
        let mut gd = load_data();
        // Force a deterministic 0.01s delay, no lifetime expiry.
        let et = &mut gd.particle_emitter_types[0];
        et.emit_rate = 0.01;
        et.emit_rate_variance = 0.0;
        et.lifetime = 100.0;
        et.lifetime_variance = 0.0;
        et.emit_relative = false; // avoid NaN cone with zero parent velocity
        let mut rng = GameRng::from_seed(7);
        let mut emitter = make_emitter(&gd, 0, true, &mut rng);
        assert!((emitter.next_emit_delay - 0.01).abs() < 1e-6);
        let mut arena = ParticleArena::new();
        // dt = 0.025 -> spawns at delays 0.01, 0.02 (2 particles), residual
        // next_emit_delay = 0.03 - 0.025 = 0.005.
        emitter.move_with(0.025, Vec3::ZERO, Vec3::ZERO, &gd, &mut arena, &mut rng);
        assert_eq!(arena.live_count(), 2);
        assert!((emitter.next_emit_delay - 0.005).abs() < 1e-5, "{}", emitter.next_emit_delay);
        // Next dt = 0.01 consumes the 0.005 residual + one more emit.
        emitter.move_with(0.01, Vec3::ZERO, Vec3::ZERO, &gd, &mut arena, &mut rng);
        assert_eq!(arena.live_count(), 3);
    }

    // ---- sub-frame interpolation count / gap-free trail (design §6) ----

    #[test]
    fn distance_emitter_spawn_count_scales_with_travel_not_dt() {
        // The rocket travel emitter (index 2) is emit_by_distance: the spawn
        // count tracks distance traveled, so a 25%-speed and a 400%-speed
        // frame covering the same distance produce the same particle count
        // (gap-free trail). emit_rate 0.05 distance units, variance 0.02.
        let mut gd = load_data();
        let et = &mut gd.particle_emitter_types[2];
        et.emit_rate = 0.05;
        et.emit_rate_variance = 0.0; // deterministic spacing
        et.emit_relative = false; // velocity is (10,0,0); avoid the cone rot
        let seed = 99;

        // Slow frame: dt large, small step. Fast frame: dt small, same step.
        let travel = Vec3::new(1.0, 0.0, 0.0); // 1.0 distance unit
        let run = |dt: f32| {
            let mut rng = GameRng::from_seed(seed);
            let mut emitter = ParticleEmitter::new(
                &gd,
                EmitterTypeId::new(2),
                ParticleTypeId::new(0),
                false,
                Vec3::ZERO,
                Vec3::new(10.0, 0.0, 0.0),
                &mut rng,
            );
            let mut arena = ParticleArena::new();
            emitter.move_with(dt, travel, Vec3::new(10.0, 0.0, 0.0), &gd, &mut arena, &mut rng);
            arena.live_count()
        };
        // next_emit_delay starts at 0.05; 1.0 distance / 0.05 ~ 20 spawns
        // (19 in f32: the accumulated 0.05 delays drift just above 0 on the
        // final check — the same drift the C++ f32 math produces).
        let slow = run(0.4); // 400% game speed maps to a big dt for the step
        let fast = run(0.025); // 25% speed, tiny dt, same 1.0 distance
        assert_eq!(
            slow, fast,
            "trail density must depend on distance, not dt (gap-free at any speed)"
        );
        assert_eq!(slow, 19, "deterministic f32 spawn count for a 1.0-unit trail");
    }

    // ---- particle stage interpolation ----

    #[test]
    fn particle_color_and_size_interpolate_between_stages() {
        // Bullet fire particle (index 0). Age it to the midpoint between the
        // first two stages and confirm the lerp is monotonic between them.
        let gd = load_data();
        let opts = &gd.particle_types[0];
        assert!(opts.stages.len() >= 2, "test needs >= 2 stages");
        let mut rng = GameRng::from_seed(3);
        let mut p = Particle::new(&gd, ParticleTypeId::new(0), Vec3::ZERO, Vec3::ZERO, Vec3::ZERO, &mut rng);
        // Pin lifetime_total so the fraction is exact.
        p.lifetime_total = 1.0;
        let t0 = opts.stages[0].time;
        let t1 = opts.stages[1].time;
        p.lifetime = (t0 + t1) * 0.5;
        let (s1, s2, w) = p.get_stage(opts);
        assert_eq!((s1, s2), (0, 1));
        assert!((0.0..=1.0).contains(&w));
        let size = p.size(&gd);
        let lo = opts.stages[0].size.min(opts.stages[1].size);
        let hi = opts.stages[0].size.max(opts.stages[1].size);
        assert!(size >= lo && size <= hi);
    }
}
