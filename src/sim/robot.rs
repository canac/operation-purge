//! `cRobot`: two-phase kinematics, throttle, energy, firing, crash damage,
//! ported verbatim from `cRobot.cpp`.
//!
//! PARITY (preserved verbatim):
//! - Fire-alignment gate `pi * 0.025` with **no 2π wrap** (`cRobot.cpp:366`).
//! - Turning is instantaneous: `m_angDirection = m_angDirectionDesired`,
//!   the rate-limit code commented out (`cRobot.cpp:117-120`).
//! - `GetMoveAccel` returns velocity deltas already ×dt, fed into ½at²
//!   kinematics — movement feel is frame-rate coupled (`cRobot.cpp:127`).
//! - Fire/restock type selection = **last** matching entry wins
//!   (`cRobot.cpp:427-458`).
//! - Collision back-off of `0.005` units (`cRobot.cpp:237`).

use crate::data::RobotType;
use crate::ids::{ProjectileTypeId, RobotId, RobotTypeId, TeamId};
use crate::math::intersect::{
    Intersection2D, MovingBox, MovingRect, PlaneAxis, Rect, no_intersection_2d,
};
use crate::math::{equals, f_clamp, f_max, f_min, length2d, sign};
use crate::rng::GameRng;
use crate::sim::ai::AiRuntime;
use glam::{Vec2, Vec3};
use std::f32::consts::PI;

/// PARITY: `Make2D (cRobot3D::GetSize ())` = `(fBarrelLength*2, fBarrelLength*2)`
/// (`cRobot3D.cpp:66-68` + `data::builtin::OBJECTS`) — the robot's collision
/// footprint. barrel_length 0.8 → 1.6 × 1.6.
pub const ROBOT_SIZE_XY: f32 = 0.8 * 2.0;

/// PARITY: robot 3D box height `fBodyHeight + fTreadRadius*2`
/// (`cRobot3D.cpp:68`): 0.5 + 0.2*2 = 0.9.
pub const ROBOT_SIZE_Z: f32 = 0.5 + 0.2 * 2.0;

/// PARITY: `cRobot3D::GetCenter ()` = `(0, 0, size.z * 0.5)` (`cRobot3D.cpp:40`).
pub const ROBOT_CENTER_Z: f32 = ROBOT_SIZE_Z * 0.5;

/// The three throttle modes. Maps 1:1 to `cRobot::eMoveState`
/// (`moveAccelerate`/`moveCruise`/`moveDecelerate`); `Brake` == `moveDecelerate`.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Throttle {
    Accelerate,
    Cruise,
    Brake,
}

/// `cRobot::tProjectileTypeState` (`cRobot.h:103-110`). `projectiles_left`
/// is `f32` (not the original `UINT`) so the unlisted-ammo `+inf` transfer
/// quirk is representable without a guard.
#[derive(Clone, Debug)]
pub struct ProjectileTypeState {
    pub projectile_type: ProjectileTypeId,
    pub projectiles_left: f32,
    pub start_count: f32,
    pub fire_delay: f32,
    pub fire_rate: f32,
    /// `tProjectileData::angAccuracy` (already radians, `data::values`).
    pub accuracy: f32,
}

impl ProjectileTypeState {
    /// PARITY: `cRobot::tProjectileTypeState::FireReady` (`cRobot.cpp:838-841`)
    /// — `nProjectilesLeft > 0 && Equals (fFireDelay, 0.0f)`.
    pub fn fire_ready(&self) -> bool {
        self.projectiles_left > 0.0 && equals(self.fire_delay, 0.0)
    }
}

/// `eCollisionType` (`Intersect.h:142-147`).
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum CollisionKind {
    Robot,
    Wall,
    None,
}

/// `tCollision2D` (`Intersect.h:149-158`); `NoCollision2D` = [`Collision2D::none`].
#[derive(Copy, Clone, Debug)]
pub struct Collision2D {
    pub ntr: Intersection2D,
    pub kind: CollisionKind,
    pub robot_id: Option<RobotId>,
    pub wall_axis: PlaneAxis,
    /// `pt2Position`: the other robot's next position; `(0,0)` for walls.
    pub position: Vec2,
    /// The *other* robot's velocity (used by crash-damage impulse); `(0,0)`
    /// for walls (`NoCollision2D` default, `Intersect.h:158`).
    pub vec2_direction: Vec2,
}

impl Collision2D {
    /// `NoCollision2D` (`Intersect.h:158`): `{ntr2NoIntersection, collisionNone,
    /// idInvalidRobot, planeInvalid, (0,0), (0,0)}`.
    pub fn none() -> Self {
        Self {
            ntr: no_intersection_2d(),
            kind: CollisionKind::None,
            robot_id: None,
            wall_axis: PlaneAxis::Invalid,
            position: Vec2::ZERO,
            vec2_direction: Vec2::ZERO,
        }
    }
}

/// A muzzle-spawn request produced by [`Robot::fire`]; the `World` turns it
/// into a live `Projectile` (mirrors `cRobot::Fire` → `AddProjectile`).
#[derive(Copy, Clone, Debug)]
pub struct FireRequest {
    pub projectile_type: ProjectileTypeId,
    pub position: Vec3,
    pub speed: f32,
    pub theta: f32,
    pub phi: f32,
}

/// A live robot instance (`cRobot`). Static `tRobotType` parameters are
/// copied in at construction (the original held a `const tRobotType*`).
pub struct Robot {
    pub robot_type: RobotTypeId,
    pub id: RobotId,
    pub team: TeamId,

    /// `m_pt2Position`.
    pub position: Vec2,
    /// `m_pt2PositionNew`.
    pub position_new: Vec2,
    /// `m_vec2Move`.
    pub move_vec: Vec2,

    pub speed: f32,
    pub desired_speed: f32,
    pub energy: f32,
    pub health: f32,
    /// `m_angDirection` (heading about +Z; turning is instantaneous, PARITY).
    pub direction: f32,
    /// `m_angDirectionDesired`.
    pub direction_desired: f32,
    /// `m_angBarrelElevation`, clamped `[0, pi/4]`.
    pub barrel_elevation: f32,
    pub move_state: Throttle,

    pub proj_states: Vec<ProjectileTypeState>,

    /// `m_fCollisionTime` (fraction of the frame move that is allowed).
    pub collision_time: f32,
    /// `m_bCollided`.
    pub collided: bool,

    // --- copied `tRobotType` params ---
    pub accel: f32,
    pub brake_decel: f32,
    pub friction_decel: f32,
    pub max_speed: f32,
    pub energy_start: f32,
    pub health_start: f32,
    pub projectile_armor: f32,
    pub crash_armor: f32,

    /// The `cAIRobot` half. Every spawned robot has one; a robot without it
    /// is always driven by `World::inputs`.
    pub ai: Option<AiRuntime>,
}

impl Robot {
    /// `cRobot::cRobot` (`cRobot.cpp:15-52`). `size` is fixed at
    /// `(ROBOT_SIZE_XY, ROBOT_SIZE_XY)` (`Make2D (shRobot3D.GetSize ())`).
    pub fn new(
        robot_type: RobotTypeId,
        team: TeamId,
        id: RobotId,
        position: Vec2,
        options: &RobotType,
    ) -> Self {
        let proj_states = options
            .projectile_data
            .iter()
            .filter_map(|pd| {
                pd.projectile_type.map(|ty| ProjectileTypeState {
                    projectile_type: ty,
                    projectiles_left: pd.count as f32,
                    start_count: pd.count as f32,
                    fire_delay: 0.0,
                    fire_rate: pd.fire_rate,
                    accuracy: pd.accuracy,
                })
            })
            .collect();

        Self {
            robot_type,
            id,
            team,
            position,
            position_new: position,
            move_vec: Vec2::ZERO,
            speed: 0.0,
            desired_speed: 0.0,
            energy: options.energy_start,
            health: options.health_start,
            direction: 0.0,
            direction_desired: 0.0,
            barrel_elevation: 0.0,
            move_state: Throttle::Accelerate, // m_MoveState (moveAccelerate)
            proj_states,
            collision_time: 0.0,
            collided: false,
            accel: options.accel,
            brake_decel: options.brake_decel,
            friction_decel: options.friction_decel,
            max_speed: options.max_speed,
            energy_start: options.energy_start,
            health_start: options.health_start,
            projectile_armor: options.projectile_armor,
            crash_armor: options.crash_armor,
            ai: None,
        }
    }

    // ----- state queries -----

    /// `cRobot::IsDead` (`cRobot.cpp:727-730`): `m_fHealth <= 0.0f`.
    pub fn is_dead(&self) -> bool {
        self.health <= 0.0
    }

    /// `cRobot::IsStationary` (`cRobot.cpp:733-736`): `Equals (m_fSpeed, 0)`.
    pub fn is_stationary(&self) -> bool {
        equals(self.speed, 0.0)
    }

    /// `cRobot::GetDirection` (`cRobot.cpp:702-705`): `(cos dir, sin dir)`.
    pub fn direction_vec(&self) -> Vec2 {
        Vec2::new(self.direction.cos(), self.direction.sin())
    }

    /// `cRobot::GetVelocity` (`cRobot.cpp:708-711`): `direction * speed`.
    pub fn velocity(&self) -> Vec2 {
        self.direction_vec() * self.speed
    }

    /// `cRobot::GetNextPosition` (`cRobot.cpp:696-699`):
    /// `m_pt2Position + m_vec2Move * m_fCollisionTime`.
    pub fn next_position(&self) -> Vec2 {
        self.position + self.move_vec * self.collision_time
    }

    pub fn size(&self) -> Vec2 {
        Vec2::new(ROBOT_SIZE_XY, ROBOT_SIZE_XY)
    }

    /// `cRobot::GetBoundingRect` (`cRobot.cpp:561-567`).
    pub fn bounding_rect(&self) -> Rect {
        Rect {
            center: self.position,
            size: self.size(),
        }
    }

    /// `cRobot::GetMovingRect` (`cRobot.cpp:570-578`).
    pub fn moving_rect(&self) -> MovingRect {
        MovingRect {
            center1: self.position,
            center2: self.next_position(),
            size: self.size(),
            stop_time: self.collision_time,
        }
    }

    /// `cRobot::GetMovingBox` (`cRobot.cpp:581-591`) — 3D swept box used for
    /// projectile-vs-robot hits. Center offset `(0,0,ROBOT_CENTER_Z)`.
    pub fn moving_box(&self) -> MovingBox {
        let center = Vec3::new(0.0, 0.0, ROBOT_CENTER_Z);
        MovingBox {
            center1: Vec3::new(self.position.x, self.position.y, 0.0) + center,
            center2: Vec3::new(self.next_position().x, self.next_position().y, 0.0) + center,
            size: Vec3::new(ROBOT_SIZE_XY, ROBOT_SIZE_XY, ROBOT_SIZE_Z),
            stop_time: self.collision_time,
        }
    }

    // ----- control (from RobotInput / cRobot::Accelerate/Rotate/Elevate) -----

    /// `cRobot::SetMoveState` (`cRobot.cpp:139-142`).
    pub fn set_move_state(&mut self, state: Throttle) {
        self.move_state = state;
    }

    /// `cRobot::Accelerate` (`cRobot.cpp:383-391`): `m_fDesiredSpeed += x`,
    /// clamped `[-fMaxSpeed, fMaxSpeed]`.
    pub fn accelerate(&mut self, accel_speed: f32) {
        if self.is_dead() {
            return;
        }
        self.desired_speed += accel_speed;
        self.desired_speed = f_clamp(self.desired_speed, -self.max_speed, self.max_speed);
    }

    /// `cRobot::Rotate` (`cRobot.cpp:394-400`): `m_angDirectionDesired += x`.
    pub fn rotate(&mut self, ang_rotate: f32) {
        if self.is_dead() {
            return;
        }
        self.direction_desired += ang_rotate;
    }

    /// `cRobot::Elevate` (`cRobot.cpp:403-410`): `m_angBarrelElevation += x`,
    /// clamped `[0, pi/4]`.
    pub fn elevate(&mut self, ang_elevate: f32) {
        if self.is_dead() {
            return;
        }
        self.barrel_elevation += ang_elevate;
        self.barrel_elevation = f_clamp(self.barrel_elevation, 0.0, PI * 0.25);
    }

    // ----- kinematics (two-phase) -----

    /// PARITY: `cRobot::GetMoveAccel` (`cRobot.cpp:791-835`). Returns
    /// `(robot_accel, friction_accel)` — both are velocity *deltas* already
    /// multiplied by `fTimeElapsed`, fed into ½at² below (frame-rate coupled).
    pub fn get_move_accel(&self, dt: f32) -> (f32, f32) {
        match self.move_state {
            Throttle::Accelerate => {
                let friction =
                    -f_min(self.speed.abs(), sign(self.speed) * self.friction_decel * dt);
                let max_robot_accel = self.accel * dt;
                let mut robot_accel = self.desired_speed - self.speed;
                robot_accel = f_clamp(robot_accel, -max_robot_accel, max_robot_accel);
                robot_accel += -friction; // accelerate to counteract friction
                (robot_accel, friction)
            }
            Throttle::Cruise => {
                let friction =
                    -f_min(self.speed.abs(), sign(self.speed) * self.friction_decel * dt);
                (0.0, friction)
            }
            Throttle::Brake => {
                let friction =
                    -f_min(self.speed.abs(), sign(self.speed) * self.brake_decel * dt);
                (0.0, friction)
            }
        }
    }

    /// PARITY: `cRobot::CalcNewPosition` (`cRobot.cpp:94-136`). Phase 1 —
    /// proposes `position_new`/`move_vec` from the current input and applies
    /// the instantaneous heading + energy/reload bookkeeping.
    pub fn calc_new_position(&mut self, dt: f32) {
        if self.is_dead() {
            return;
        }

        self.collision_time = 1.0;
        self.collided = false;

        for state in &mut self.proj_states {
            state.fire_delay = f_max(state.fire_delay - dt, 0.0);
        }

        if self.energy <= 0.0 {
            // A robot with no energy left can only cruise.
            self.move_state = Throttle::Cruise;
        }

        // PARITY: instantaneous turn (rate-limit commented out, cRobot.cpp:117-120).
        self.direction = self.direction_desired;

        let (robot_accel, friction_accel) = self.get_move_accel(dt);

        let distance_traveled =
            self.speed * dt + 0.5 * (robot_accel + friction_accel) * dt * dt;
        self.move_vec = self.direction_vec() * distance_traveled;
        self.position_new = self.position + self.move_vec;

        self.speed += robot_accel + friction_accel;
        self.energy = f_max(0.0, self.energy - robot_accel.abs());
    }

    /// PARITY: `cRobot::AddCollision` (`cRobot.cpp:226-275`). Records the
    /// earliest collision and truncates `collision_time` by the `0.005`-unit
    /// back-off so the robot stops just short of contact.
    pub fn add_collision(&mut self, collided: Collision2D) {
        let intersect_time = collided.ntr.time;
        if collided.kind != CollisionKind::None
            && (!self.collided || intersect_time < self.collision_time)
        {
            self.collision_time = intersect_time;
            self.collided = true;

            const OFFSET_DISTANCE: f32 = 0.005;
            let mut subtract_time = 0.0;

            match collided.kind {
                CollisionKind::Robot => {
                    if self.move_vec.x.abs() > self.move_vec.y.abs() {
                        subtract_time = if equals(self.move_vec.x, 0.0) {
                            0.0
                        } else {
                            OFFSET_DISTANCE / self.move_vec.x.abs()
                        };
                    } else if self.move_vec.x.abs() < self.move_vec.y.abs() {
                        subtract_time = if equals(self.move_vec.y, 0.0) {
                            0.0
                        } else {
                            OFFSET_DISTANCE / self.move_vec.y.abs()
                        };
                    } else {
                        subtract_time = if equals(self.move_vec.x, 0.0) {
                            0.0
                        } else {
                            OFFSET_DISTANCE / self.move_vec.x.abs()
                        };
                    }
                }
                CollisionKind::Wall => match collided.wall_axis {
                    PlaneAxis::X => {
                        subtract_time = if equals(self.move_vec.x, 0.0) {
                            0.0
                        } else {
                            OFFSET_DISTANCE / self.move_vec.x.abs()
                        };
                    }
                    PlaneAxis::Y => {
                        subtract_time = if equals(self.move_vec.y, 0.0) {
                            0.0
                        } else {
                            OFFSET_DISTANCE / self.move_vec.y.abs()
                        };
                    }
                    _ => {}
                },
                CollisionKind::None => {}
            }

            self.collision_time -= subtract_time;
        }
    }

    /// PARITY: `cRobot::Move` (`cRobot.cpp:145-223`). Phase 3 — commits the
    /// (possibly truncated) move and applies crash damage from the closing
    /// impulse against `fCrashArmor`.
    pub fn commit_move(&mut self, collided: Collision2D) {
        if self.collided {
            let direction = self.velocity();
            let collision = direction - collided.vec2_direction;
            let collision_force = if equals(self.speed, 0.0) {
                // Special case to prevent division by zero.
                length2d(collided.vec2_direction)
            } else {
                ((direction.x * collision.x + direction.y * collision.y) / self.speed).abs()
            };
            self.health -= collision_force / self.crash_armor;
            self.speed = 0.0;
        }

        self.position += self.move_vec * self.collision_time;
    }

    // ----- collision detection (robot side) -----

    /// PARITY: `cRobot::IntersectRobot` (`cRobot.cpp:488-513`).
    pub fn intersect_robot(&self, other: &Robot) -> Collision2D {
        if self.id == other.id {
            return Collision2D::none();
        }
        let ntr = crate::math::intersect::moving_rect_moving_rect(
            self.moving_rect(),
            other.moving_rect(),
        );
        if ntr.hit {
            Collision2D {
                ntr,
                kind: CollisionKind::Robot,
                robot_id: Some(other.id),
                wall_axis: PlaneAxis::Invalid,
                position: other.next_position(),
                vec2_direction: other.velocity(),
            }
        } else {
            Collision2D::none()
        }
    }

    // ----- firing -----

    fn projectile_state(&self, ty: ProjectileTypeId) -> Option<&ProjectileTypeState> {
        self.proj_states
            .iter()
            .find(|s| s.projectile_type == ty)
    }

    fn projectile_state_mut(&mut self, ty: ProjectileTypeId) -> Option<&mut ProjectileTypeState> {
        self.proj_states
            .iter_mut()
            .find(|s| s.projectile_type == ty)
    }

    /// PARITY: `cRobot::GetProjectileFireType` (`cRobot.cpp:427-441`) — the
    /// **last** ready-to-fire type wins (last-matching-entry selection).
    pub fn projectile_fire_type(&self) -> Option<ProjectileTypeId> {
        let mut chosen = None;
        for state in &self.proj_states {
            if state.fire_ready() {
                chosen = Some(state.projectile_type);
            }
        }
        chosen
    }

    /// PARITY: `cRobot::GetProjectileRestockType` (`cRobot.cpp:444-458`) — the
    /// **last** not-full type wins.
    pub fn projectile_restock_type(&self) -> Option<ProjectileTypeId> {
        let mut chosen = None;
        for state in &self.proj_states {
            if state.projectiles_left < state.start_count {
                chosen = Some(state.projectile_type);
            }
        }
        chosen
    }

    /// PARITY: `cRobot::Fire (tProjectileTypeID)` (`cRobot.cpp:279-313`).
    /// Applies scatter (`phi` scatter scaled ×0.25 then clamped `[0, pi/4]`),
    /// records the new heading/elevation, decrements ammo, and returns the
    /// muzzle-spawn request. Returns `None` if dead / type unused / not ready.
    pub fn fire(&mut self, ty: ProjectileTypeId, rng: &mut GameRng) -> Option<FireRequest> {
        if self.is_dead() {
            return None;
        }
        let (accuracy, fire_rate) = {
            let state = self.projectile_state(ty)?;
            if !state.fire_ready() {
                return None;
            }
            (state.accuracy, state.fire_rate)
        };

        let ang_theta = rng.rand_variance(self.direction, accuracy);
        let ang_phi = self.barrel_elevation + rng.rand_variance(0.0, accuracy) * 0.25;
        let ang_phi = f_clamp(ang_phi, 0.0, PI * 0.25);

        self.direction = ang_theta;
        self.barrel_elevation = ang_phi;

        let pt3 = Vec3::new(self.position.x, self.position.y, 0.0)
            + firing_offset(ang_theta, ang_phi);

        let request = FireRequest {
            projectile_type: ty,
            position: pt3,
            speed: self.speed,
            theta: ang_theta,
            phi: ang_phi,
        };

        if let Some(state) = self.projectile_state_mut(ty) {
            state.fire_delay = fire_rate;
            state.projectiles_left -= 1.0;
        }

        Some(request)
    }

    /// PARITY: `cRobot::IncreaseAmmo` (`cRobot.cpp:461-471`) — clamped to the
    /// starting count. `n` may be `+inf` for the unlisted-transfer-rate quirk,
    /// in which case `min(+inf, start) == start` fills the
    /// magazine; **no guard** (defined IEEE `min`).
    pub fn increase_ammo(&mut self, ty: ProjectileTypeId, n: f32) {
        if let Some(state) = self.projectile_state_mut(ty) {
            state.projectiles_left = f_min(state.projectiles_left + n, state.start_count);
        }
    }

    /// `cRobot::IncreaseEnergy` (`cRobot.cpp:474-478`).
    pub fn increase_energy(&mut self, energy: f32) {
        self.energy += energy;
        self.energy = f_min(self.energy, self.energy_start);
    }

    /// `cRobot::IncreaseHealth` (`cRobot.cpp:481-485`).
    pub fn increase_health(&mut self, health: f32) {
        self.health += health;
        self.health = f_min(self.health, self.health_start);
    }

    /// `cRobot::ProjectileHit` (`cRobot.cpp:373-380`).
    pub fn projectile_hit(&mut self, damage: f32) {
        if self.is_dead() {
            return;
        }
        self.health = f_max(self.health - damage / self.projectile_armor, 0.0);
    }
}

/// PARITY: `cRobot::Fire (const cRobot&)` fire-alignment gate
/// (`cRobot.cpp:366`): `fabs (m_angDirection - m_angDirectionDesired) <
/// D3DX_PI * 0.025f`. **No 2π wrap** — a heading just past the ±2π boundary
/// fails the gate even when geometrically aligned.
pub fn fire_gate_aligned(direction: f32, direction_desired: f32) -> bool {
    (direction - direction_desired).abs() < PI * 0.025
}

/// PARITY: `cRobot3D::GetFiringOffset` (`cRobot3D.cpp:45-49`):
/// `SphereCoord (theta, phi, fBarrelLength*2) + (0, 0, fTreadRadius*2 +
/// fBarrelRadius)` from the compiled-in `data::builtin::OBJECTS` dims.
pub fn firing_offset(theta: f32, phi: f32) -> Vec3 {
    use crate::data::builtin::OBJECTS;
    let barrel_offset = Vec3::new(
        0.0,
        0.0,
        OBJECTS.tread_radius * 2.0 + OBJECTS.barrel_radius,
    );
    crate::math::sphere_coord(theta, phi, OBJECTS.barrel_length * 2.0) + barrel_offset
}
