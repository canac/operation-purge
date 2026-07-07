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

impl Command {
    /// Position of this command in the fixed `eCommand` index space
    /// (`COMMAND_ORDER`) — the same index `[ui].commands` binds against.
    pub fn index(self) -> usize {
        COMMAND_ORDER
            .iter()
            .position(|&c| c == self)
            .expect("every Command variant is in COMMAND_ORDER")
    }
}

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

impl CommandBinding {
    /// Lift a data-layer `TCommand` (already key-resolved from its
    /// display-name string) into an input binding.
    pub fn from_tcommand(cmd: &crate::data::TCommand) -> Self {
        Self {
            key: cmd.key,
            control: cmd.control,
            alt: cmd.alt,
            shift: cmd.shift,
        }
    }

    /// PARITY (`cOperationPurge::GetCommandState`, cOperationPurge.cpp:3116-3144
    /// with the verified `bExactModifiers = false`): a *required* modifier
    /// must be held (either side); a *non-required* modifier is ignored
    /// (holding Shift never suppresses a command that doesn't ask for
    /// Shift). Formally, for each modifier m: `!required_m || down_m`.
    pub fn modifiers_satisfied(&self, input: &super::Input) -> bool {
        (!self.control || input.control_down())
            && (!self.alt || input.alt_down())
            && (!self.shift || input.shift_down())
    }

    /// `bDown`: the bound key is down AND the required modifiers are held.
    pub fn is_down(&self, input: &super::Input) -> bool {
        input.is_down(self.key) && self.modifiers_satisfied(input)
    }

    /// `bPressed`: the bound key edged down this frame AND modifiers hold.
    pub fn just_pressed(&self, input: &super::Input) -> bool {
        input.just_pressed(self.key) && self.modifiers_satisfied(input)
    }

    /// `bReleased`: the bound key edged up this frame AND modifiers hold.
    pub fn just_released(&self, input: &super::Input) -> bool {
        input.just_released(self.key) && self.modifiers_satisfied(input)
    }
}

/// The full resolved binding set: the 36 fixed commands (in `COMMAND_ORDER`
/// index space) plus one fire binding per projectile type
/// (`ProjectileType.fire_command`), loaded from `GameData` (design §5.2).
#[derive(Clone, Debug)]
pub struct CommandBindings {
    /// Indexed by `Command::index()` / `COMMAND_ORDER` position.
    pub commands: [CommandBinding; 36],
    /// One per `GameData.projectile_types` entry, in array order (the index
    /// space `ProjectileType` refs point into).
    pub fire_commands: Vec<CommandBinding>,
}

impl CommandBindings {
    /// Resolve all bindings from loaded game data. The 36-entry shape of
    /// `ui.commands` is already enforced at load (data/gamedata.rs); this
    /// re-checks defensively (GUARD) so a mismatch is a clean error, not a
    /// panic on `try_into`.
    pub fn from_game_data(data: &crate::data::GameData) -> Result<Self, crate::error::GameError> {
        if data.ui.commands.len() != 36 {
            return Err(crate::error::GameError::Other(format!(
                "[ui].commands must have exactly 36 entries, found {}",
                data.ui.commands.len()
            )));
        }
        let mut commands = [CommandBinding {
            key: crate::input::keys::KEY_LAST,
            control: false,
            alt: false,
            shift: false,
        }; 36];
        for (i, tcmd) in data.ui.commands.iter().enumerate() {
            commands[i] = CommandBinding::from_tcommand(tcmd);
        }
        let fire_commands = data
            .projectile_types
            .iter()
            .map(|pt| CommandBinding::from_tcommand(&pt.fire_command))
            .collect();
        Ok(Self {
            commands,
            fire_commands,
        })
    }

    /// The binding for a fixed command.
    pub fn binding(&self, command: Command) -> &CommandBinding {
        &self.commands[command.index()]
    }
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

    #[test]
    fn command_index_is_the_inverse_of_command_order() {
        for (i, &cmd) in COMMAND_ORDER.iter().enumerate() {
            assert_eq!(cmd.index(), i);
        }
    }

    use crate::input::keys::Key;
    use crate::input::Input;

    fn binding(key: Key, control: bool, alt: bool, shift: bool) -> CommandBinding {
        CommandBinding {
            key,
            control,
            alt,
            shift,
        }
    }

    #[test]
    fn plain_binding_ignores_extra_held_modifiers() {
        // PARITY: bExactModifiers = false — a command with no required
        // modifier still fires while Shift/Ctrl/Alt happen to be down.
        let b = binding(Key::KeyF, false, false, false);
        let mut input = Input::new();
        input.on_key_event(Key::KeyF, true);
        input.on_key_event(Key::ShiftLeft, true);
        input.on_key_event(Key::ControlRight, true);
        input.begin_frame();
        assert!(b.is_down(&input));
    }

    #[test]
    fn required_modifier_must_be_held_either_side() {
        let b = binding(Key::KeyR, true, false, false); // Ctrl+R
        let mut input = Input::new();
        input.on_key_event(Key::KeyR, true);
        input.begin_frame();
        assert!(!b.is_down(&input), "no Ctrl held ⇒ inactive");

        // Right control satisfies a Ctrl requirement (either-side).
        input.on_key_event(Key::ControlRight, true);
        input.begin_frame();
        assert!(b.is_down(&input));
    }

    #[test]
    fn pressed_and_released_edges_respect_modifiers() {
        let b = binding(Key::KeyP, false, false, true); // Shift+P
        let mut input = Input::new();
        input.begin_frame(); // consume the first-frame edge suppression (FIX)
        input.on_key_event(Key::ShiftLeft, true);
        input.on_key_event(Key::KeyP, true);
        input.begin_frame();
        assert!(b.just_pressed(&input));
        assert!(b.is_down(&input));
        // Hold: no repeated press edge.
        input.begin_frame();
        assert!(!b.just_pressed(&input));
        // Release the key.
        input.on_key_event(Key::KeyP, false);
        input.begin_frame();
        assert!(b.just_released(&input));
    }

    #[test]
    fn bindings_resolve_from_the_shipped_game_data() {
        let path = std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/GameData.toml"
        ));
        let data = crate::data::gamedata::load(path).expect("shipped GameData.toml parses");

        let bindings = CommandBindings::from_game_data(&data).expect("36 commands resolve");
        assert_eq!(bindings.commands.len(), 36);
        assert_eq!(bindings.fire_commands.len(), data.projectile_types.len());

        // Every fixed command's binding is exactly the ui.commands entry at
        // the same index — the whole point of the index-space ABI.
        for (i, tcmd) in data.ui.commands.iter().enumerate() {
            assert_eq!(bindings.commands[i], CommandBinding::from_tcommand(tcmd));
            assert_eq!(bindings.binding(COMMAND_ORDER[i]).key, tcmd.key);
        }
    }
}
