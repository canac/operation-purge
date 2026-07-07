//! Spawn scheduling data: per-team `tSpawnInfo` timing state and the pool
//! of candidate center positions (`cOperationPurge.cpp` sim half,
//! source-map §3.4, §7). The scheduling *orchestration* (which advances
//! these, samples placements, and inserts robots/centers) lives in
//! `sim::world` because it needs the robot/center maps + RNG.
//!
//! PARITY: rejection sampling caps at **1000** tries
//! (`cOperationPurge.cpp:2360`); unspawned robots still count as alive for
//! win/lose purposes (`GetGameStatus`, `cOperationPurge.cpp:454-472`).

use crate::sim::center::Orientation;
use glam::Vec2;

/// Rejection-sampling cap for spawn-point placement (`cOperationPurge.cpp:2360`).
pub const SPAWN_PLACEMENT_TRIES: u32 = 1000;

/// `tSpawnInfo` (`cOperationPurge.h`), one per team definition.
#[derive(Clone, Debug)]
pub struct SpawnInfo {
    /// `bFinishedSpawning` — the last robot of the last group has spawned.
    pub finished_spawning: bool,
    /// `fNextSpawn` — countdown to the next spawn (seconds).
    pub next_spawn: f32,
    /// `iCurrentGroup` — index into the team's robot-group list.
    pub current_group: usize,
    /// `nRobotsLeft` — robots remaining to spawn in the current group.
    pub robots_left: u32,
    /// `pt2Spawn` — the fixed per-team spawn point (for `spawn_from_point`).
    pub spawn_point: Vec2,
}

/// `tCenterPosition` (`cOperationPurge.h`) — a candidate wall-adjacent
/// center slot, before the per-center inset offset is applied.
#[derive(Copy, Clone, Debug)]
pub struct CenterPosition {
    /// The room's lower-left corner in world units (`WallX*roomSize`,
    /// `WallY*roomSize`), i.e. `ChosenPosition.pt2Center` pre-offset.
    pub position: Vec2,
    pub orientation: Orientation,
}
