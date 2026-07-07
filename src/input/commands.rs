//! The 36-entry command enum, in `eCommand` order (design §5.2; source-map
//! §4.1 "Command index enum"), plus per-projectile-type `FireCommand`s
//! which live alongside `tProjectileType` in `data/mod.rs` rather than
//! here (they're data, not part of the fixed 36).
//!
//! PARITY: order is the index space `UIOptions.vCommands`/`[ui].commands`
//! binds against (design §4.1 schema) — reordering this enum changes
//! which physical key a `GameData.toml` entry resolves to.

/// One of the 36 fixed (non-per-projectile) commands.
///
/// PARITY: variant order matches `cGameData::s_aszCommandsNames`
/// (GameEditor/cGameData.cpp:17-54) exactly, so `Command as usize` is a
/// valid index into `COMMAND_ORDER` / the `[ui].commands` array.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum Command {
    Restart,
    EndGame,
    ToggleMap,
    ToggleProfilerRunning,
    ToggleProfilerVisible,
    ToggleProfilerInteraction,
    IncreaseBuildingWidth,
    IncreaseBuildingHeight,
    DecreaseBuildingWidth,
    DecreaseBuildingHeight,
    MoveBuildingRight,
    MoveBuildingDown,
    MoveBuildingLeft,
    MoveBuildingUp,
    Info,
    SelectObserverRobot,
    SelectObserverPosition,
    Enter2DMode,
    Exit2DMode,
    SetObserver,
    MoveObserverForwards,
    MoveObserverBackwards,
    MoveObserverRight,
    MoveObserverLeft,
    MoveObserverUp,
    MoveObserverDown,
    MoveObserverSpeed1,
    MoveObserverSpeed2,
    MoveObserverSpeed3,
    IncreaseGameSpeed,
    DecreaseGameSpeed,
    Pause,
    ToggleAIMode,
    Accelerate,
    Decelerate,
    FireProjectiles,
}

/// Fixed iteration order — the same order `[ui].commands` in
/// `GameData.toml` lists its 36 entries in (design §4.1). PARITY:
/// verbatim `s_aszCommandsNames` order (cGameData.cpp:17-54).
pub const COMMAND_ORDER: [Command; 36] = [
    Command::Restart,
    Command::EndGame,
    Command::ToggleMap,
    Command::ToggleProfilerRunning,
    Command::ToggleProfilerVisible,
    Command::ToggleProfilerInteraction,
    Command::IncreaseBuildingWidth,
    Command::IncreaseBuildingHeight,
    Command::DecreaseBuildingWidth,
    Command::DecreaseBuildingHeight,
    Command::MoveBuildingRight,
    Command::MoveBuildingDown,
    Command::MoveBuildingLeft,
    Command::MoveBuildingUp,
    Command::Info,
    Command::SelectObserverRobot,
    Command::SelectObserverPosition,
    Command::Enter2DMode,
    Command::Exit2DMode,
    Command::SetObserver,
    Command::MoveObserverForwards,
    Command::MoveObserverBackwards,
    Command::MoveObserverRight,
    Command::MoveObserverLeft,
    Command::MoveObserverUp,
    Command::MoveObserverDown,
    Command::MoveObserverSpeed1,
    Command::MoveObserverSpeed2,
    Command::MoveObserverSpeed3,
    Command::IncreaseGameSpeed,
    Command::DecreaseGameSpeed,
    Command::Pause,
    Command::ToggleAIMode,
    Command::Accelerate,
    Command::Decelerate,
    Command::FireProjectiles,
];

/// A binding: key + modifier requirements. PARITY: modifiers test
/// *either*-side keys (design §5.2 "modifier flags bControl/bAlt/bShift
/// test either-side keys").
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct CommandBinding {
    pub key: crate::input::keys::Key,
    pub control: bool,
    pub alt: bool,
    pub shift: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_order_has_exactly_36_entries() {
        assert_eq!(COMMAND_ORDER.len(), 36);
    }

    // PARITY: pins the eCommand ABI (cGameData.cpp:17-54). Index N here
    // must equal s_aszCommandsNames[N] or GameData.toml [ui].commands
    // bindings resolve to the wrong command.
    #[test]
    fn command_order_matches_ecommand_abi() {
        assert_eq!(COMMAND_ORDER[0], Command::Restart);
        assert_eq!(COMMAND_ORDER[1], Command::EndGame);
        assert_eq!(COMMAND_ORDER[2], Command::ToggleMap);
        assert_eq!(COMMAND_ORDER[14], Command::Info);
        assert_eq!(COMMAND_ORDER[17], Command::Enter2DMode);
        assert_eq!(COMMAND_ORDER[35], Command::FireProjectiles);
    }
}
