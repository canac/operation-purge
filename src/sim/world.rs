//! `World`: owns every live simulation entity and runs the per-frame step
//! in the original's exact order (design §2, §5.1; source-map §3.4
//! `cOperationPurge`'s sim half), ported from `cOperationPurge.cpp`,
//! `cBuilding.cpp` (collision helpers), `cRobot.cpp`, `cProjectile.cpp`,
//! `cCenter.cpp`.
//!
//! Ownership/iteration-order contract (PARITY-critical, design §5.1):
//! `robots: BTreeMap<RobotId, Robot>` — ascending-ID iteration provably
//! matches `std::map<tRobotID, cAIRobot*>` over the `cIDGenerator`
//! sequence (pinned by `sim::idgen` tests); `projectiles: Vec<Projectile>`
//! with in-order (never `swap_remove`) retain-style removal; `centers`
//! iterated in ascending `CenterId` order (the Vec is sorted by id at
//! setup) to match `std::map<tCenterID, cCenter*>`.
//!
//! All of the original's function-local statics (0.25s enemy-visible
//! throttle, 0.03s cheat cooldown, 10-frame listener counter, LightData
//! read-once) become fields here, reset in `start_game()` (design §5.1,
//! source-map §8.11).
//!
//! M6 scope: the sim runs **headless and drivable without AI** — every
//! robot is controlled by a per-frame [`RobotInput`] (design §5.2's
//! possession path). AI decision-making (`CalcNewPositionAI`/`MoveAI`) is
//! M7 (`sim::ai` stays a stub). Audio alarms/listener and particles are
//! stubbed here (M9/M10 hooks).

use crate::data::{GameData, RobotType};
use crate::ids::{CenterId, EffectDefId, GameDefId, RobotId, TeamId};
use crate::maze::building::{Building, WallDir};
use crate::math::intersect::{rect_rect, Box3, Rect};
use crate::rng::GameRng;
use crate::sim::ai::AiRuntime;
use crate::sim::center::{Center, CenterKind, Orientation};
use crate::sim::idgen::IdGenerator;
use crate::sim::particles::{Hole, HoleFifo, ParticleArena, ParticleEffect};
use crate::sim::projectile::Projectile;
use crate::sim::robot::{f_min, Collision2D, CollisionKind, Robot, Throttle};
use glam::{Vec2, Vec3};
use std::collections::BTreeMap;

/// `eStatus` (source-map §1): the game's win/lose/draw/ongoing state.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Status {
    /// No terminal condition yet (`statusNone`).
    None,
    Won,
    Lost,
    Draw,
}

/// Per-robot control for one frame, replacing the possession path of
/// `cOperationPurge::ExecuteCommands` (design §5.2). The sim is fully
/// drivable through this without AI (M7 will synthesize these).
#[derive(Copy, Clone, Debug)]
pub struct RobotInput {
    /// `SetMoveState` target (`cOperationPurge.cpp:1185-1192`).
    pub throttle: Throttle,
    /// Radians added to `direction_desired` this frame (`Rotate`).
    pub turn: f32,
    /// Radians added to barrel elevation this frame (`Elevate`).
    pub elevate: f32,
    /// Desired-speed delta this frame (`Accelerate`).
    pub accel: f32,
    /// Fire the last ready-to-fire projectile type (`Fire()`).
    pub fire: bool,
}

impl Default for RobotInput {
    fn default() -> Self {
        // PARITY: no move keys down => cruise (`cOperationPurge.cpp:1190`).
        Self {
            throttle: Throttle::Cruise,
            turn: 0.0,
            elevate: 0.0,
            accel: 0.0,
            fire: false,
        }
    }
}

pub struct World {
    pub game_data: GameData,
    pub game_def: GameDefId,
    pub active_team: TeamId,

    pub building: Building,
    pub robots: BTreeMap<RobotId, Robot>,
    /// Per-robot AI runtime (populated at M7; empty in M6).
    pub ai: BTreeMap<RobotId, AiRuntime>,
    pub projectiles: Vec<Projectile>,
    /// Sorted ascending by `CenterId` (std::map iteration parity).
    pub centers: Vec<Center>,
    pub particles: ParticleArena,
    pub holes: HoleFifo,
    pub effects: Vec<ParticleEffect>,

    pub id_gen: IdGenerator,
    pub center_id_gen: IdGenerator,
    pub rng: GameRng,

    /// Per-robot input for the current frame (`ExecuteCommands` twin).
    pub inputs: BTreeMap<RobotId, RobotInput>,

    /// `m_fGameTime`.
    pub game_time: f32,
    /// `m_vSpawnInfo`, one per team definition.
    pub spawn_info: Vec<crate::sim::spawn::SpawnInfo>,
    /// `m_vCenterPositions`, the pool of candidate center slots.
    pub center_positions: Vec<crate::sim::spawn::CenterPosition>,

    // --- ex-function-local statics (reset in `start_game`, design §5.1) ---
    pub enemy_visible_throttle: f32,
    pub cheat_fire_cooldown: f32,
    pub listener_frame_counter: u32,
    /// M10 audio hook: the DS3D listener pose (observer/active-robot
    /// position + heading), refreshed every 10th frame by the app on the
    /// [`is_audio_listener_update_frame`](Self::is_audio_listener_update_frame)
    /// boundary. Headless `World` has no observer of its own, so it holds
    /// the neutral pose until the app pushes one via
    /// [`set_audio_listener`](Self::set_audio_listener). PARITY:
    /// `ListenerData` in `cOperationPurge.cpp:1276-1291`.
    pub audio_listener: crate::audio::spatial::AudioListener,
    /// One-shot 3D sounds raised this frame (`Play3DSound` call sites);
    /// drained by the app's audio pass.
    pub sound_events: Vec<SoundEvent>,
}

/// Which per-event sound to play (`PlayFireSound` / `PlayCollisionSound` /
/// `PlayDestroySound`).
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum SoundKind {
    Fire(crate::ids::ProjectileTypeId),
    Collision(crate::ids::ProjectileTypeId),
    Destroy,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct SoundEvent {
    pub kind: SoundKind,
    pub origin: Vec3,
}

impl World {
    /// Builds a fresh world for the given game definition + active team,
    /// mirroring `SetGameParams` (`cOperationPurge.cpp:104-150`) then
    /// `StartGame` (`cOperationPurge.cpp:153-306`). `seed` drives the
    /// shared RNG (design §1.5.11).
    pub fn new(
        game_data: GameData,
        maze: crate::maze::Maze,
        game_def: GameDefId,
        active_team: TeamId,
        seed: u64,
    ) -> Self {
        let building = Building {
            maze,
            wall_width: game_data.building.wall_width,
            room_size: game_data.building.room_size,
            ceiling_height: game_data.building.ceiling_height,
        };

        let mut world = Self {
            game_data,
            game_def,
            active_team,
            building,
            robots: BTreeMap::new(),
            ai: BTreeMap::new(),
            projectiles: Vec::new(),
            centers: Vec::new(),
            particles: ParticleArena::new(),
            holes: HoleFifo::new(),
            effects: Vec::new(),
            id_gen: IdGenerator::default(),
            center_id_gen: IdGenerator::default(),
            rng: GameRng::from_seed(seed),
            inputs: BTreeMap::new(),
            game_time: 0.0,
            sound_events: Vec::new(),
            spawn_info: Vec::new(),
            center_positions: Vec::new(),
            enemy_visible_throttle: 0.0,
            cheat_fire_cooldown: 0.0,
            listener_frame_counter: 0,
            audio_listener: crate::audio::spatial::AudioListener::default(),
        };
        world.start_game();
        world
    }

    fn game_definition(&self) -> &crate::data::GameDefinition {
        &self.game_data.game_definitions[self.game_def.get() as usize]
    }

    /// PARITY: `cOperationPurge::StartGame` (`cOperationPurge.cpp:153-306`).
    /// Resets every ex-static field and re-initializes the world for a
    /// fresh game (design §5.1).
    pub fn start_game(&mut self) {
        self.game_time = 0.0;
        self.robots.clear();
        self.ai.clear();
        self.projectiles.clear();
        self.centers.clear();
        self.effects.clear();
        self.holes = HoleFifo::new();
        self.sound_events.clear();
        self.inputs.clear();
        self.spawn_info.clear();
        self.center_positions.clear();
        self.id_gen.reset();
        self.center_id_gen.reset();

        // ex-statics
        self.enemy_visible_throttle = 0.0;
        self.cheat_fire_cooldown = 0.0;
        self.listener_frame_counter = 0;
        self.audio_listener = crate::audio::spatial::AudioListener::default();

        // Centers (before robots, as in StartGame).
        self.initialize_center_positions();
        self.create_center_groups();
        // std::map<tCenterID, cCenter*> iteration order = ascending id.
        self.centers.sort_by_key(|c| c.id.get());

        // Spawn info + initial robots per team.
        let n_teams = self.game_definition().teams.len();
        // PARITY: initial spawn point uses the robot's 3D footprint
        // (`Make2D(m_Robot3D.GetSize())` = (barrelLength*2, barrelLength*2)).
        let robot_size = crate::sim::robot::ROBOT_SIZE_XY;
        for t in 0..n_teams {
            let groups = &self.game_definition().teams[t].robot_groups;
            let info = if groups.is_empty() {
                crate::sim::spawn::SpawnInfo {
                    finished_spawning: true,
                    next_spawn: 0.0,
                    current_group: 0,
                    robots_left: 0,
                    spawn_point: Vec2::ZERO,
                }
            } else {
                let g0 = groups[0].clone();
                let next_spawn = self.rand_variance(g0.initial_delay, g0.initial_delay_variance);
                let spawn_point = self.get_rect_position(Vec2::new(robot_size, robot_size));
                crate::sim::spawn::SpawnInfo {
                    finished_spawning: false,
                    next_spawn,
                    current_group: 0,
                    robots_left: g0.count,
                    spawn_point,
                }
            };
            self.spawn_info.push(info);
            self.spawn_robots(t, 0.0);
        }
    }

    // ================= per-frame step (design §5.1) =================

    /// One simulation step, in the exact original order (design §5.1;
    /// `ExecuteCommands` fire/control, then `FrameChange`/`MoveObjects`,
    /// `cOperationPurge.cpp:609-775,1227-1240`): apply input -> spawn ->
    /// two-phase movement (<=11 collision passes) -> projectile hits ->
    /// remove destroyed robots -> center transfer -> (audio/listener stub).
    /// The end check is exposed via [`status`](Self::status) rather than
    /// popping a modal.
    ///
    /// `dt` is the game-time delta (the caller multiplies real dt by game
    /// speed, `cOperationPurge.cpp:1235`).
    pub fn frame(&mut self, dt: f32) {
        crate::profile_scope!("MoveObjects");
        // ExecuteCommands: fire + set throttle/rotate/elevate/accelerate.
        self.apply_inputs();

        // MoveObjects (`cOperationPurge.cpp:609`).
        self.game_time += dt;

        let n_teams = self.game_definition().teams.len();
        for t in 0..n_teams {
            self.spawn_robots(t, dt);
        }

        // Phase 1: calculate new positions.
        let ids: Vec<RobotId> = self.robots.keys().copied().collect();
        for id in &ids {
            let r = self.robots.get_mut(id).unwrap();
            if !r.is_dead() {
                r.calc_new_position(dt);
            }
        }

        // Phase 2: <=11 collision passes, mutating each robot's collision_time.
        let collisions = self.test_collisions(&ids);

        // Phase 3: commit moves + crash damage.
        for (i, id) in ids.iter().enumerate() {
            let r = self.robots.get_mut(id).unwrap();
            if !r.is_dead() {
                r.commit_move(collisions[i]);
            }
        }

        {
            crate::profile_scope!("Move particles");
            let size = self.building.size();
            let bounds = Box3 {
                center: size * 0.5,
                size,
            };
            self.particles.move_all(dt, bounds);
        }

        // Move projectiles + resolve hits (`cOperationPurge.cpp:702-736`).
        // Projectile trail (travel) effects and per-hit collision effects
        // are driven inside `step_projectiles` (M9).
        {
            crate::profile_scope!("Move projectiles");
            self.step_projectiles(dt);
        }

        // M9: advance static burst effects (fire/collision/destroy) and drop
        // finished ones. `mem::take` sidesteps the split borrow of
        // `self.effects` + `self.particles`/`self.rng`.
        {
            crate::profile_scope!("Move particle effects");
            let mut effects = std::mem::take(&mut self.effects);
            for eff in effects.iter_mut() {
                eff.move_self(dt, &self.game_data, &mut self.particles, &mut self.rng);
            }
            // PARITY: dynamic effects never self-prune (they never reach
            // `is_finished`); only static burst effects are removed here.
            effects.retain(|e| !e.is_finished());
            self.effects = effects;
        }

        // RemoveDestroyedRobots -> Transfer (`cOperationPurge.cpp:1239-1240`).
        self.remove_destroyed_robots();
        {
            crate::profile_scope!("Transfer");
            self.transfer(dt);
        }

        // Audio alarms/listener (`cOperationPurge.cpp:1247-1349`) — M10 hook.
        // PARITY: the original increments the 10-frame counter, then updates
        // the DS3D listener when it wraps to 0. The counter (an ex-static) is
        // kept live here; the app reads
        // [`is_audio_listener_update_frame`](Self::is_audio_listener_update_frame)
        // after each `frame()` to know when to refresh
        // [`audio_listener`](Self::audio_listener) and to drive the alarm
        // loops + per-event 3D sounds through `audio::SpatialSounds`.
        self.listener_frame_counter = (self.listener_frame_counter + 1)
            % crate::audio::spatial::LISTENER_UPDATE_INTERVAL_FRAMES;
    }

    /// M10 audio hook: `true` on the frame boundary where the DS3D listener
    /// must be refreshed (every 10th frame). PARITY: mirrors the original's
    /// `iFrame = (iFrame + 1) % UpdateFrameDelay; if (iFrame == 0)` gate
    /// (`cOperationPurge.cpp:1249-1253`) — call after [`frame`](Self::frame).
    pub fn is_audio_listener_update_frame(&self) -> bool {
        self.listener_frame_counter == 0
    }

    /// M10 audio hook: push the observer/active-robot pose the app resolved
    /// for this update frame (position + heading), building the DS3D
    /// listener exactly as `ExecuteCommands` did
    /// (`cOperationPurge.cpp:1277-1285`).
    pub fn set_audio_listener(&mut self, position: Vec2, rotation: f32) {
        self.audio_listener = crate::audio::spatial::AudioListener::from_pose(position, rotation);
    }

    /// Sets (or replaces) a robot's input for the next frame.
    pub fn set_input(&mut self, id: RobotId, input: RobotInput) {
        self.inputs.insert(id, input);
    }

    /// Clears all per-robot input (e.g. on focus loss, design §5.1/§5.2).
    pub fn clear_inputs(&mut self) {
        self.inputs.clear();
    }

    /// `ExecuteCommands` possession path (`cOperationPurge.cpp:1164-1197`):
    /// fire first (uses the *current* heading), then set move state, then
    /// apply rotate/elevate/accelerate deltas.
    fn apply_inputs(&mut self) {
        let ids: Vec<RobotId> = self.robots.keys().copied().collect();
        let mut requests: Vec<(RobotId, crate::sim::robot::FireRequest)> = Vec::new();

        for id in &ids {
            let input = self.inputs.get(id).copied().unwrap_or_default();
            let r = self.robots.get_mut(id).unwrap();
            if r.is_dead() {
                continue;
            }
            if input.fire {
                if let Some(ty) = r.projectile_fire_type() {
                    if let Some(req) = r.fire(ty, &mut self.rng) {
                        requests.push((*id, req));
                    }
                }
            }
            r.set_move_state(input.throttle);
            r.rotate(input.turn);
            r.elevate(input.elevate);
            r.accelerate(input.accel);
        }

        for (owner, req) in requests {
            self.add_projectile(
                req.projectile_type,
                Some(owner),
                req.position,
                req.speed,
                req.theta,
                req.phi,
            );
        }
    }

    /// PARITY: `cOperationPurge::AddProjectile` (`cOperationPurge.cpp:1424+`) —
    /// spawn one projectile (with its fire/travel particle effects) into the
    /// world. `owner = None` is the observer-point cheat-fire case
    /// (`cOperationPurge.cpp:1211-1218`), where no robot owns the shot.
    pub fn add_projectile(
        &mut self,
        projectile_type: crate::ids::ProjectileTypeId,
        owner: Option<RobotId>,
        position: Vec3,
        speed: f32,
        theta: f32,
        phi: f32,
    ) {
        let idx = projectile_type.get() as usize;
        let (initial_speed, fire_effect, travel_effect) = {
            let pt = &self.game_data.projectile_types[idx];
            (pt.initial_speed, pt.fire_effect, pt.travel_effect)
        };
        let mut proj = Projectile::new(
            projectile_type,
            owner,
            position,
            speed,
            theta,
            phi,
            initial_speed,
        );
        // PARITY: the travel effect is created in the `cProjectile` ctor,
        // before `AddProjectile` plays the fire sound and fire effect.
        if let Some(def) = travel_effect {
            proj.travel_effect = Some(ParticleEffect::new(
                &self.game_data,
                def,
                true,
                proj.position,
                proj.velocity,
                &mut self.rng,
            ));
        }
        self.sound_events.push(SoundEvent {
            kind: SoundKind::Fire(projectile_type),
            origin: proj.position,
        });
        self.spawn_burst_effect(fire_effect, proj.position, proj.velocity);
        self.projectiles.push(proj);
    }

    /// PARITY: `cOperationPurge::TestCollisions` (`cOperationPurge.cpp:804-865`)
    /// — the index-parallel two-phase collision loop. `raCollisions`
    /// (returned) holds the last collision recorded for each robot;
    /// `aCollisionsTemp` (`temp`) drives the re-check gate; robots that
    /// already collided are NOT re-tested (the `time > fCollisionTime`
    /// gate is false once `fCollisionTime` is reset to `1.0`). Up to 11
    /// passes (`nPasses > 10` security break).
    fn test_collisions(&mut self, ids: &[RobotId]) -> Vec<Collision2D> {
        let n = ids.len();
        let mut collisions = vec![Collision2D::none(); n];
        let mut temp = vec![Collision2D::none(); n];

        let mut collision_time = 0.0f32;
        let mut passes = 0u32;
        loop {
            if passes > 10 {
                // PARITY: security break against an infinite loop
                // (`cOperationPurge.cpp:823-828`).
                break;
            }
            let mut b_collision = false;
            for i in 0..n {
                if temp[i].kind == CollisionKind::None || temp[i].ntr.time > collision_time {
                    let nc = self.intersect_robot_objects(ids[i]);
                    temp[i] = nc;
                    if nc.kind != CollisionKind::None {
                        collisions[i] = nc;
                        self.robots.get_mut(&ids[i]).unwrap().add_collision(nc);
                        b_collision = true;
                        collision_time = f_min(collision_time, nc.ntr.time);
                    }
                }
            }
            passes += 1;
            collision_time = 1.0;
            if !b_collision {
                break;
            }
        }
        collisions
    }

    /// PARITY: `cOperationPurge::IntersectRobotObjects`
    /// (`cOperationPurge.cpp:1792-1817`) + the inlined
    /// `cRobot::IntersectBuilding` (`cRobot.cpp:516-533`). Returns the
    /// earliest-time collision against any other robot or the building.
    fn intersect_robot_objects(&self, id: RobotId) -> Collision2D {
        let robot = &self.robots[&id];
        let mut collision = Collision2D::none();

        for other in self.robots.values() {
            let nc = robot.intersect_robot(other);
            if nc.kind != CollisionKind::None && nc.ntr.time < collision.ntr.time {
                collision = nc;
            }
        }

        let ntr = self.building.intersect_moving_rect(robot.moving_rect());
        if ntr.hit && ntr.time < collision.ntr.time {
            collision = Collision2D {
                ntr,
                kind: CollisionKind::Wall,
                robot_id: None,
                wall_axis: ntr.plane.axis,
                vec2_direction: Vec2::ZERO,
            };
        }
        collision
    }

    /// PARITY: the "Move projectiles" pass of `MoveObjects`
    /// (`cOperationPurge.cpp:702-736`) + `IntersectProjectileObjects`
    /// (`cOperationPurge.cpp:1820-1830`). In-order retain removal (never
    /// `swap_remove`).
    fn step_projectiles(&mut self, dt: f32) {
        let projectiles = std::mem::take(&mut self.projectiles);
        let mut survivors = Vec::with_capacity(projectiles.len());

        for mut proj in projectiles {
            let idx = proj.projectile_type.get() as usize;
            let gravity = self.game_data.projectile_types[idx].gravity_accel;
            let collision_effect = self.game_data.projectile_types[idx].collision_effect;
            proj.step(dt, gravity);

            // M9: drive the trail effect to the projectile's new state so
            // the emitter interpolates spawns along the traveled segment
            // (gap-free at any game speed).
            if let Some(eff) = proj.travel_effect.as_mut() {
                eff.move_with(
                    dt,
                    proj.position,
                    proj.velocity,
                    &self.game_data,
                    &mut self.particles,
                    &mut self.rng,
                );
            }

            // IntersectProjectileObjects: every robot, then the building.
            for r in self.robots.values() {
                proj.intersect_robot(r.moving_box(), r.id);
            }
            let ntr = self.building.intersect_trajectory(proj.trajectory);
            proj.intersect_building(ntr);

            let collision = proj.collision();
            match collision.kind {
                CollisionKind::None => {
                    survivors.push(proj);
                    continue;
                }
                CollisionKind::Robot => {
                    if let Some(rid) = collision.robot_id {
                        let damage = proj.get_damage(&self.game_data, &mut self.rng);
                        if let Some(r) = self.robots.get_mut(&rid) {
                            r.projectile_hit(damage);
                        }
                    }
                }
                CollisionKind::Wall => {
                    if let Some((position, normal)) = proj.hole_placement() {
                        let size = self.game_data.projectile_types[idx].hole_size;
                        let rotation = self.rng.frand() * crate::math::d3dx::D3DX_PI * 2.0;
                        self.holes.push(Hole {
                            position,
                            normal,
                            rotation,
                            size,
                        });
                    }
                }
            }
            self.sound_events.push(SoundEvent {
                kind: SoundKind::Collision(proj.projectile_type),
                origin: proj.position,
            });
            if let Some((position, velocity)) = proj.collision_effect_placement() {
                self.spawn_burst_effect(collision_effect, position, velocity);
            }
            // Dropping the projectile drops its travel effect with it.
        }

        self.projectiles = survivors;
    }

    /// M9: spawn a **static** burst effect (fire/collision/destroy) at
    /// `position` carrying `velocity`, if `def` is set. Static so `frame`
    /// prunes it once every emitter has expired.
    fn spawn_burst_effect(&mut self, def: Option<EffectDefId>, position: Vec3, velocity: Vec3) {
        if let Some(def) = def {
            let eff = ParticleEffect::new(&self.game_data, def, false, position, velocity, &mut self.rng);
            self.effects.push(eff);
        }
    }

    /// PARITY: `cOperationPurge::RemoveDestroyedRobots`
    /// (`cOperationPurge.cpp:778-800`). Headless has no active robot to
    /// protect, so every dead robot is removed (the original keeps a
    /// dead active robot alive only to preserve a raw pointer; the
    /// win/lose accounting in `GetGameStatus` treats it as dead either
    /// way). PlayDestroySound is an M10 hook; the destroy effect is M9.
    fn remove_destroyed_robots(&mut self) {
        let dead: Vec<RobotId> = self
            .robots
            .iter()
            .filter(|(_, r)| r.is_dead())
            .map(|(id, _)| *id)
            .collect();
        for id in dead {
            let r = self.robots.remove(&id).expect("collected from the map");
            self.sound_events.push(SoundEvent {
                kind: SoundKind::Destroy,
                origin: Vec3::new(r.position.x, r.position.y, 0.0),
            });
            // PARITY: `cRobot::DisplayDestroyEffect` (`cRobot.cpp:419-423`)
            // emits from the robot's center with zero velocity.
            let def = self.game_data.robot_types[r.robot_type.get() as usize].destroy_effect;
            let center = Vec3::new(r.position.x, r.position.y, crate::sim::robot::ROBOT_CENTER_Z);
            self.spawn_burst_effect(def, center, Vec3::ZERO);
        }
    }

    /// PARITY: `cOperationPurge::Transfer` (`cOperationPurge.cpp:868-877`)
    /// — every center transfers, iterated in ascending-`CenterId` order.
    fn transfer(&mut self, dt: f32) {
        for i in 0..self.centers.len() {
            self.center_transfer(i, dt);
        }
    }

    /// PARITY: `cCenter::ChooseNewTransferRobot`/`GetTransferRobot` +
    /// per-kind `Transfer` (`cCenter.cpp:108-172,232-324`).
    fn center_transfer(&mut self, i: usize, dt: f32) {
        // ChooseNewTransferRobot (`cCenter.cpp:144-172`).
        let choose_new = {
            let c = &self.centers[i];
            if c.transferring {
                match c.transfer_robot.and_then(|rid| self.robots.get(&rid)) {
                    Some(r) => !c.is_transfer_possible(r),
                    None => true,
                }
            } else {
                true
            }
        };
        if choose_new {
            let new_robot = self.get_new_transfer_robot(i);
            self.centers[i].transfer_robot = new_robot;
            let valid = match self.centers[i]
                .transfer_robot
                .and_then(|rid| self.robots.get(&rid))
            {
                Some(r) => self.centers[i].is_transfer_possible(r),
                None => false,
            };
            if valid {
                self.centers[i].transferring = true;
            } else {
                self.centers[i].transfer_robot = None;
                self.centers[i].transferring = false;
            }
        }

        // GetTransferRobot (`cCenter.cpp:108-128`).
        // GUARD (design §1.3): the original could deref end()/begin() on an
        // empty map here — the port resolves through the map and yields
        // `None`, same observable outcome.
        let rid = if self.centers[i].transferring {
            match self.centers[i].transfer_robot {
                Some(rid) if self.robots.contains_key(&rid) => Some(rid),
                _ => {
                    self.centers[i].transferring = false;
                    self.centers[i].transfer_robot = None;
                    None
                }
            }
        } else {
            None
        };
        let rid = match rid {
            Some(r) => r,
            None => return,
        };

        // Per-kind transfer.
        match self.centers[i].kind.clone() {
            CenterKind::Energy { rate } => {
                if let Some(r) = self.robots.get_mut(&rid) {
                    r.increase_energy(dt / rate);
                }
            }
            CenterKind::Health { rate } => {
                if let Some(r) = self.robots.get_mut(&rid) {
                    r.increase_health(dt / rate);
                }
            }
            CenterKind::Ammo { rates } => {
                // GetProjectileRestockType (`cCenter.cpp:241-245`).
                let restock = self
                    .robots
                    .get(&rid)
                    .and_then(|r| r.projectile_restock_type());
                let ty = match restock {
                    Some(t) => t,
                    None => return,
                };

                let mut te = dt;
                if self.centers[i].last_transfer_robot == Some(rid)
                    && self.centers[i].last_transfer_type == Some(ty)
                {
                    te += self.centers[i].time_left;
                }

                // PARITY (design §1.3): a projectile type with no listed
                // rate transfers at rate 0.0 => `te / 0.0 = +inf`; the
                // `(UINT)` truncation is reproduced with `.floor()` on a
                // NON-negative value, and the +inf flows through unguarded
                // (`increase_ammo`'s `f_min(left + inf, start) == start`
                // fills the magazine). NO guard code (a "guard to
                // f32::INFINITY" would be a no-op).
                let rate = rates
                    .iter()
                    .find(|(t, _)| *t == ty)
                    .map(|(_, r)| *r)
                    .unwrap_or(0.0);
                let count = (te / rate).floor();
                if let Some(r) = self.robots.get_mut(&rid) {
                    r.increase_ammo(ty, count);
                }

                self.centers[i].last_transfer_robot = Some(rid);
                self.centers[i].last_transfer_type = Some(ty);
                self.centers[i].time_left = te - count * rate;
            }
        }
    }

    /// PARITY: `cOperationPurge::GetNewTransferRobot`
    /// (`cOperationPurge.cpp:1530-1544`) — first robot (ascending id) with
    /// which the center can transfer.
    fn get_new_transfer_robot(&self, i: usize) -> Option<RobotId> {
        let c = &self.centers[i];
        for (id, r) in self.robots.iter() {
            if c.is_transfer_possible(r) {
                return Some(*id);
            }
        }
        None
    }

    // ================= win/lose/draw =================

    /// PARITY: `cOperationPurge::GetGameStatus` (`cOperationPurge.cpp:445-516`).
    pub fn status(&self) -> Status {
        let mut active_team_alive = false;
        let mut active_team_won = true;
        let mut all_dead = true;

        let n_teams = self.game_definition().teams.len();
        for t in 0..n_teams {
            let unspawned = !self.spawn_info[t].finished_spawning;
            if TeamId(t as u32) == self.active_team {
                if unspawned {
                    active_team_alive = true;
                    all_dead = false;
                }
            } else if unspawned {
                active_team_won = false;
                all_dead = false;
            }
        }

        for r in self.robots.values() {
            if !r.is_dead() {
                all_dead = false;
                if r.team == self.active_team {
                    active_team_alive = true;
                } else {
                    active_team_won = false;
                }
            }
        }

        if active_team_alive && active_team_won {
            return Status::Won;
        }
        if !active_team_alive && !all_dead {
            return Status::Lost;
        }
        if all_dead {
            return Status::Draw;
        }
        Status::None
    }

    // ================= spawn scheduling =================

    /// PARITY: `cOperationPurge::SpawnRobots` (`cOperationPurge.cpp:2235-2309`).
    fn spawn_robots(&mut self, team: usize, dt: f32) {
        // A finished team does no further RNG/state work — bail before the
        // per-frame clones below (the flag is persisted, so this is exact).
        if self.spawn_info[team].finished_spawning {
            return;
        }
        let groups = self.game_definition().teams[team].robot_groups.clone();
        if groups.is_empty() {
            self.spawn_info[team].finished_spawning = true;
            return;
        }

        let mut info = self.spawn_info[team].clone();
        let mut group = groups[info.current_group].clone();

        if info.robots_left == 0 && info.current_group == groups.len() - 1 {
            info.finished_spawning = true;
        }
        if info.finished_spawning {
            self.spawn_info[team] = info;
            return;
        }

        info.next_spawn -= dt;
        while info.next_spawn <= 0.0 {
            let ai_idx = group.ai_robot_type.expect("robot group has no AI type").get() as usize;
            let min_wall_dist = self.game_data.ai_robot_types[ai_idx].min_wall_dist;
            let size = Vec2::new(min_wall_dist, min_wall_dist);

            // Placement.
            let position = if group.spawn_from_point {
                let p = info.spawn_point;
                let rc = Rect { center: p, size };
                if self.intersect_rect_objects(None, rc) {
                    // If placed, the robot would intersect existing objects.
                    self.spawn_info[team] = info;
                    return;
                }
                p
            } else {
                self.get_rect_position(size)
            };

            let robot_type = group.robot_type.expect("robot group has no robot type");
            let id = RobotId(self.id_gen.next());
            let options: RobotType = self.game_data.robot_types[robot_type.get() as usize].clone();
            let robot = Robot::new(robot_type, TeamId(team as u32), id, position, &options);
            self.robots.insert(id, robot);

            info.next_spawn += self.rand_variance(group.spawn_delay, group.spawn_delay_variance);
            info.robots_left -= 1;
            if info.robots_left == 0 {
                if info.current_group == groups.len() - 1 {
                    info.finished_spawning = true;
                    self.spawn_info[team] = info;
                    return;
                } else {
                    info.current_group += 1;
                    group = groups[info.current_group].clone();
                    info.robots_left = group.count;
                    info.next_spawn +=
                        self.rand_variance(group.initial_delay, group.initial_delay_variance);
                }
            }
        }

        self.spawn_info[team] = info;
    }

    /// PARITY: `cOperationPurge::GetRectPosition` (`cOperationPurge.cpp:2347-2369`)
    /// — 1000-try rejection sampling inside the building extents.
    fn get_rect_position(&mut self, size: Vec2) -> Vec2 {
        let building = self.building.size();
        let mut tries = 0u32;
        loop {
            let x = self.rng.frand() * building.x;
            let y = self.rng.frand() * building.y;
            let rc = Rect {
                center: Vec2::new(x, y),
                size,
            };
            tries += 1;
            if tries >= crate::sim::spawn::SPAWN_PLACEMENT_TRIES {
                // Could not find an empty position (`cOperationPurge.cpp:2362`).
                return Vec2::ZERO;
            }
            if !self.intersect_rect_objects(None, rc) {
                return Vec2::new(x, y);
            }
        }
    }

    /// PARITY: `cOperationPurge::IntersectRectObjects`
    /// (`cOperationPurge.cpp:1731-1754`). `exclude` is the robot to skip
    /// (`idInvalidRobot` => `None` => exclude nothing).
    fn intersect_rect_objects(&self, exclude: Option<RobotId>, rc: Rect) -> bool {
        for (id, r) in self.robots.iter() {
            if Some(*id) != exclude && rect_rect(rc, r.bounding_rect()) {
                return true;
            }
        }
        self.building.intersect_rect(rc)
    }

    /// PARITY: `cOperationPurge::InitializeCenterPositions`
    /// (`cOperationPurge.cpp:2372-2427`). For each room, gather the walls
    /// bordering it and pick ONE at random as a candidate center slot.
    fn initialize_center_positions(&mut self) {
        self.center_positions.clear();
        let cx = self.building.maze.cx;
        let cy = self.building.maze.cy;
        let room_size = self.building.room_size;

        for wall_y in 0..cy {
            for wall_x in 0..cx {
                let mut candidates: Vec<crate::sim::spawn::CenterPosition> = Vec::new();
                let base = Vec2::new(wall_x as f32 * room_size, wall_y as f32 * room_size);

                if self.building.wall_at(WallDir::X, wall_x, wall_y) {
                    candidates.push(crate::sim::spawn::CenterPosition {
                        position: base,
                        orientation: Orientation::Up,
                    });
                }
                if self.building.wall_at(WallDir::Y, wall_x, wall_y) {
                    candidates.push(crate::sim::spawn::CenterPosition {
                        position: base,
                        orientation: Orientation::Left,
                    });
                }
                if self.building.wall_at(WallDir::X, wall_x, wall_y + 1) {
                    candidates.push(crate::sim::spawn::CenterPosition {
                        position: base,
                        orientation: Orientation::Down,
                    });
                }
                if self.building.wall_at(WallDir::Y, wall_x + 1, wall_y) {
                    candidates.push(crate::sim::spawn::CenterPosition {
                        position: base,
                        orientation: Orientation::Right,
                    });
                }

                let n = candidates.len();
                if n != 0 {
                    let chosen = self.rand_index(n);
                    self.center_positions.push(candidates[chosen]);
                }
            }
        }
    }

    /// PARITY: `cOperationPurge::CreateCenterGroups`
    /// (`cOperationPurge.cpp:2312-2343`) for all three center kinds.
    fn create_center_groups(&mut self) {
        // Snapshot the group lists (they don't change during creation).
        let ammo_groups = self.game_definition().ammo_center_groups.clone();
        let energy_groups = self.game_definition().energy_center_groups.clone();
        let health_groups = self.game_definition().health_center_groups.clone();

        for group in &ammo_groups {
            let idx = group.center_type.expect("center group has no type") as usize;
            for _ in 0..group.count {
                let ty = self.game_data.center_types.ammo[idx].clone();
                let (position, orientation) = self.get_center_position(ty.size);
                let id = CenterId(self.center_id_gen.next());
                let rates = ty
                    .transfer_rates
                    .iter()
                    .filter_map(|tr| tr.projectile_type.map(|pt| (pt, tr.transfer_rate)))
                    .collect();
                self.centers.push(Center::new(
                    id,
                    CenterKind::Ammo { rates },
                    position,
                    ty.size,
                    ty.color,
                    orientation,
                ));
            }
        }
        for group in &energy_groups {
            let idx = group.center_type.expect("center group has no type") as usize;
            for _ in 0..group.count {
                let ty = self.game_data.center_types.energy[idx].clone();
                let (position, orientation) = self.get_center_position(ty.size);
                let id = CenterId(self.center_id_gen.next());
                self.centers.push(Center::new(
                    id,
                    CenterKind::Energy {
                        rate: ty.transfer_rate,
                    },
                    position,
                    ty.size,
                    ty.color,
                    orientation,
                ));
            }
        }
        for group in &health_groups {
            let idx = group.center_type.expect("center group has no type") as usize;
            for _ in 0..group.count {
                let ty = self.game_data.center_types.health[idx].clone();
                let (position, orientation) = self.get_center_position(ty.size);
                let id = CenterId(self.center_id_gen.next());
                self.centers.push(Center::new(
                    id,
                    CenterKind::Health {
                        rate: ty.transfer_rate,
                    },
                    position,
                    ty.size,
                    ty.color,
                    orientation,
                ));
            }
        }
    }

    /// PARITY: `cOperationPurge::GetCenterPosition`
    /// (`cOperationPurge.cpp:2431-2473`). Picks and removes a candidate
    /// slot, then insets the center by its size + half wall width.
    fn get_center_position(&mut self, size: f32) -> (Vec2, Orientation) {
        let n = self.center_positions.len();
        if n == 0 {
            // The building is not large enough (`cOperationPurge.cpp:2436`).
            return (Vec2::ZERO, Orientation::Up);
        }
        let chosen = self.rand_index(n);
        let slot = self.center_positions.remove(chosen);

        let room_size = self.building.room_size;
        let wall_width = self.building.wall_width;
        let mut pos = slot.position;
        match slot.orientation {
            Orientation::Right => {
                pos.x += room_size - size * 0.5 - wall_width * 0.5;
                pos.y += size * 0.5 + self.rng.frand() * (room_size - size);
            }
            Orientation::Up => {
                pos.x += size * 0.5 + self.rng.frand() * (room_size - size);
                pos.y += size * 0.5 + wall_width * 0.5;
            }
            Orientation::Left => {
                pos.x += size * 0.5 + wall_width * 0.5;
                pos.y += size * 0.5 + self.rng.frand() * (room_size - size);
            }
            Orientation::Down => {
                pos.x += size * 0.5 + self.rng.frand() * (room_size - size);
                pos.y += room_size - size * 0.5 - wall_width * 0.5;
            }
        }
        (pos, slot.orientation)
    }

    /// PARITY: `fRandVariance` (`Functions.cpp:96-99`) — `median + rand01 *
    /// variance*2 - variance`. Deviation: the original divides `rand()` by
    /// `RAND_MAX` (inclusive of 1.0); the port's `frand()` is `[0,1)` (design
    /// §1.5.11 — no msvcrt bit-compat claimed, only the same coarseness).
    fn rand_variance(&mut self, median: f32, variance: f32) -> f32 {
        median + self.rng.frand() * variance * 2.0 - variance
    }

    /// A uniform integer pick in `[0, n)`, replacing the original's
    /// `rand() % n` (`cOperationPurge.cpp:2422,2441`). Deviation: uses the
    /// shared `frand()` facade (deterministic; no msvcrt bit-compat,
    /// design §1.5.11). `n` must be nonzero.
    fn rand_index(&mut self, n: usize) -> usize {
        ((self.rng.frand() * n as f32) as usize).min(n - 1)
    }
}
