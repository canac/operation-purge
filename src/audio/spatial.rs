//! Manual DS3D spatial model: gain/pan math in the sim's LH Z-up frame,
//! replacing `IDirectSound3DListener`/`IDirectSound3DBuffer`
//! (design §5.4). Ported at M10 from the DS3D setup in
//! `cOperationPurge::Play3DSound` (`cOperationPurge.cpp:1842-1857`,
//! `flMinDistance = 10`, `flMaxDistance = 100`, `DSBCAPS_MUTE3DATMAXDISTANCE`)
//! and the listener frame in `ExecuteCommands`
//! (`cOperationPurge.cpp:1276-1291`: `vOrientFront = (cos ang, sin ang, 0)`,
//! `vOrientTop = (0, 0, 1)`).
//!
//! PARITY: unity gain within min distance 10; `10/d` inverse-distance
//! rolloff from 10..100; **hard mute beyond 100**; no doppler (the
//! original never set source velocities); listener updated every 10th
//! frame (design §7 constants table).

use glam::{Vec2, Vec3};

/// Minimum distance: unity gain within this radius.
pub const MIN_DISTANCE: f32 = 10.0;
/// Maximum distance: hard mute beyond this radius.
pub const MAX_DISTANCE: f32 = 100.0;
/// Listener position/orientation refresh cadence, in frames.
pub const LISTENER_UPDATE_INTERVAL_FRAMES: u32 = 10;

/// Inverse-distance rolloff with unity gain inside `MIN_DISTANCE` and hard
/// mute beyond `MAX_DISTANCE` (design §5.4).
///
/// PARITY: reproduces the DS3D inverse-distance model with `flMinDistance
/// = 10`, `flMaxDistance = 100` and the `DSBCAPS_MUTE3DATMAXDISTANCE` flag
/// (`cOperationPurge.cpp:1856-1857`, `Sound.cpp` create flags). The design
/// pins the boundaries verbatim: unity for `d <= 10`, `10/d` across the
/// `10..=100` rolloff band (so `d == 100` yields `0.1`, still audible),
/// and a hard `0.0` mute for `d > 100`.
pub fn distance_gain(distance: f32) -> f32 {
    if distance <= MIN_DISTANCE {
        1.0
    } else if distance <= MAX_DISTANCE {
        MIN_DISTANCE / distance
    } else {
        0.0
    }
}

/// Stereo pan in `[-1.0, 1.0]` (`-1` = full left, `+1` = full right, `0` =
/// centered) from the listener-relative azimuth of `to_source` (the vector
/// from the listener to the sound), in the sim's LH Z-up frame.
///
/// PARITY: DirectSound uses a **left-handed** coordinate system, so the
/// listener's right axis is `up × forward` with `up = +Z`
/// (`cOperationPurge.cpp:1283-1288`). For a forward in the XY plane,
/// `Z × forward = (-sin ang, cos ang, 0)`. The pan is the normalized
/// projection of `to_source` onto that right axis — i.e. `sin(azimuth)`
/// measured from forward toward the listener's (LH) right — which is what
/// DS3D panned a mono 3D buffer to. There is a front/back ambiguity (a
/// source directly ahead and directly behind both pan to center); that is
/// inherent to a single stereo pan value and matches DS3D's own behavior.
///
/// A source at the listener position (`to_source == 0`) or a degenerate
/// forward (no XY component) pans to center.
pub fn azimuth_pan(listener_forward: Vec3, to_source: Vec3) -> f32 {
    // right = up × forward in the LH frame (up = +Z).
    let right = Vec3::Z.cross(listener_forward);
    let right_len = right.length();
    let source_len = to_source.length();
    if right_len <= f32::EPSILON || source_len <= f32::EPSILON {
        return 0.0;
    }
    let pan = to_source.dot(right) / (right_len * source_len);
    pan.clamp(-1.0, 1.0)
}

/// The DS3D listener pose in the sim's LH Z-up frame (position + forward),
/// refreshed every `LISTENER_UPDATE_INTERVAL_FRAMES` frames (design §5.4).
///
/// PARITY: `ListenerData` in `cOperationPurge::ExecuteCommands`
/// (`cOperationPurge.cpp:1276-1291`). Position is the observer/active-robot
/// 2D position lifted to `z = 0`; forward is `(cos ang, sin ang, 0)`; up is
/// implicitly `+Z`; velocity is always zero (no doppler).
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct AudioListener {
    pub position: Vec3,
    pub forward: Vec3,
}

impl Default for AudioListener {
    fn default() -> Self {
        // Origin, facing +X (ang = 0) — the neutral pose before any
        // observer/active robot is selected.
        Self {
            position: Vec3::ZERO,
            forward: Vec3::X,
        }
    }
}

impl AudioListener {
    /// Build from a 2D observer/active-robot position + heading (radians),
    /// exactly as `ExecuteCommands` set `vPosition`/`vOrientFront`
    /// (`cOperationPurge.cpp:1277-1285`).
    pub fn from_pose(position_2d: Vec2, rotation: f32) -> Self {
        Self {
            position: Vec3::new(position_2d.x, position_2d.y, 0.0),
            forward: Vec3::new(rotation.cos(), rotation.sin(), 0.0),
        }
    }

    /// Distance from the listener to a 3D sound origin (`Distance3D`,
    /// the sqrt of the `Distance3DSqr` the steal rule compares).
    pub fn distance_to(&self, origin: Vec3) -> f32 {
        (origin - self.position).length()
    }

    /// `(gain, pan)` for a mono 3D source emitted at `origin`.
    pub fn spatialize(&self, origin: Vec3) -> (f32, f32) {
        let to_source = origin - self.position;
        (
            distance_gain(to_source.length()),
            azimuth_pan(self.forward, to_source),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gain_is_unity_at_and_within_min_distance() {
        assert_eq!(distance_gain(0.0), 1.0);
        assert_eq!(distance_gain(5.0), 1.0);
        assert_eq!(distance_gain(MIN_DISTANCE), 1.0);
    }

    #[test]
    fn gain_rolls_off_as_ten_over_distance_in_band() {
        assert!((distance_gain(20.0) - 0.5).abs() < 1e-6);
        assert!((distance_gain(50.0) - 0.2).abs() < 1e-6);
        // d == 100 is the far edge of the rolloff band, still audible at 0.1.
        assert!((distance_gain(MAX_DISTANCE) - 0.1).abs() < 1e-6);
    }

    #[test]
    fn gain_hard_mutes_beyond_max_distance() {
        assert_eq!(distance_gain(100.001), 0.0);
        assert_eq!(distance_gain(150.0), 0.0);
        assert_eq!(distance_gain(1000.0), 0.0);
    }

    #[test]
    fn pan_full_right_when_source_is_on_listener_right_facing_x() {
        // Facing +X, LH right axis = Z × X = +Y. A source due +Y pans hard right.
        let pan = azimuth_pan(Vec3::X, Vec3::Y);
        assert!((pan - 1.0).abs() < 1e-6, "pan = {pan}");
    }

    #[test]
    fn pan_full_left_when_source_is_on_listener_left_facing_x() {
        // Facing +X, a source due -Y pans hard left.
        let pan = azimuth_pan(Vec3::X, -Vec3::Y);
        assert!((pan + 1.0).abs() < 1e-6, "pan = {pan}");
    }

    #[test]
    fn pan_is_centered_directly_ahead_and_behind() {
        // Source straight ahead (along forward) — right component is zero.
        assert!(azimuth_pan(Vec3::X, Vec3::new(5.0, 0.0, 0.0)).abs() < 1e-6);
        // Source straight behind — also centered (front/back ambiguity).
        assert!(azimuth_pan(Vec3::X, Vec3::new(-5.0, 0.0, 0.0)).abs() < 1e-6);
    }

    #[test]
    fn pan_left_axis_flips_with_listener_heading() {
        // PARITY: DS3D is left-handed. Facing +Y, right = Z × Y = -X, so a
        // source at +X pans LEFT (not right as a right-handed intuition would
        // suggest). This inversion is faithful to the original.
        let pan = azimuth_pan(Vec3::Y, Vec3::X);
        assert!((pan + 1.0).abs() < 1e-6, "pan = {pan}");
    }

    #[test]
    fn pan_azimuth_known_diagonal() {
        // Facing +X (right = +Y), source at 45° between forward and right:
        // to_source = (1, 1, 0) normalized => pan = 1/sqrt(2).
        let pan = azimuth_pan(Vec3::X, Vec3::new(1.0, 1.0, 0.0));
        assert!((pan - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-6, "pan = {pan}");
    }

    #[test]
    fn zero_vectors_pan_to_center() {
        assert_eq!(azimuth_pan(Vec3::X, Vec3::ZERO), 0.0);
        assert_eq!(azimuth_pan(Vec3::ZERO, Vec3::X), 0.0);
    }

    #[test]
    fn listener_from_pose_matches_ds3d_orientation() {
        let l = AudioListener::from_pose(Vec2::new(3.0, 4.0), 0.0);
        assert_eq!(l.position, Vec3::new(3.0, 4.0, 0.0));
        assert_eq!(l.forward, Vec3::X);
        let (gain, pan) = l.spatialize(Vec3::new(3.0, 5.0, 0.0)); // 1 unit due +Y
        assert_eq!(gain, 1.0); // within min distance
        assert!((pan - 1.0).abs() < 1e-6); // hard right
    }
}
