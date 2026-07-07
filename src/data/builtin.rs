//! `Objects.txt` / `LightData.txt` → compiled-in constants (design §4.3).
//!
//! User decision: these two tiny fixed-value text files are hardcoded as
//! Rust `const`s instead of being read at runtime — no parser, no file,
//! no missing-file failure path.

/// Robot/projectile procedural-geometry dimensions.
///
/// PARITY: verified 11-value record, `cOperationPurge.cpp:2153-2199` (+
/// its commented-out defaults). Consumed by `render::robot3d` at M5.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Robot3DDimensions {
    pub body_resolution: u32,
    pub barrel_resolution: u32,
    pub tread_resolution: u32,
    pub body_radius: f32,
    pub body_height: f32,
    pub barrel_radius: f32,
    pub barrel_length: f32,
    pub tread_radius: f32,
    pub tread_width: f32,
    pub tread_length: f32,
    pub projectile_resolution: u32,
}

/// `// ` cOperationPurge.cpp:2153-2199 — Objects.txt defaults (file is
/// lost; these are the game's own hardcoded fallback values, not a
/// reconstruction).
pub const OBJECTS: Robot3DDimensions = Robot3DDimensions {
    body_resolution: 12,
    barrel_resolution: 8,
    tread_resolution: 8,
    body_radius: 0.5,
    body_height: 0.5,
    barrel_radius: 0.08,
    barrel_length: 0.8,
    tread_radius: 0.2,
    tread_width: 0.3,
    tread_length: 0.6,
    projectile_resolution: 10,
};

/// Point-light attenuation coefficients: `att = 1 / (a0 + a1*d + a2*d^2)`.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct LightAttenuation {
    pub a0: f32,
    pub a1: f32,
    pub a2: f32,
}

/// `// ` `LightData.txt` was lost entirely; these values are
/// **reconstructed** (design §4.3) and tuned against help-doc screenshots
/// at M5 — labeled reconstructed here and in the README, not verified
/// originals.
pub const LIGHT_DATA: LightAttenuation = LightAttenuation {
    a0: 1.0,
    a1: 0.02,
    a2: 0.0,
};

#[cfg(test)]
mod tests {
    use super::*;

    /// Pins the exact compiled-in values so an accidental edit is caught
    /// (design §4.3).
    #[test]
    fn objects_values_are_pinned() {
        assert_eq!(OBJECTS.body_resolution, 12);
        assert_eq!(OBJECTS.barrel_resolution, 8);
        assert_eq!(OBJECTS.tread_resolution, 8);
        assert_eq!(OBJECTS.body_radius, 0.5);
        assert_eq!(OBJECTS.body_height, 0.5);
        assert_eq!(OBJECTS.barrel_radius, 0.08);
        assert_eq!(OBJECTS.barrel_length, 0.8);
        assert_eq!(OBJECTS.tread_radius, 0.2);
        assert_eq!(OBJECTS.tread_width, 0.3);
        assert_eq!(OBJECTS.tread_length, 0.6);
        assert_eq!(OBJECTS.projectile_resolution, 10);
    }

    #[test]
    fn light_data_values_are_pinned() {
        assert_eq!(LIGHT_DATA.a0, 1.0);
        assert_eq!(LIGHT_DATA.a1, 0.02);
        assert_eq!(LIGHT_DATA.a2, 0.0);
    }
}
