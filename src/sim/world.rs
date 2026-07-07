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
use crate::maze::building::Building;
use crate::math::floor_i32;
use crate::math::intersect::{
    moving_rect_rect, no_intersection_2d, no_intersection_3d, rect_rect, trajectory_box,
    trajectory_plane, Box3, Intersection2D, Intersection3D, MovingRect, Plane3D, PlaneAxis, Rect,
};
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

/// Which of the two wall directions a wall runs along (`eWallDirection`,
/// `cMaze.h`). `X` = horizontal (top/bottom room edge), `Y` = vertical
/// (left/right room edge).
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum WallDir {
    X,
    Y,
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

        // M9: move + expire particles (the original stepped particles here
        // in MoveObjects). No RNG/data needed — kinematics live on each
        // particle; expired slots are freed.
        self.particles.move_all(dt);

        // Move projectiles + resolve hits (`cOperationPurge.cpp:702-736`).
        // Projectile trail (travel) effects and per-hit collision effects
        // are driven inside `step_projectiles` (M9).
        self.step_projectiles(dt);

        // M9: advance static burst effects (fire/collision/destroy) and drop
        // finished ones. `mem::take` sidesteps the split borrow of
        // `self.effects` + `self.particles`/`self.rng`.
        let mut effects = std::mem::take(&mut self.effects);
        for eff in effects.iter_mut() {
            eff.move_self(dt, &self.game_data, &mut self.particles, &mut self.rng);
        }
        // PARITY: dynamic effects never self-prune (they never reach
        // `is_finished`); only static burst effects are removed here.
        effects.retain(|e| !e.is_finished());
        self.effects = effects;

        // RemoveDestroyedRobots -> Transfer (`cOperationPurge.cpp:1239-1240`).
        self.remove_destroyed_robots();
        self.transfer(dt);

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
            let idx = req.projectile_type.get() as usize;
            let (initial_speed, fire_effect, travel_effect) = {
                let pt = &self.game_data.projectile_types[idx];
                (pt.initial_speed, pt.fire_effect, pt.travel_effect)
            };
            let mut proj = Projectile::new(
                req.projectile_type,
                Some(owner),
                req.position,
                req.speed,
                req.theta,
                req.phi,
                initial_speed,
            );
            // M9: fire effect — a static burst at the muzzle carrying the
            // muzzle velocity (so a relative emit cone points down-barrel).
            if let Some(def) = fire_effect {
                let eff = ParticleEffect::new(
                    &self.game_data,
                    def,
                    false,
                    proj.position,
                    proj.velocity,
                    &mut self.rng,
                );
                self.effects.push(eff);
            }
            // M9: travel effect — a dynamic trail that follows the projectile
            // (never self-prunes; dropped with the projectile on destroy).
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
            self.projectiles.push(proj);
        }
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

        let ntr = building_intersect_moving_rect(&self.building, robot.moving_rect());
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
            let ntr = building_intersect_trajectory(&self.building, proj.trajectory);
            proj.intersect_building(ntr);

            let collision = proj.collision();
            // M9: the impact point (on the swept segment) + the projectile's
            // velocity there, for the collision burst effect.
            let impact = crate::math::accelerate_point(
                collision.ntr.time,
                proj.trajectory.start,
                proj.trajectory.direction,
                proj.trajectory.gravity,
            );
            let impact_vel = proj.velocity;
            match collision.kind {
                CollisionKind::None => survivors.push(proj),
                CollisionKind::Robot => {
                    if let Some(rid) = collision.robot_id {
                        let damage = proj.get_damage(&self.game_data, &mut self.rng);
                        if let Some(r) = self.robots.get_mut(&rid) {
                            r.projectile_hit(damage);
                        }
                    }
                    // M9: collision burst. The projectile (with its trail) is
                    // dropped here — the dynamic trail dies with its owner.
                    self.spawn_burst_effect(collision_effect, impact, impact_vel);
                }
                CollisionKind::Wall => {
                    // DisplayHole (`cProjectile.cpp:237-300`).
                    if let Some((position, normal)) = proj.hole_placement() {
                        self.holes.push(Hole { position, normal });
                    }
                    // M9: collision burst; the projectile is destroyed.
                    self.spawn_burst_effect(collision_effect, impact, impact_vel);
                }
            }
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
        // M9: collect each dead robot's destroy effect (at its 3D center,
        // carrying its 2D velocity) before removal.
        let mut bursts: Vec<(EffectDefId, Vec3, Vec3)> = Vec::new();
        for r in self.robots.values() {
            if r.is_dead() {
                if let Some(def) =
                    self.game_data.robot_types[r.robot_type.get() as usize].destroy_effect
                {
                    let vel = r.velocity();
                    bursts.push((
                        def,
                        Vec3::new(r.position.x, r.position.y, crate::sim::robot::ROBOT_CENTER_Z),
                        Vec3::new(vel.x, vel.y, 0.0),
                    ));
                }
            }
        }
        self.robots.retain(|_, r| !r.is_dead());
        for (def, position, velocity) in bursts {
            self.spawn_burst_effect(Some(def), position, velocity);
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
        let building = building_size(&self.building);
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
        building_intersect_rect(&self.building, rc)
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

                if wall_at(&self.building, WallDir::X, wall_x, wall_y) {
                    candidates.push(crate::sim::spawn::CenterPosition {
                        position: base,
                        orientation: Orientation::Up,
                    });
                }
                if wall_at(&self.building, WallDir::Y, wall_x, wall_y) {
                    candidates.push(crate::sim::spawn::CenterPosition {
                        position: base,
                        orientation: Orientation::Left,
                    });
                }
                if wall_at(&self.building, WallDir::X, wall_x, wall_y + 1) {
                    candidates.push(crate::sim::spawn::CenterPosition {
                        position: base,
                        orientation: Orientation::Down,
                    });
                }
                if wall_at(&self.building, WallDir::Y, wall_x + 1, wall_y) {
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

// ===================== building collision helpers =====================
// PARITY: ported from `cBuilding.cpp`. Kept as free functions over
// `&Building` (rather than methods on `maze::building`) so M6 does not have
// to touch the render-track `maze/building.rs` file; wall existence is read
// through `Maze::surrounding_walls` (the GUARD out-of-grid=no-wall accessor,
// design §1.3).

/// `cMaze::GetWall(x, y, dir)` for the two wall directions.
/// `WallDir::X` = `surrounding_walls[top]` = `h_wall(x,y)`;
/// `WallDir::Y` = `surrounding_walls[left]` = `v_wall(x,y)`.
fn wall_at(building: &Building, dir: WallDir, x: i32, y: i32) -> bool {
    let sw = building.maze.surrounding_walls(x, y);
    match dir {
        WallDir::X => sw[0], // top edge of room (x,y)
        WallDir::Y => sw[2], // left edge of room (x,y)
    }
}

/// `cBuilding::GetSize` (`cBuilding.cpp:52-56`).
fn building_size(building: &Building) -> Vec3 {
    Vec3::new(
        building.maze.cx as f32 * building.room_size,
        building.maze.cy as f32 * building.room_size,
        building.ceiling_height,
    )
}

/// `cBuilding::WallToRect` (`cBuilding.cpp:96-119`).
fn wall_to_rect(building: &Building, dir: WallDir, x: i32, y: i32) -> Rect {
    let rs = building.room_size;
    let ww = building.wall_width;
    match dir {
        WallDir::X => Rect {
            center: Vec2::new((x as f32 + 0.5) * rs, y as f32 * rs),
            size: Vec2::new(rs + ww, ww),
        },
        WallDir::Y => Rect {
            center: Vec2::new(x as f32 * rs, (y as f32 + 0.5) * rs),
            size: Vec2::new(ww, rs + ww),
        },
    }
}

/// `cBuilding::WallToBox` (`cBuilding.cpp:122-147`).
fn wall_to_box(building: &Building, dir: WallDir, x: i32, y: i32) -> Box3 {
    let rs = building.room_size;
    let ww = building.wall_width;
    let ch = building.ceiling_height;
    match dir {
        WallDir::X => Box3 {
            center: Vec3::new((x as f32 + 0.5) * rs, y as f32 * rs, 0.5 * ch),
            size: Vec3::new(rs + ww, ww, ch),
        },
        WallDir::Y => Box3 {
            center: Vec3::new(x as f32 * rs, (y as f32 + 0.5) * rs, 0.5 * ch),
            size: Vec3::new(ww, rs + ww, ch),
        },
    }
}

/// PARITY: `cBuilding::IntersectRect` (`cBuilding.cpp:164-208`) — static
/// rect-vs-wall test used by spawn placement. Preserves the original's
/// quirk of using `size2Size.y` for BOTH the x and y bounds
/// (`cBuilding.cpp:170-173`; harmless because spawn rects are square).
fn building_intersect_rect(building: &Building, rc: Rect) -> bool {
    let recip = 1.0 / building.room_size;
    let size_y = (rc.size.y + building.wall_width) * 0.5;
    let iwx1 = floor_i32((rc.center.x - size_y) * recip);
    let iwy1 = floor_i32((rc.center.y - size_y) * recip);
    let iwx2 = floor_i32((rc.center.x + size_y) * recip);
    let iwy2 = floor_i32((rc.center.y + size_y) * recip);

    // Walls in the x direction.
    for y in (iwy1 + 1).max(0)..=iwy2 {
        for x in iwx1.max(0)..=iwx2 {
            if wall_at(building, WallDir::X, x, y)
                && rect_rect(rc, wall_to_rect(building, WallDir::X, x, y))
            {
                return true;
            }
        }
    }
    // Walls in the y direction.
    for y in iwy1.max(0)..=iwy2 {
        for x in (iwx1 + 1).max(0)..=iwx2 {
            if wall_at(building, WallDir::Y, x, y)
                && rect_rect(rc, wall_to_rect(building, WallDir::Y, x, y))
            {
                return true;
            }
        }
    }
    false
}

/// PARITY: `cBuilding::IntersectMovingRect` -> `GetIntersectedWalls`
/// (`cBuilding.cpp:211-216,451-460`) with `pntr` set, `pvWalls` null =>
/// `bFindAll = true`: returns the earliest-time wall intersection.
fn building_intersect_moving_rect(building: &Building, mrc: MovingRect) -> Intersection2D {
    let mut ntr = no_intersection_2d();
    let mut walls: Vec<(WallDir, i32, i32)> = Vec::new();
    get_intersected_walls_portion(building, mrc, false, true, &mut walls, &mut ntr, 0);
    ntr
}

/// PARITY: `cBuilding::IntersectTrajectory` (`cBuilding.cpp:219-273`).
fn building_intersect_trajectory(
    building: &Building,
    traj: crate::math::intersect::Trajectory,
) -> Intersection3D {
    // The moving rect spans the trajectory's 2D footprint (size 0).
    let end =
        crate::math::accelerate_point(traj.travel_time, traj.start, traj.direction, traj.gravity);
    let mrc = MovingRect {
        center1: traj.start.truncate(),
        center2: end.truncate(),
        size: Vec2::ZERO,
        stop_time: 1.0,
    };
    let mut walls: Vec<(WallDir, i32, i32)> = Vec::new();
    let mut ntr2 = no_intersection_2d();
    get_intersected_walls_portion(building, mrc, true, false, &mut walls, &mut ntr2, 0);

    let mut ntr = no_intersection_3d();
    for &(dir, x, y) in &walls {
        let bx = wall_to_box(building, dir, x, y);
        let n = trajectory_box(traj, bx);
        if n.hit && n.time < ntr.time {
            ntr = n;
        }
    }

    let size = building_size(building);
    // Floor plane.
    let floor = Plane3D {
        axis: PlaneAxis::Z,
        center: Vec3::new(size.x * 0.5, size.y * 0.5, 0.0),
        size,
    };
    let n = trajectory_plane(traj, floor);
    if n.hit && n.time < ntr.time {
        ntr = n;
    }
    // Ceiling plane.
    let ceiling = Plane3D {
        axis: PlaneAxis::Z,
        center: Vec3::new(size.x * 0.5, size.y * 0.5, building.ceiling_height),
        size,
    };
    let n = trajectory_plane(traj, ceiling);
    if n.hit && n.time < ntr.time {
        ntr = n;
    }

    ntr
}

/// PARITY: `cBuilding::GetIntersectedWallsPortion`
/// (`cBuilding.cpp:467-602`). Both call sites pass `bFindAll = true`, so
/// this always visits every sub-segment and every candidate wall (the
/// early-return `&& !bFindAll` short-circuits never fire); `want_walls`
/// collects the intersected walls, `want_ntr` keeps the earliest-time
/// intersection. Recursion depth is capped at 3 (`iLevel > 3`).
#[allow(clippy::too_many_arguments)]
fn get_intersected_walls_portion(
    building: &Building,
    mrc: MovingRect,
    want_walls: bool,
    want_ntr: bool,
    walls: &mut Vec<(WallDir, i32, i32)>,
    ntr: &mut Intersection2D,
    level: u32,
) {
    let offset_x = (mrc.size.x + building.wall_width) * 0.5;
    let offset_y = (mrc.size.y + building.wall_width) * 0.5;
    let recip = 1.0 / building.room_size;
    let fwx1 = (mrc.center1.x.min(mrc.center2.x) - offset_x) * recip;
    let fwy1 = (mrc.center1.y.min(mrc.center2.y) - offset_y) * recip;
    let fwx2 = (mrc.center1.x.max(mrc.center2.x) + offset_x) * recip;
    let fwy2 = (mrc.center1.y.max(mrc.center2.y) + offset_y) * recip;
    let iwx1 = floor_i32(fwx1);
    let iwy1 = floor_i32(fwy1);
    let iwx2 = floor_i32(fwx2);
    let iwy2 = floor_i32(fwy2);

    if iwx1 == iwx2 || iwy1 == iwy2 || level > 3 {
        let cx = building.maze.cx;
        let cy = building.maze.cy;
        if iwx1 >= cx || iwx2 < 0 || iwy1 >= cy || iwy2 < 0 {
            // Both points are outside of the maze.
            return;
        }

        // Walls in the x direction.
        for y in (iwy1 + 1)..=iwy2 {
            for x in iwx1..=iwx2 {
                if wall_at(building, WallDir::X, x, y) {
                    let n = moving_rect_rect(mrc, wall_to_rect(building, WallDir::X, x, y));
                    if !n.hit {
                        continue;
                    }
                    if want_walls {
                        walls.push((WallDir::X, x, y));
                    }
                    if want_ntr && n.time < ntr.time {
                        *ntr = n;
                    }
                }
            }
        }
        // Walls in the y direction.
        for y in iwy1..=iwy2 {
            for x in (iwx1 + 1)..=iwx2 {
                if wall_at(building, WallDir::Y, x, y) {
                    let n = moving_rect_rect(mrc, wall_to_rect(building, WallDir::Y, x, y));
                    if !n.hit {
                        continue;
                    }
                    if want_walls {
                        walls.push((WallDir::Y, x, y));
                    }
                    if want_ntr && n.time < ntr.time {
                        *ntr = n;
                    }
                }
            }
        }
    } else {
        // Split the segment into four and recurse.
        let movement = mrc.center2 - mrc.center1;
        for (a, b) in [(0.0, 0.25), (0.25, 0.5), (0.5, 0.75), (0.75, 1.0)] {
            let sub = MovingRect {
                center1: mrc.center1 + movement * a,
                center2: mrc.center1 + movement * b,
                size: mrc.size,
                stop_time: mrc.stop_time,
            };
            get_intersected_walls_portion(building, sub, want_walls, want_ntr, walls, ntr, level + 1);
        }
    }
}
