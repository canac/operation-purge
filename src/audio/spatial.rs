//! Manual DS3D spatial model: gain/pan math in the sim's LH Z-up frame,
//! replacing `IDirectSound3DListener`/`IDirectSound3DBuffer`
//! (design §5.4). Body lands at M10.
//!
//! PARITY: unity gain within min distance 10; `10/d` inverse-distance
//! rolloff from 10..100; **hard mute beyond 100**; no doppler (the
//! original never set source velocities); listener updated every 10th
//! frame (design §7 constants table).

/// Minimum distance: unity gain within this radius.
pub const MIN_DISTANCE: f32 = 10.0;
/// Maximum distance: hard mute beyond this radius.
pub const MAX_DISTANCE: f32 = 100.0;
/// Listener position/orientation refresh cadence, in frames.
pub const LISTENER_UPDATE_INTERVAL_FRAMES: u32 = 10;

/// Inverse-distance rolloff with unity gain inside `MIN_DISTANCE` and
/// hard mute beyond `MAX_DISTANCE`. Body lands at M10.
pub fn distance_gain(_distance: f32) -> f32 {
    todo!("ported at M10 — 10/d rolloff, mute beyond 100 (design §5.4)")
}

/// Stereo pan from listener-relative azimuth in the LH Z-up frame,
/// applied via kira's tweened `set_panning` (no per-frame crackle).
/// Body lands at M10.
pub fn azimuth_pan(_listener_forward: glam::Vec3, _to_source: glam::Vec3) -> f32 {
    todo!("ported at M10 — LH Z-up azimuth pan (design §3.2, §5.4)")
}
