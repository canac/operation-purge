//! `Objects.txt` / `LightData.txt` → compiled-in constants.
//!
//! These two tiny fixed-value text files are hardcoded as
//! Rust `const`s instead of being read at runtime — no parser, no file,
//! no missing-file failure path.
//!
//! Both files were recovered from `OperationPurge.msi` (AppData payload) and
//! these constants are the **verified originals**, transcribed from the
//! shipped `Objects.txt` / `LightData.txt`. They override the divergent
//! hardcoded fallbacks in `cOperationPurge.cpp:2153-2199` (which the running
//! game replaced by reading `Objects.txt`).

/// Robot/projectile procedural-geometry dimensions.
///
/// PARITY: verified 11-value record, `cOperationPurge.cpp:2153-2199` (+
/// its commented-out defaults). Consumed by `render::robot3d`.
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

/// Verified originals from the shipped `Objects.txt` (recovered from
/// `OperationPurge.msi`). Note `barrel_resolution` (12) and
/// `projectile_resolution` (16) differ from the `cOperationPurge.cpp:2153-2199`
/// hardcoded fallbacks (8 and 10) — the game read `Objects.txt` at runtime,
/// so these file values are what actually shipped.
pub const OBJECTS: Robot3DDimensions = Robot3DDimensions {
    body_resolution: 12,
    barrel_resolution: 12,
    tread_resolution: 8,
    body_radius: 0.5,
    body_height: 0.5,
    barrel_radius: 0.08,
    barrel_length: 0.8,
    tread_radius: 0.2,
    tread_width: 0.3,
    tread_length: 0.6,
    projectile_resolution: 16,
};

/// Point-light attenuation coefficients: `att = 1 / (a0 + a1*d + a2*d^2)`.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct LightAttenuation {
    pub a0: f32,
    pub a1: f32,
    pub a2: f32,
}

/// Verified originals from the shipped `LightData.txt` (recovered from
/// `OperationPurge.msi`): `att = 1 / (0.5 + 0.25*d)`.
pub const LIGHT_DATA: LightAttenuation = LightAttenuation {
    a0: 0.5,
    a1: 0.25,
    a2: 0.0,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn objects_values_are_pinned() {
        // Verified against the recovered Objects.txt.
        assert_eq!(OBJECTS.body_resolution, 12);
        assert_eq!(OBJECTS.barrel_resolution, 12);
        assert_eq!(OBJECTS.tread_resolution, 8);
        assert_eq!(OBJECTS.body_radius, 0.5);
        assert_eq!(OBJECTS.body_height, 0.5);
        assert_eq!(OBJECTS.barrel_radius, 0.08);
        assert_eq!(OBJECTS.barrel_length, 0.8);
        assert_eq!(OBJECTS.tread_radius, 0.2);
        assert_eq!(OBJECTS.tread_width, 0.3);
        assert_eq!(OBJECTS.tread_length, 0.6);
        assert_eq!(OBJECTS.projectile_resolution, 16);
    }

    #[test]
    fn light_data_values_are_pinned() {
        // Verified against the recovered LightData.txt.
        assert_eq!(LIGHT_DATA.a0, 0.5);
        assert_eq!(LIGHT_DATA.a1, 0.25);
        assert_eq!(LIGHT_DATA.a2, 0.0);
    }
}
