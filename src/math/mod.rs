//! Shared math helpers ported from `Functions.h/.cpp` (source-map §3.7),
//! plus the `Rgba` color type used by `data/` and `render/`.
//!
//! Full bodies (SphereCoord, AcceleratePoint) land at M2 alongside
//! `math::d3dx`; this module fixes their signatures now because `sim/`
//! and `render/` stubs already need to name them.

pub mod d3dx;
pub mod intersect;

use glam::Vec3;

/// Absolute-epsilon float comparison.
///
/// PARITY: `Functions.cpp:8` `Equals` uses an **absolute** epsilon of
/// `1e-7`, not a relative/ULP comparison — load-bearing in at least 6
/// behaviors per source-map §7 (fire-alignment gate, wall-bounce heading
/// snap, center-transfer gating, etc). Do not swap in a relative
/// comparison here even though `1e-7` absolute is unusual for large
/// magnitudes; the original's inputs are always small (radians, unit
/// vectors, room-scale positions).
pub const EPSILON: f32 = 1e-7;

/// PARITY: `Functions.cpp:83-86` — `fabsf (fNumber1 - fNumber2) <= COMPARE_TOLERANCE`
/// (`<=`, not `<`); a difference of exactly `1e-7` compares equal.
#[inline]
pub fn equals(a: f32, b: f32) -> bool {
    (a - b).abs() <= EPSILON
}

/// PARITY: `Functions.cpp:56-62` `Between` — inclusive range test in either
/// bound order, **plus** an epsilon-`Equals` fallback against either bound
/// (catches values that miss the inclusive range by float rounding but are
/// `Equals`-close to a boundary). Used pervasively by `math::intersect` and
/// (at M6/M7) `sim::robot`/`sim::ai` heading checks.
#[inline]
pub fn between(number: f32, bound1: f32, bound2: f32) -> bool {
    (number >= bound1 && number <= bound2)
        || (number >= bound2 && number <= bound1)
        || equals(number, bound1)
        || equals(number, bound2)
}

/// PARITY: `Functions.cpp:78-81` `Sign` — `fNumber >= 0.0f ? 1.0f : -1.0f`
/// (note: `Sign(0.0) == 1.0`, and `Sign(-0.0) == 1.0` since `-0.0 >= 0.0`).
/// Consumed by `sim::robot::GetMoveAccel` friction sign (M6).
#[inline]
pub fn sign(n: f32) -> f32 {
    if n >= 0.0 {
        1.0
    } else {
        -1.0
    }
}

/// PARITY: `Functions.cpp:64-75` `Floor` — a truncation-based `floor` that
/// matches C `(int) floorf` for the value range the sim uses: non-negative
/// values truncate toward zero (== floor); negative values truncate then
/// subtract 1 unless the value was already integral. Used by the building
/// swept-collision grid walk (`cBuilding::GetIntersectedWallsPortion`, M6).
#[inline]
pub fn floor_i32(n: f32) -> i32 {
    if n >= 0.0 {
        n as i32
    } else {
        let i = n as i32;
        if i as f32 == n {
            i
        } else {
            i - 1
        }
    }
}

/// RGBA color in final (post-`TransformGameData`) form.
///
/// Always plain `f32` components — a packed `COLORREF`/`D3DCOLOR` `u32`
/// never exists in this port's memory (design §4.1: colors are converted
/// once, at `GameData.toml` deserialize time, in `data::values`).
#[derive(Copy, Clone, PartialEq, Debug, Default)]
pub struct Rgba {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Rgba {
    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    pub const BLACK_OPAQUE: Rgba = Rgba::new(0.0, 0.0, 0.0, 1.0);
}

/// PARITY: `Functions.cpp:191-200` `GetAngle(pt2Point1, pt2Point2)` =
/// `atan2f((to - from).y, (to - from).x)`. Consumed by the projectile aim
/// solver (`sim::projectile::solve_aim`) and (at M7) `sim::ai` targeting.
#[inline]
pub fn get_angle(from: glam::Vec2, to: glam::Vec2) -> f32 {
    let d = to - from;
    d.y.atan2(d.x)
}

/// Spherical-to-Cartesian point on a sphere, Z-up left-handed frame:
/// `theta` is the azimuth about +Z (in the X/Y plane), `phi` the elevation
/// from the X/Y plane toward +Z, `radius` the resulting vector's
/// magnitude.
///
/// PARITY: `Functions.cpp:157-164` `SphereCoord` (source-map §3.7) —
/// `x = cos(theta)*cos(phi)*r`, `y = sin(theta)*cos(phi)*r`, `z = sin(phi)*r`;
/// Z is the pole axis, matching the game's Z-up convention. Consumed by
/// `cOperationPurge::SetupScene` (camera look direction) and (at M6/M9)
/// `sim::projectile` (muzzle velocity) / `sim::particles` (emit cone).
pub fn sphere_coord(theta: f32, phi: f32, radius: f32) -> Vec3 {
    let x = theta.cos() * phi.cos() * radius;
    let y = theta.sin() * phi.cos() * radius;
    let z = phi.sin() * radius;
    Vec3::new(x, y, z)
}

/// Position after traveling `time` seconds from `position` at constant
/// `velocity` under a gravitational acceleration of `gravity_accel` applied
/// on -Z. This is the closed-form ballistic equation of motion (evaluated
/// at an absolute time, not an incremental integrator step) — velocity is
/// unchanged and not returned.
///
/// PARITY: `Functions.cpp:166-174` `AcceleratePoint(fTime, pt3Position,
/// vec3Velocity, fGravityAccel)` — `pos += vel*time; pos.z -= 0.5*g*time^2`
/// (source-map §3.7). Consumed by `math::intersect::trajectory_plane` and
/// (at M6/M9) `sim::projectile`/`sim::particles`.
///
/// Deviation note: the M0-era stub declared a different, semi-implicit
/// `(pos, vel, accel, dt) -> (new_pos, new_vel)` signature; the actual
/// C++ function is this simpler closed-form position-only evaluation, so
/// the signature was corrected to match `Functions.cpp` verbatim.
pub fn accelerate_point(time: f32, position: Vec3, velocity: Vec3, gravity_accel: f32) -> Vec3 {
    let mut result = position + velocity * time;
    result.z -= 0.5 * gravity_accel * time * time;
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equals_is_absolute_epsilon() {
        assert!(equals(1.0, 1.0 + 1e-8));
        assert!(!equals(1.0, 1.0 + 1e-6));
        // Absolute, not relative: a fixed offset that trips the epsilon at
        // small magnitudes must also trip it at large ones.
        assert!(!equals(1_000_000.0, 1_000_000.5));
    }

    #[test]
    fn equals_boundary_is_inclusive() {
        // PARITY: Functions.cpp:83-86 uses `<=`, so a difference of exactly
        // EPSILON compares equal. Compare against 0.0 (rather than
        // `1.0 + EPSILON`) so the difference is bit-for-bit EPSILON
        // instead of being perturbed by f32 rounding near 1.0's ULP.
        assert!(equals(0.0, EPSILON));
        assert!(!equals(0.0, EPSILON * 1.5));
    }

    #[test]
    fn between_accepts_either_bound_order() {
        assert!(between(0.5, 0.0, 1.0));
        assert!(between(0.5, 1.0, 0.0));
        assert!(!between(1.5, 0.0, 1.0));
    }

    #[test]
    fn between_epsilon_fallback_catches_near_boundary() {
        // Slightly outside the inclusive range but within Equals of a bound.
        assert!(between(1.0 + EPSILON * 0.5, 0.0, 1.0));
    }

    #[test]
    fn sign_matches_cpp() {
        // PARITY Functions.cpp:78-81: >= 0 -> +1 (including zero), else -1.
        assert_eq!(sign(3.0), 1.0);
        assert_eq!(sign(0.0), 1.0);
        assert_eq!(sign(-0.0), 1.0);
        assert_eq!(sign(-3.0), -1.0);
    }

    #[test]
    fn floor_i32_matches_cpp() {
        assert_eq!(floor_i32(2.9), 2);
        assert_eq!(floor_i32(2.0), 2);
        assert_eq!(floor_i32(0.0), 0);
        assert_eq!(floor_i32(-0.1), -1);
        assert_eq!(floor_i32(-2.0), -2);
        assert_eq!(floor_i32(-2.5), -3);
    }

    #[test]
    fn sphere_coord_matches_hand_computed_axes() {
        // theta=0, phi=0 -> +X axis at the given radius.
        let v = sphere_coord(0.0, 0.0, 2.0);
        assert!((v - Vec3::new(2.0, 0.0, 0.0)).length() < 1e-6);

        // theta=pi/2, phi=0 -> +Y axis.
        let v = sphere_coord(std::f32::consts::FRAC_PI_2, 0.0, 1.0);
        assert!((v - Vec3::new(0.0, 1.0, 0.0)).length() < 1e-5);

        // phi=pi/2 -> +Z pole, independent of theta.
        let v = sphere_coord(1.234, std::f32::consts::FRAC_PI_2, 3.0);
        assert!((v - Vec3::new(0.0, 0.0, 3.0)).length() < 1e-4);
    }

    #[test]
    fn accelerate_point_matches_ballistic_equation() {
        let pos = Vec3::new(0.0, 0.0, 0.0);
        let vel = Vec3::new(10.0, 0.0, 5.0);
        let result = accelerate_point(2.0, pos, vel, 9.8);
        // x/y move linearly; z drops by 0.5*g*t^2 in addition to v*t.
        assert!(equals(result.x, 20.0));
        assert!(equals(result.y, 0.0));
        let expected_z = 5.0 * 2.0 - 0.5 * 9.8 * 4.0;
        assert!((result.z - expected_z).abs() < 1e-4);
    }

    #[test]
    fn accelerate_point_zero_gravity_is_pure_linear_motion() {
        let pos = Vec3::new(1.0, 2.0, 3.0);
        let vel = Vec3::new(1.0, 1.0, 1.0);
        let result = accelerate_point(4.0, pos, vel, 0.0);
        assert!((result - Vec3::new(5.0, 6.0, 7.0)).length() < 1e-6);
    }
}
