//! Status text assembly (design §5.3), replacing `cOverlay2D`'s HUD text
//! half. Full body lands at M5 (static layout) / M8 (live wiring).
//!
//! PARITY: enemy-visible flag refreshed every 0.25s of *game* time
//! (`cOperationPurge.cpp:586`); EnemyVisible alarm outranks Targeted —
//! code over docs (`cOperationPurge.cpp:1318-1340`, design §1.3).

/// Enemy-visible HUD/alarm poll interval, in game-time seconds.
pub const ENEMY_VISIBLE_POLL_SECONDS: f32 = 0.25;

/// Assembles the status text block: Speed x/max, Desired speed, Energy,
/// Health, per-type Reload + Ammunition, Type, Team, per-team Robots
/// left, Projectiles, Game speed %, elapsed seconds, plus conditional
/// lines (AI mode / targeted / enemy visible / above center). Body lands
/// at M5/M8.
pub fn status_text() -> String {
    todo!("ported at M5/M8 — HUD status block assembly (design §5.3)")
}
