//! `Game`: owns the `World` + render objects + camera + speed + timers
//! (design §2), replacing `cOperationPurge`'s app half (source-map §3.4).
//! Full wiring lands at M8.

pub mod hud;
pub mod newgame;
pub mod screens;

use crate::sim::world::World;
use screens::Screen;

/// PARITY: game speed starts paused at `0.0` after robot selection
/// (`cOperationPurge.cpp:14`); `T`/`G` ramp by `fGameSpeedRate * dt`, `Q`
/// toggles pause, no upper bound (design §5.1).
pub const INITIAL_GAME_SPEED: f32 = 0.0;

pub struct Game {
    pub world: World,
    pub screen: Screen,
    pub game_speed: f32,
    /// Elapsed *game* time, shown in the HUD status block (design §5.3).
    pub elapsed_seconds: f32,
}

impl Game {
    /// Advances the game by `real_dt` (already gated on window focus by
    /// the caller, design §5.1). `world.frame` receives `real_dt *
    /// game_speed`. Body lands at M8.
    pub fn tick(&mut self, _real_dt: f32) {
        todo!("ported at M8 — Game::tick wiring (design §5.1)")
    }
}
