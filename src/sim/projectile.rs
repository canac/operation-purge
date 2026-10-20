//! `cProjectile`: ballistic step, swept hits, Newton aim solver
//! (source-map §3.5), ported from `cProjectile.cpp`.
//!
//! PARITY (design §1.3, preserved verbatim):
//! - Aim-solver time-of-flight derived from **X displacement only**
//!   (`cProjectile.cpp:66-71`); the 0/0 -> NaN path near ±Y targets is
//!   defined IEEE f32 behavior, preserved as-is (Newton fails to
//!   converge -> shot misses; must not panic).
//! - Sprite tint literally `(0,0,0,1)` — black silhouette under modulate
//!   (`cProjectile.cpp:211`) — recorded here for the M9 renderer.
//! - `AddCollision` keeps the collision with the **smallest** intersect
//!   time (`cProjectile.cpp:180-187`); the initial sentinel time is `1.0`
//!   (`ntr3NoIntersection`), so the first hit inside `[0,1)` always wins.

use crate::data::GameData;
use crate::ids::{ProjectileTypeId, RobotId};
use crate::math::accelerate_point;
use crate::math::intersect::{
    Intersection3D, MovingBox, PlaneAxis, Trajectory, no_intersection_3d, trajectory_moving_box,
};
use crate::rng::GameRng;
use crate::sim::robot::CollisionKind;
use glam::Vec3;
use std::f32::consts::PI;

/// `tCollision3D` (`Types.h`), the 3D twin of `sim::robot::Collision2D`.
#[derive(Copy, Clone, Debug)]
pub struct Collision3D {
    pub ntr: Intersection3D,
    pub kind: CollisionKind,
    pub robot_id: Option<RobotId>,
    pub wall_axis: PlaneAxis,
    pub position: Vec3,
    pub direction: Vec3,
}

impl Collision3D {
    /// `NoCollision3D`: `{ntr3NoIntersection, collisionNone, idInvalidRobot,
    /// planeInvalid, (0,0,0), (0,0,0)}`.
    pub fn none() -> Self {
        Self {
            ntr: no_intersection_3d(),
            kind: CollisionKind::None,
            robot_id: None,
            wall_axis: PlaneAxis::Invalid,
            position: Vec3::ZERO,
            direction: Vec3::ZERO,
        }
    }
}

pub struct Projectile {
    pub projectile_type: ProjectileTypeId,
    /// The firer, for reference only. PARITY: the original tracks no owner
    /// and a projectile **can hit the robot that fired it**
    /// (`IntersectProjectileObjects` tests every robot,
    /// `cOperationPurge.cpp:1820-1830`) — this field is never used to
    /// exclude a robot from the swept test.
    pub owner: Option<RobotId>,
    pub position: Vec3,
    pub velocity: Vec3,
    /// `m_trjTrajectory` — the segment swept this frame (set by [`step`]).
    pub trajectory: Trajectory,
    /// `m_CollidedObject` accumulated across the per-frame swept tests.
    pub collided: Collision3D,
    /// The projectile's ongoing trail effect (`travel_effect`), if its type
    /// defines one. A **dynamic** effect that follows the projectile and is
    /// dropped with it when the projectile is destroyed (owner-deletes;
    /// M9 wiring, `sim::world`).
    pub travel_effect: Option<crate::sim::particles::ParticleEffect>,
}

impl Projectile {
    /// `cProjectile::cProjectile` (`cProjectile.cpp:14-33`): initial
    /// velocity = `SphereCoord(theta, phi, type.initial_speed + speed)`.
    /// `speed` is the firer's forward speed.
    pub fn new(
        projectile_type: ProjectileTypeId,
        owner: Option<RobotId>,
        position: Vec3,
        speed: f32,
        theta: f32,
        phi: f32,
        initial_speed: f32,
    ) -> Self {
        let velocity = crate::math::sphere_coord(theta, phi, initial_speed + speed);
        Self {
            projectile_type,
            owner,
            position,
            velocity,
            trajectory: Trajectory {
                start: position,
                direction: velocity,
                gravity: 0.0,
                travel_time: 0.0,
            },
            collided: Collision3D::none(),
            travel_effect: None,
        }
    }

    /// PARITY: `cProjectile::Move` (`cProjectile.cpp:108-135`). Advances one
    /// ballistic segment and records the swept trajectory for this frame.
    /// `gravity` is `type.fGravityAccel`.
    pub fn step(&mut self, dt: f32, gravity: f32) {
        let old_pos = self.position;
        let old_vel = self.velocity;

        self.position = accelerate_point(dt, self.position, self.velocity, gravity);
        self.velocity.z -= gravity * dt;

        self.trajectory = Trajectory {
            start: old_pos,
            direction: old_vel,
            gravity,
            travel_time: dt,
        };
        // Reset the per-frame collision accumulator.
        self.collided = Collision3D::none();
    }

    /// PARITY: `cProjectile::IntersectRobot` (`cProjectile.cpp:139-157`).
    pub fn intersect_robot(&mut self, moving_box: MovingBox, robot_id: RobotId) {
        let ntr = trajectory_moving_box(self.trajectory, moving_box);
        if ntr.hit {
            let collision = Collision3D {
                ntr,
                kind: CollisionKind::Robot,
                robot_id: Some(robot_id),
                wall_axis: PlaneAxis::Invalid,
                position: self.trajectory.start + self.trajectory.direction * ntr.time,
                direction: self.trajectory.direction,
            };
            self.add_collision(collision);
        }
    }

    /// PARITY: `cProjectile::IntersectBuilding` (`cProjectile.cpp:161-177`).
    /// The building sweep `ntr` is computed by the caller (`sim::world`).
    pub fn intersect_building(&mut self, ntr: Intersection3D) {
        if ntr.hit {
            let collision = Collision3D {
                ntr,
                kind: CollisionKind::Wall,
                robot_id: None,
                wall_axis: ntr.plane.axis,
                position: Vec3::ZERO,
                direction: Vec3::ZERO,
            };
            self.add_collision(collision);
        }
    }

    /// PARITY: `cProjectile::AddCollision` (`cProjectile.cpp:180-187`) —
    /// keep the earliest-time collision.
    pub fn add_collision(&mut self, collision: Collision3D) {
        if collision.kind != CollisionKind::None && collision.ntr.time < self.collided.ntr.time {
            self.collided = collision;
        }
    }

    /// `cProjectile::GetCollision` (`cProjectile.cpp:190-193`).
    pub fn collision(&self) -> Collision3D {
        self.collided
    }

    /// PARITY: `cProjectile::GetDamage` (`cProjectile.cpp:229-233`):
    /// `fMax(0, damage + fRand()*variance*2 - variance)`.
    pub fn get_damage(&self, game_data: &GameData, rng: &mut GameRng) -> f32 {
        let opts = &game_data.projectile_types[self.projectile_type.get() as usize];
        let variance = opts.damage_variance;
        crate::sim::robot::f_max(0.0, opts.damage + rng.frand() * variance * 2.0 - variance)
    }

    /// PARITY: `cProjectile::DisplayCollisionEffect` (`cProjectile.cpp:321-357`).
    /// Returns the burst's `(position, velocity)`: the impact point and the
    /// pre-step velocity with the hit plane's component flipped, or `None`
    /// when the hit plane is invalid.
    pub fn collision_effect_placement(&self) -> Option<(Vec3, Vec3)> {
        let position = accelerate_point(
            self.collided.ntr.time,
            self.trajectory.start,
            self.trajectory.direction,
            self.trajectory.gravity,
        );
        let mut direction = self.trajectory.direction;
        match self.collided.ntr.plane.axis {
            PlaneAxis::X => direction.x = -direction.x,
            PlaneAxis::Y => direction.y = -direction.y,
            PlaneAxis::Z => direction.z = -direction.z,
            PlaneAxis::Invalid => return None,
        }
        Some((position, direction))
    }

    /// PARITY: `cProjectile::DisplayHole` position/orientation
    /// (`cProjectile.cpp:237-300`). Returns `(position, outward_normal)`, or
    /// `None` when the collision plane is invalid (not a wall hit).
    pub fn hole_placement(&self) -> Option<(Vec3, Vec3)> {
        let t = self.collided.ntr.time;
        let plane = self.collided.ntr.plane;
        let mut pos = accelerate_point(
            t,
            self.trajectory.start,
            self.trajectory.direction,
            self.trajectory.gravity,
        );
        const WALL_DIST: f32 = 0.005;
        let normal;
        match plane.axis {
            PlaneAxis::X => {
                if self.trajectory.direction.x >= 0.0 {
                    pos.x = plane.center.x - WALL_DIST;
                    normal = Vec3::new(-1.0, 0.0, 0.0);
                } else {
                    pos.x = plane.center.x + WALL_DIST;
                    normal = Vec3::new(1.0, 0.0, 0.0);
                }
            }
            PlaneAxis::Y => {
                if self.trajectory.direction.y >= 0.0 {
                    pos.y = plane.center.y - WALL_DIST;
                    normal = Vec3::new(0.0, -1.0, 0.0);
                } else {
                    pos.y = plane.center.y + WALL_DIST;
                    normal = Vec3::new(0.0, 1.0, 0.0);
                }
            }
            PlaneAxis::Z => {
                if self.trajectory.direction.z >= 0.0 {
                    pos.z = plane.center.z - WALL_DIST;
                    normal = Vec3::new(0.0, 0.0, -1.0);
                } else {
                    pos.z = plane.center.z + WALL_DIST;
                    normal = Vec3::new(0.0, 0.0, 1.0);
                }
            }
            PlaneAxis::Invalid => return None,
        }
        Some((pos, normal))
    }
}

/// PARITY: `cProjectile::GetTrajectory` (the static aim solver,
/// `cProjectile.cpp:54-106`). Newton-Raphson, 20 iterations, tolerance
/// `1e-6`, initial guess `pi/4`, **time-of-flight derived from X
/// displacement only**. `speed` is the total launch speed
/// (`firer_speed + type.initial_speed`); `gravity` is `type.gravity_accel`.
///
/// Returns `(theta, phi)` on convergence into the first quadrant, or `None`
/// on non-convergence. PARITY: a near-pure-±Y target makes `fCosTheta == 0`
/// so the X-only `fTime = dx / (cosTheta*cosPhi*speed)` is `0/0 = NaN`;
/// Newton then never converges (the `|Δ| <= tol` check is always false for
/// `NaN`) and the loop falls through to `None` — no panic (design §1.3).
pub fn solve_aim(position: Vec3, target: Vec3, speed: f32, gravity: f32) -> Option<(f32, f32)> {
    // theta = GetAngle(Make2D(position), Make2D(target))
    let theta = crate::math::get_angle(position.truncate(), target.truncate());
    let cos_theta = theta.cos();

    const MAX_ITERATIONS: u32 = 20;
    const ACCURACY: f32 = 1e-6;

    let mut phi = PI * 0.25; // initial guess
    for _ in 0..MAX_ITERATIONS {
        let cos_phi = phi.cos();
        let sin_phi = phi.sin();
        let time = (target.x - position.x) / (cos_theta * cos_phi * speed);
        let time_prime =
            (target.x - position.x) * sin_phi / (cos_theta * (cos_phi * cos_phi) * speed);
        let function =
            (position.z - target.z) + sin_phi * speed * time - 0.5 * gravity * time * time;
        let function_prime =
            cos_phi * speed * time + sin_phi * speed * time_prime - gravity * time * time_prime;

        let phi_old = phi;
        phi -= function / function_prime;
        if (phi - phi_old).abs() <= ACCURACY {
            // Reduce to a first-quadrant angle.
            let revolutions = (phi / (PI * 2.0)) as i32;
            phi -= revolutions as f32 * PI * 2.0;
            if !crate::math::between(phi, 0.0, PI * 0.5) {
                return None;
            }
            return Some((theta, phi));
        }
    }

    // The speed was probably too low; the projectile cannot reach the target.
    None
}
